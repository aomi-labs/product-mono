use crate::printer::split_system_events;
use crate::session::CliSession;
use aomi_core::{CoreAppBuilder, SystemEvent, SystemEventQueue};
use aomi_tools::test_utils::{MockAsyncTool, MockSingleTool};
use aomi_tools::{CallMetadata, ToolReciever};
use eyre::Result;
use serde_json::{Value, json};

/// Test app lifecycle with mock tools: register tools, poll scheduler, verify sync ack and async callbacks
#[tokio::test(flavor = "multi_thread")]
async fn test_app_lifecycle_with_mock_tools() -> Result<()> {
    let system_events = SystemEventQueue::new();
    let mut builder = CoreAppBuilder::new_for_tests(Some(&system_events)).await?;

    // Register mock tools using the metadata() interface
    builder.add_tool(MockSingleTool)?;
    builder.add_tool(MockAsyncTool::default())?;

    let scheduler = builder.scheduler();

    // Get session handler
    let handler =
        scheduler.get_session_handler("test_session".to_string(), vec!["default".to_string()]);

    // === Test sync tool (single result via oneshot) ===
    let sync_metadata = CallMetadata::new(
        "mock_single".to_string(),
        "default".to_string(),
        "sync_1".to_string(),
        None,
        false,
    );

    let (sync_tx, sync_rx) = tokio::sync::oneshot::channel();
    let _ = sync_tx.send(Ok(json!({ "sync_result": "ack" })));

    {
        let mut guard = handler.lock().await;
        guard.register_receiver(ToolReciever::new_single(sync_metadata.clone(), sync_rx));

        // Poll once - sync tool should complete immediately
        let count = guard.poll_once();
        assert_eq!(count, 1, "sync tool should complete in one poll");

        let completed = guard.take_completed_calls();
        assert_eq!(completed.len(), 1);

        let result = completed[0].result.as_ref().unwrap();
        assert_eq!(
            result.get("sync_result").and_then(|v| v.as_str()),
            Some("ack")
        );
    }

    // === Test async tool (multiple results via mpsc) ===
    let async_metadata = CallMetadata::new(
        "mock_async".to_string(),
        "default".to_string(),
        "async_1".to_string(),
        Some("llm_call_id".to_string()),
        true,
    );

    let (async_tx, async_rx) = tokio::sync::mpsc::channel(4);

    // Spawn background task that sends multiple callbacks
    tokio::spawn(async move {
        // First callback - immediate ack
        let _ = async_tx
            .send(Ok(json!({ "status": "started", "progress": 0 })))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        // Second callback - progress update
        let _ = async_tx
            .send(Ok(json!({ "status": "in_progress", "progress": 50 })))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        // Final callback - completion
        let _ = async_tx
            .send(Ok(json!({ "status": "completed", "progress": 100 })))
            .await;
    });

    {
        let mut guard = handler.lock().await;
        guard.register_receiver(ToolReciever::new_async(async_metadata.clone(), async_rx));

        // Poll until we get all 3 results
        let mut all_results = Vec::new();
        for _ in 0..20 {
            guard.poll_once();
            all_results.extend(guard.take_completed_calls());

            if all_results.len() >= 3 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert_eq!(all_results.len(), 3, "should receive all 3 async callbacks");

        // Verify we got all progress values
        let progress_values: Vec<i64> = all_results
            .iter()
            .filter_map(|c| c.result.as_ref().ok())
            .filter_map(|v| v.get("progress").and_then(|p| p.as_i64()))
            .collect();

        assert!(
            progress_values.contains(&0),
            "should have initial ack (progress=0)"
        );
        assert!(
            progress_values.contains(&50),
            "should have progress update (progress=50)"
        );
        assert!(
            progress_values.contains(&100),
            "should have completion (progress=100)"
        );
    }

    Ok(())
}

/// Test CliSession routes system events into correct buckets
#[tokio::test]
async fn test_cli_session_routes_system_events_into_buckets() -> Result<()> {
    use crate::test_backend::TestSchedulerBackend;
    use aomi_backend::{Namespace, session::AomiBackend};
    use std::{collections::HashMap, sync::Arc};

    let backend: Arc<AomiBackend> = Arc::new(
        TestSchedulerBackend::new()
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
