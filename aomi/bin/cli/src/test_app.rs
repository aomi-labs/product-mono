use crate::printer::split_system_events;
use crate::session::CliSession;
use aomi_chat::{CoreAppBuilder, SystemEvent, SystemEventQueue};
use aomi_tools::test_utils::{MockMultiStepTool, MockSingleTool, register_mock_multi_step_tool};
use eyre::Result;
use futures::StreamExt;
use rig::tool::Tool;
use serde_json::{Value, json};

/// Build a CoreAppBuilder and exercise tool scheduling paths (single + multi-step),
/// plus inline/async system event fan-out.
#[tokio::test(flavor = "multi_thread")]
async fn test_app_builder_covers_tool_and_system_paths() -> Result<()> {
    let system_events = SystemEventQueue::new();
    let mut builder = CoreAppBuilder::new_for_tests(Some(&system_events)).await?;

    // Register tools across both single and multi-step paths using shared test mocks.
    builder.add_tool(MockSingleTool)?;
    let scheduler = builder.scheduler_for_tests();
    register_mock_multi_step_tool(
        &scheduler,
        Some(MockMultiStepTool::default().with_error_at(2)),
    );

    // Single tool should round-trip via oneshot channel.
    let mut handler = scheduler.get_handler();
    let call_id = "single_1".to_string();
    let payload = json!({ "input": "hello" });
    handler
        .request(MockSingleTool::NAME.to_string(), payload, call_id.clone())
        .await;
    let mut ui_stream = handler.resolve_last_call().expect("stream for single tool");
    let (_id, value) = ui_stream.next().await.expect("single tool yields");
    let value = value.map_err(|e: String| eyre::eyre!(e))?;
    let parsed: Value = serde_json::from_str(value.as_str().unwrap())?;
    assert_eq!(parsed.get("result").and_then(Value::as_str), Some("single"));

    // Multi-step tool: first chunk surfaces via UI stream, remaining via handler poll.
    let mut handler = scheduler.get_handler();
    handler
        .request(
            "mock_multi_step".to_string(),
            json!({ "input": "world" }),
            "multi_1".to_string(),
        )
        .await;
    let mut ui_stream = handler.resolve_last_call().expect("stream for multi tool");

    let (chunk_call_id, first_result) = ui_stream.next().await.expect("first chunk");
    assert_eq!(chunk_call_id, "multi_1");
    let first_chunk = first_result.map_err(|e: String| eyre::eyre!(e))?;
    assert_eq!(first_chunk.get("step").and_then(Value::as_i64), Some(1));

    // Collect remaining chunks via poll_streams_to_next_result
    let mut results = Vec::new();
    while let Some(completion) = handler.poll_streams().await {
        results.push(completion.result);
    }
    assert_eq!(
        results.len(),
        3,
        "fanout should include first chunk plus remaining"
    );
    assert_eq!(
        results[0]
            .as_ref()
            .ok()
            .and_then(|v| v.get("step"))
            .and_then(Value::as_i64),
        Some(1)
    );
    assert_eq!(
        results[1]
            .as_ref()
            .ok()
            .and_then(|v| v.get("step"))
            .and_then(Value::as_i64),
        Some(2)
    );
    assert!(
        results[2].is_err(),
        "final chunk should surface the stream error"
    );

    // Exercise system event fan-out (inline + async).
    system_events.push(SystemEvent::InlineDisplay(json!({"type": "test_inline"})));
    system_events.push(SystemEvent::SystemNotice("test_notice".to_string()));
    system_events.push(SystemEvent::SystemError("test_error".to_string()));
    system_events.push(SystemEvent::AsyncUpdate(json!({"type": "async_update"})));
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
            .any(|e| matches!(e, SystemEvent::AsyncUpdate(_))),
        "async update surfaced"
    );

    Ok(())
}

#[tokio::test]
async fn test_cli_session_routes_system_events_into_buckets() -> Result<()> {
    use crate::test_backend::TestBackend;
    use aomi_backend::{BackendType, session::BackendwithTool};
    use std::{collections::HashMap, sync::Arc};

    let backend: Arc<BackendwithTool> = Arc::new(
        TestBackend::new()
            .await
            .map_err(|e| eyre::eyre!(e.to_string()))?,
    );
    let mut backends: HashMap<BackendType, Arc<BackendwithTool>> = HashMap::new();
    backends.insert(BackendType::Forge, backend);

    let mut session = CliSession::new(Arc::new(backends), BackendType::Forge)
        .await
        .map_err(|e| eyre::eyre!(e.to_string()))?;

    // Drain initial "Backend connected" notices etc.
    session.sync_state().await;
    let _ = session.advance_frontend_events();

    session.push_system_event(SystemEvent::InlineDisplay(json!({"type": "test_inline"})));
    session.push_system_event(SystemEvent::SystemNotice("notice".to_string()));
    session.push_system_event(SystemEvent::SystemError("error".to_string()));
    session.push_system_event(SystemEvent::AsyncUpdate(json!({"type": "test_async"})));

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
        "AsyncUpdate payload should surface in system events"
    );

    Ok(())
}
