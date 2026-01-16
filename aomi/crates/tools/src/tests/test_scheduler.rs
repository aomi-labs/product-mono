use crate::scheduler::ToolScheduler;
use crate::streams::ToolReciever;
use crate::CallMetadata;
use crate::test_utils::{MockSingleTool, MockAsyncTool};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

/// Test that tools registered via register_tool() are properly filtered by namespace
#[tokio::test(flavor = "multi_thread")]
async fn test_session_handler_metadata_filter() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();

    // Register tools using metadata() interface
    scheduler.register_tool(&MockSingleTool).unwrap();
    scheduler.register_tool(&MockAsyncTool::default()).unwrap();

    // Get handler for "default" namespace only
    let handler = scheduler.get_session_handler(
        "session_a".to_string(),
        vec!["default".to_string()],
    );
    let guard = handler.lock().await;

    // MockSingleTool is sync, MockAsyncTool is async
    assert!(!guard.is_async("mock_single"));
    assert!(guard.is_async("mock_async"));

    // Both should have descriptions from their metadata()
    assert_eq!(guard.get_description("mock_single"), "Mock single-result tool for testing");
    assert_eq!(guard.get_description("mock_async"), "Mock multi-step tool for scheduler tests");
}

/// Test single-result tool with poll_once()
#[tokio::test(flavor = "multi_thread")]
async fn test_handler_single_receiver() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    let handler = scheduler.get_session_handler(
        "session_single".to_string(),
        vec!["default".to_string()],
    );

    let metadata = CallMetadata::new(
        "mock_single".to_string(),
        "default".to_string(),
        "single_1".to_string(),
        None,
        false,
    );

    // Create channel and send result immediately
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(Ok(json!({ "ok": true })));

    let mut guard = handler.lock().await;
    guard.register_receiver(ToolReciever::new_single(metadata.clone(), rx));

    // Single poll should complete the call
    let count = guard.poll_once();
    assert_eq!(count, 1);

    let completed = guard.take_completed_calls();
    assert_eq!(completed.len(), 1);

    let completion = &completed[0];
    assert_eq!(completion.metadata, metadata);
    assert_eq!(
        completion.result.as_ref().unwrap().get("ok").and_then(|v| v.as_bool()),
        Some(true)
    );

    // No more ongoing calls
    assert!(!guard.has_ongoing_calls());
}

/// Test async/streaming tool with poll_once()
#[tokio::test(flavor = "multi_thread")]
async fn test_handler_async_receiver() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    let handler = scheduler.get_session_handler(
        "session_async".to_string(),
        vec!["default".to_string()],
    );

    let metadata = CallMetadata::new(
        "mock_async".to_string(),
        "default".to_string(),
        "async_1".to_string(),
        None,
        true,
    );

    let (tx, rx) = mpsc::channel(4);

    // Spawn sender that sends multiple chunks
    tokio::spawn(async move {
        let _ = tx.send(Ok(json!({ "step": 1 }))).await;
        let _ = tx.send(Ok(json!({ "step": 2 }))).await;
    });

    let mut guard = handler.lock().await;
    guard.register_receiver(ToolReciever::new_async(metadata.clone(), rx));

    // Poll until we get both results
    let mut all_results = Vec::new();
    for _ in 0..10 {
        guard.poll_once();
        all_results.extend(guard.take_completed_calls());

        if all_results.len() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert_eq!(all_results.len(), 2, "should receive both steps");

    let steps: Vec<i64> = all_results
        .iter()
        .filter_map(|c| c.result.as_ref().ok())
        .filter_map(|v| v.get("step").and_then(|s| s.as_i64()))
        .collect();

    assert!(steps.contains(&1), "should have step 1");
    assert!(steps.contains(&2), "should have step 2");
}

/// Test close_ongoing_calls gracefully terminates channels
#[tokio::test(flavor = "multi_thread")]
async fn test_close_ongoing_calls() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    let handler = scheduler.get_session_handler(
        "session_close".to_string(),
        vec!["default".to_string()],
    );

    let metadata = CallMetadata::new(
        "mock_async".to_string(),
        "default".to_string(),
        "close_1".to_string(),
        None,
        true,
    );

    let (tx, rx) = mpsc::channel(4);

    let mut guard = handler.lock().await;
    guard.register_receiver(ToolReciever::new_async(metadata, rx));

    assert!(guard.has_ongoing_calls());

    // Close ongoing calls
    guard.close_ongoing_calls();

    assert!(!guard.has_ongoing_calls());

    // Sender should detect closed channel
    let send_result = tx.send(Ok(json!({ "after_close": true }))).await;
    assert!(send_result.is_err(), "send should fail after close");
}
