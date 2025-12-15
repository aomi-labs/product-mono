use crate::ToolScheduler;
use futures::StreamExt;
use serde_json::json;
use std::time::Duration;

use super::utils::{
    register_mock_tools, request_and_get_stream, unique_call_id,
};

#[tokio::test(flavor = "multi_thread")]
async fn test_typed_scheduler_unknown_tool_and_streaming() {
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    let mut handler = scheduler.get_handler();

    let json = serde_json::json!({"test": "data"});
    let mut tool_stream =
        request_and_get_stream(&mut handler, "unknown_tool", json, "stream_1".to_string()).await;

    let message = tool_stream.next().await;
    assert!(message.is_some(), "Should receive stream message");

    let (call_id, result) = message.unwrap();
    assert_eq!(call_id, "stream_1");
    assert!(result.is_err(), "Result should be an Err for unknown tool");

    let error_msg = result.unwrap_err();
    assert!(
        error_msg.contains("Unknown tool"),
        "Message should mention unknown tool: {}",
        error_msg
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_step_tool_first_chunk() {
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.test_set_tool_metadata("mock_multi_step", true, "Mock multi step");

    let call_id = unique_call_id("multi_step");
    let json = serde_json::json!({});
    let mut stream =
        request_and_get_stream(&mut handler, "mock_multi_step", json, call_id.clone()).await;

    let first = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Timeout")
        .expect("Should receive first chunk");

    let (recv_call_id, result) = first;
    assert_eq!(recv_call_id, call_id);
    let value = result.expect("First result should be Ok");
    assert_eq!(value.get("step").unwrap(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_step_tool_streams_all_chunks_and_errors() {
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    register_mock_tools(&scheduler);

    // override error tool metadata to mark as multi-step
    let mut handler = scheduler.get_handler();
    handler.test_set_tool_metadata("mock_multi_step_error", true, "Mock multi step error");

    let mut handler_ok = scheduler.get_handler();
    handler_ok.test_set_tool_metadata("mock_multi_step", true, "Mock multi step");

    // Success path: collect all chunks
    let ok_call_id = unique_call_id("multi_step_ok");
    let _ui_stream = request_and_get_stream(
        &mut handler_ok,
        "mock_multi_step",
        json!({}),
        ok_call_id.clone(),
    )
    .await;

    let mut steps = Vec::new();
    while let Some(completion) = handler_ok.poll_streams_to_next_result().await {
        assert_eq!(completion.call_id, ok_call_id);
        if let Ok(val) = completion.result {
            if let Some(step) = val.get("step").and_then(|v| v.as_u64()) {
                steps.push(step);
            }
        }
    }
    assert_eq!(steps, vec![1, 2, 3], "expected all three step chunks");

    // Error path: ensure error surfaces after initial ok
    let err_call_id = unique_call_id("multi_step_err");
    let _ = request_and_get_stream(
        &mut handler,
        "mock_multi_step_error",
        json!({}),
        err_call_id.clone(),
    )
    .await;

    let mut seen_error = false;
    while let Some(completion) = handler.poll_streams_to_next_result().await {
        assert_eq!(completion.call_id, err_call_id);
        if completion.result.is_err() {
            seen_error = true;
            break;
        }
    }
    assert!(seen_error, "expected error chunk from mock_multi_step_error");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_single_tool_uses_oneshot() {
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.test_set_tool_metadata("mock_single", false, "Mock single");

    let call_id = unique_call_id("single");
    let json = serde_json::json!({});
    let mut stream =
        request_and_get_stream(&mut handler, "mock_single", json, call_id.clone()).await;

    let result = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Timeout")
        .expect("Should receive result via stream");
    let (recv_id, value) = result;
    assert_eq!(recv_id, call_id);
    let value = value.expect("Result should be Ok");
    assert_eq!(value.get("result").unwrap(), "single");

    assert!(stream.next().await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_single_tool_waits_for_completion() {
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.test_set_tool_metadata("mock_slow_single", false, "Mock slow single");

    let call_id = unique_call_id("slow_single");
    let json = serde_json::json!({});
    let mut stream =
        request_and_get_stream(&mut handler, "mock_slow_single", json, call_id.clone()).await;

    let pending = tokio::time::timeout(Duration::from_millis(20), stream.next()).await;
    assert!(pending.is_err(), "stream yielded before tool completed");

    let (recv_id, value) = stream.next().await.expect("Stream should yield result");
    assert_eq!(recv_id, call_id);
    let value = value.expect("Result should be Ok");
    assert_eq!(value.get("result").unwrap(), "slow");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_step_flag_detection() {
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.test_set_tool_metadata("mock_multi_step", true, "test");
    handler.test_set_tool_metadata("mock_single", false, "test");

    assert!(handler.is_multi_step("mock_multi_step"));
    assert!(!handler.is_multi_step("mock_single"));
    assert!(!handler.is_multi_step("nonexistent"));
}
