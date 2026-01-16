use aomi_chat::{CoreAppBuilder, SystemEvent, SystemEventQueue};
use aomi_tools::{CallMetadata, ToolReciever};
use aomi_tools::test_utils::{MockAsyncTool, MockSingleTool, register_mock_multi_step_tool};
use crate::{printer::split_system_events, session::CliSession};
use eyre::Result;
use serde_json::{Value, json};

/// Build a CoreAppBuilder and exercise tool scheduling paths (single + multi-step),
/// plus inline/async system event fan-out.
#[tokio::test(flavor = "multi_thread")]
async fn test_app_builder_covers_tool_and_system_paths() -> Result<()> {
    let system_events = SystemEventQueue::new();
    let mut builder = CoreAppBuilder::new_for_tests(Some(&system_events)).await?;

    // Register tools across both single and multi-step paths using shared test mocks.
    builder.add_aomi_tool(MockSingleTool)?;
    let scheduler = builder.scheduler_for_tests();
    register_mock_multi_step_tool(
        &scheduler,
        Some(MockAsyncTool::default().with_error_at(2)),
    );

    // Single tool should round-trip via oneshot channel.
    let handler = scheduler.get_session_handler(
        "test_session".to_string(),
        vec!["default".to_string()],
    );
    let call_id = CallMetadata::new(
        "mock_single".to_string(),
        "default".to_string(),
        "single_1".to_string(),
        None,
        false,
    );
    let payload = json!({ "input": "hello" });
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = tx.send(Ok(json!({ "result": payload })));
    let mut guard = handler.lock().await;
    guard.register_receiver(ToolReciever::new_single(call_id.clone(), rx));
    guard.poll_streams_once();
    let completed = guard.take_completed_calls();
    let value = completed
        .into_iter()
        .find_map(|completion| completion.result.ok())
        .expect("single tool completion");
    assert_eq!(value.get("result"), Some(&payload));

    // Async tool: first chunk surfaces via UI stream, remaining via handler poll.
    let handler = scheduler.get_session_handler(
        "test_session".to_string(),
        vec!["default".to_string()],
    );
    let multi_call_id = CallMetadata::new(
        "mock_multi_step".to_string(),
        "default".to_string(),
        "multi_1".to_string(),
        None,
        true,
    );
    let (multi_tx, multi_rx) = tokio::sync::mpsc::channel(4);
    tokio::spawn(async move {
        let _ = multi_tx.send(Ok(json!({ "step": 1 }))).await;
        let _ = multi_tx.send(Ok(json!({ "step": 2 }))).await;
        let _ = multi_tx.send(Err(eyre::eyre!("boom"))).await;
    });
    let mut guard = handler.lock().await;
    guard.register_receiver(ToolReciever::new_multi_step(multi_call_id.clone(), multi_rx));

    let mut results = Vec::new();
    for _ in 0..10 {
        guard.poll_streams_once();
        results.extend(guard.take_completed_calls().into_iter().map(|c| c.result));
        if !guard.has_ongoing_streams() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(results.len(), 3, "fanout should include all chunks");
    let mut steps = results
        .iter()
        .filter_map(|res| res.as_ref().ok())
        .filter_map(|value| value.get("step").and_then(Value::as_i64))
        .collect::<Vec<_>>();
    steps.sort_unstable();
    assert_eq!(steps, vec![1, 2]);
    assert!(
        results.iter().any(|res| res.is_err()),
        "final chunk should surface the stream error"
    );

    // Exercise system event fan-out (inline + async).
    system_events.push(SystemEvent::InlineDisplay(json!({"type": "test_inline"})));
    system_events.push(SystemEvent::SystemNotice("test_notice".to_string()));
    system_events.push(SystemEvent::SystemError("test_error".to_string()));
    system_events.push(SystemEvent::AsyncCallback(json!({"type": "async_update"})));
    let inline = system_events.slice_from(0);
    assert!(
        inline
            .iter()
            .any(|e| matches!(e, SystemEvent::InlineDisplay(_))),
        "inline event surfaced"
    );
    assert!(
        inline
            .iter()
            .any(|e| matches!(e, SystemEvent::SystemNotice(_))),
        "system notice surfaced"
    );
    assert!(
        inline
            .iter()
            .any(|e| matches!(e, SystemEvent::SystemError(_))),
        "system error surfaced"
    );
    assert!(
        inline
            .iter()
            .any(|e| matches!(e, SystemEvent::AsyncCallback(_))),
        "async update surfaced"
    );

    Ok(())
}

#[tokio::test]
async fn test_cli_session_routes_system_events_into_buckets() -> Result<()> {
    use crate::test_backend::TestBackend;
    use aomi_backend::{Namespace, session::AomiBackend};
    use std::{collections::HashMap, sync::Arc};

    let backend: Arc<AomiBackend> = Arc::new(
        TestBackend::new()
            .await
            .map_err(|e| eyre::eyre!(e.to_string()))?,
    );
    let mut backends: HashMap<Namespace, Arc<AomiBackend>> = HashMap::new();
    backends.insert(Namespace::Forge, backend);

    let mut session = CliSession::new(Arc::new(backends), Namespace::Forge)
        .await
        .map_err(|e| eyre::eyre!(e.to_string()))?;

    // Drain initial "Backend connected" notices etc.
    session.sync_state().await;
    let _ = session.advance_frontend_events();

    session.push_system_event(SystemEvent::InlineDisplay(json!({"type": "test_inline"})));
    session.push_system_event(SystemEvent::SystemNotice("notice".to_string()));
    session.push_system_event(SystemEvent::SystemError("error".to_string()));
    session.push_system_event(SystemEvent::AsyncCallback(json!({"type": "test_async"})));

    session.sync_state().await;
    let (inline_events, async_updates) = split_system_events(session.advance_frontend_events());

    assert!(
        inline_events
            .iter()
            .any(|e| matches!(e, SystemEvent::InlineDisplay(_))),
        "InlineDisplay should end up in active system events"
    );
    assert!(
        inline_events
            .iter()
            .any(|e| matches!(e, SystemEvent::SystemNotice(_))),
        "SystemNotice should end up in active system events"
    );
    assert!(
        inline_events
            .iter()
            .any(|e| matches!(e, SystemEvent::SystemError(_))),
        "SystemError should end up in active system events"
    );
    let buffered_async = async_updates
        .iter()
        .any(|v| v.get("type").and_then(Value::as_str) == Some("test_async"));
    assert!(
        buffered_async,
        "AsyncCallback payload should surface in system events"
    );

    Ok(())
}
