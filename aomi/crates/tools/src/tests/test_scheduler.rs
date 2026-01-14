use crate::scheduler::{ToolMetadata, ToolScheduler};
use crate::streams::ToolReciever;
use crate::CallMetadata;
use futures::StreamExt;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

#[tokio::test(flavor = "multi_thread")]
async fn test_session_handler_metadata_filter() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    scheduler
        .register_metadata(ToolMetadata::new(
            "tool_a".to_string(),
            "default".to_string(),
            "Tool A".to_string(),
            false,
        ))
        .unwrap();
    scheduler
        .register_metadata(ToolMetadata::new(
            "tool_b".to_string(),
            "other".to_string(),
            "Tool B".to_string(),
            true,
        ))
        .unwrap();

    let handler = scheduler.get_session_handler(
        "session_a".to_string(),
        vec!["default".to_string()],
    );
    let guard = handler.lock().await;
    assert!(!guard.is_multi_step("tool_a"));
    assert_eq!(guard.get_topic("tool_a"), "Tool A");
    assert_eq!(guard.get_topic("tool_b"), "tool_b");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_handler_single_receiver() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    let handler = scheduler.get_session_handler(
        "session_single".to_string(),
        vec!["default".to_string()],
    );
    let metadata = CallMetadata::new(
        "mock_single".to_string(),
        "single_1".to_string(),
        None,
        false,
    );
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(Ok(json!({ "ok": true })));

    let mut guard = handler.lock().await;
    guard.register_receiver(ToolReciever::new_single(metadata.clone(), rx));
    let mut ui_stream = guard.resolve_last_call().expect("ui stream");
    drop(guard);

    let (recv_meta, result) = ui_stream.next().await.expect("stream item");
    assert_eq!(recv_meta, metadata);
    assert_eq!(result.unwrap().get("ok").and_then(|v| v.as_bool()), Some(true));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_handler_multi_step_receiver() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    let handler = scheduler.get_session_handler(
        "session_multi".to_string(),
        vec!["default".to_string()],
    );
    let metadata = CallMetadata::new(
        "mock_multi".to_string(),
        "multi_1".to_string(),
        None,
        true,
    );
    let (tx, rx) = mpsc::channel(4);
    tokio::spawn(async move {
        let _ = tx.send(Ok(json!({ "step": 1 }))).await;
        let _ = tx.send(Ok(json!({ "step": 2 }))).await;
    });

    let mut guard = handler.lock().await;
    guard.register_receiver(ToolReciever::new_multi_step(metadata.clone(), rx));
    let mut ui_stream = guard.resolve_last_call().expect("ui stream");
    drop(guard);

    let (recv_meta, result) = ui_stream.next().await.expect("first chunk");
    assert_eq!(recv_meta, metadata);
    assert_eq!(
        result.unwrap().get("step").and_then(|v| v.as_i64()),
        Some(1)
    );

    let mut guard = handler.lock().await;
    let mut saw_step_two = false;
    for _ in 0..5 {
        guard.poll_streams_once();
        let completed = guard.take_completed_calls();
        if completed.iter().any(|c| {
            c.result
                .as_ref()
                .ok()
                .and_then(|v| v.get("step"))
                .and_then(|v| v.as_i64())
                == Some(2)
        }) {
            saw_step_two = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(saw_step_two, "expected step 2 completion from background stream");
}
