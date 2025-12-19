use crate::ToolScheduler;
use futures::StreamExt;
use std::time::Duration;

use super::utils::{
    MockMultiStepTool, register_mock_multi_step_tool, register_mock_tools, request_and_get_stream,
    unique_call_id,
};

#[tokio::test(flavor = "multi_thread")]
async fn test_typed_scheduler_unknown_tool_and_streaming() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
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
async fn test_single_tool_uses_oneshot() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.test_set_tool_metadata("mock_single", false, "Mock single");

    let call_id = unique_call_id("single");
    let json = serde_json::json!({"input": "test"});
    let mut stream =
        request_and_get_stream(&mut handler, "mock_single", json, call_id.clone()).await;

    let result = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Timeout")
        .expect("Should receive result via stream");
    let (recv_id, value) = result;
    assert_eq!(recv_id, call_id);
    let value = value.expect("Result should be Ok");
    // Tool returns a JSON string, so we parse it
    let parsed: serde_json::Value =
        serde_json::from_str(value.as_str().unwrap()).expect("Should parse as JSON");
    assert_eq!(parsed.get("result").unwrap(), "single");

    assert!(stream.next().await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_single_tool_waits_for_completion() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.test_set_tool_metadata("mock_slow_single", false, "Mock slow single");

    let call_id = unique_call_id("slow_single");
    let json = serde_json::json!({"input": "test"});
    let mut stream =
        request_and_get_stream(&mut handler, "mock_slow_single", json, call_id.clone()).await;

    let pending = tokio::time::timeout(Duration::from_millis(20), stream.next()).await;
    assert!(pending.is_err(), "stream yielded before tool completed");

    let (recv_id, value) = stream.next().await.expect("Stream should yield result");
    assert_eq!(recv_id, call_id);
    let value = value.expect("Result should be Ok");
    let parsed: serde_json::Value =
        serde_json::from_str(value.as_str().unwrap()).expect("Should parse as JSON");
    assert_eq!(parsed.get("result").unwrap(), "slow");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_error_tool_returns_error() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.test_set_tool_metadata("mock_error", false, "Mock error");

    let call_id = unique_call_id("error");
    let json = serde_json::json!({"input": "test"});
    let mut stream =
        request_and_get_stream(&mut handler, "mock_error", json, call_id.clone()).await;

    let result = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Timeout")
        .expect("Should receive result via stream");
    let (recv_id, value) = result;
    assert_eq!(recv_id, call_id);
    assert!(value.is_err(), "Result should be an Err");
    let error_msg = value.unwrap_err();
    assert!(
        error_msg.contains("mock error"),
        "Error should contain 'mock error': {}",
        error_msg
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_step_tool_first_chunk() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    register_mock_multi_step_tool(&scheduler, None);

    let mut handler = scheduler.get_handler();

    let call_id = unique_call_id("multi_first");
    let json = serde_json::json!({"input": "hello"});
    let mut stream =
        request_and_get_stream(&mut handler, "mock_multi_step", json, call_id.clone()).await;

    assert!(stream.is_multi_step, "stream should be marked multi-step");

    let first = tokio::time::timeout(Duration::from_millis(200), stream.next())
        .await
        .expect("timed out waiting for first chunk")
        .expect("stream ended early");

    let (recv_id, value) = first;
    assert_eq!(recv_id, call_id);
    let value = value.expect("chunk should be Ok");
    assert_eq!(value.get("step").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(value.get("input").and_then(|v| v.as_str()), Some("hello"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_step_tool_streams_all_chunks_and_errors() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    let tool = MockMultiStepTool::default().with_error_at(2);
    register_mock_multi_step_tool(&scheduler, Some(tool));

    let mut handler = scheduler.get_handler();

    let call_id = unique_call_id("multi_err");
    let json = serde_json::json!({"input": "bye"});
    let mut stream =
        request_and_get_stream(&mut handler, "mock_multi_step", json, call_id.clone()).await;

    // UI stream should still surface the first chunk quickly
    let _first = tokio::time::timeout(Duration::from_millis(200), stream.next())
        .await
        .expect("timed out waiting for first chunk")
        .expect("stream ended early");

    // Remaining chunks (including the first, fan-out) are polled from handler
    let mut completions = Vec::new();
    for _ in 0..5 {
        match tokio::time::timeout(
            Duration::from_millis(200),
            handler.poll_streams_to_next_result(),
        )
        .await
        {
            Ok(Some(completion)) => completions.push(completion),
            _ => break,
        }
    }

    assert_eq!(completions.len(), 3, "should emit 2 ok chunks and an error");

    let first = &completions[0];
    assert_eq!(first.call_id, call_id);
    assert!(first.result.is_ok());
    assert_eq!(
        first
            .result
            .as_ref()
            .ok()
            .and_then(|v| v.get("step"))
            .and_then(|v| v.as_i64()),
        Some(1)
    );

    let second = &completions[1];
    assert_eq!(second.call_id, call_id);
    assert!(second.result.is_ok());
    assert_eq!(
        second
            .result
            .as_ref()
            .ok()
            .and_then(|v| v.get("step"))
            .and_then(|v| v.as_i64()),
        Some(2)
    );

    let third = &completions[2];
    assert_eq!(third.call_id, call_id);
    assert!(third.result.is_err(), "third chunk should be error");
    let err = third.result.clone().unwrap_err();
    assert!(
        err.contains("chunk error"),
        "error message should mention chunk error: {err}"
    );
}
