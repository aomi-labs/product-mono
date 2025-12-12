use super::*;
use crate::types::format_tool_name;
use futures::future::BoxFuture;
use futures::FutureExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

// Mock multi-step tool for testing
struct MockMultiStepTool;

impl AnyApiTool for MockMultiStepTool {
    fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
        async { Ok(serde_json::json!({"step": 1})) }.boxed()
    }

    fn validate_json(&self, _payload: &Value) -> bool {
        true
    }

    fn tool(&self) -> &'static str {
        "mock_multi_step"
    }

    fn description(&self) -> &'static str {
        "Mock multi-step tool"
    }

    fn static_topic(&self) -> &'static str {
        "mock_multi_step"
    }

    fn multi_steps(&self) -> bool {
        true
    }

    fn call_with_sender(
        &self,
        _payload: Value,
        sender: mpsc::Sender<Result<Value>>,
    ) -> BoxFuture<'static, Result<()>> {
        async move {
            for i in 1..=3 {
                let finished = i == 3;
                let result = serde_json::json!({
                    "step": i,
                    "finished": finished
                });
                if sender.send(Ok(result)).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok(())
        }
        .boxed()
    }
}

// Mock single-result tool for comparison
struct MockSingleTool;

impl AnyApiTool for MockSingleTool {
    fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
        async { Ok(serde_json::json!({"result": "single"})) }.boxed()
    }

    fn validate_json(&self, _payload: &Value) -> bool {
        true
    }

    fn tool(&self) -> &'static str {
        "mock_single"
    }

    fn description(&self) -> &'static str {
        "Mock single tool"
    }

    fn static_topic(&self) -> &'static str {
        "mock_single"
    }

    fn multi_steps(&self) -> bool {
        false
    }
}

// Mock single-result tool that delays before responding
struct MockSlowSingleTool;

impl AnyApiTool for MockSlowSingleTool {
    fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
        async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(serde_json::json!({"result": "slow"}))
        }
        .boxed()
    }

    fn validate_json(&self, _payload: &Value) -> bool {
        true
    }

    fn tool(&self) -> &'static str {
        "mock_slow_single"
    }

    fn description(&self) -> &'static str {
        "Mock slow single tool"
    }

    fn static_topic(&self) -> &'static str {
        "mock_slow_single"
    }

    fn multi_steps(&self) -> bool {
        false
    }
}

// Mock multi-step tool that returns error
struct MockMultiStepErrorTool;

impl AnyApiTool for MockMultiStepErrorTool {
    fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
        async { Err(eyre::eyre!("error")) }.boxed()
    }

    fn validate_json(&self, _payload: &Value) -> bool {
        true
    }

    fn tool(&self) -> &'static str {
        "mock_multi_step_error"
    }

    fn description(&self) -> &'static str {
        "Mock multi-step error tool"
    }

    fn static_topic(&self) -> &'static str {
        "mock_multi_step_error"
    }

    fn multi_steps(&self) -> bool {
        true
    }

    fn call_with_sender(
        &self,
        _payload: Value,
        sender: mpsc::Sender<Result<Value>>,
    ) -> BoxFuture<'static, Result<()>> {
        async move {
            let _ = sender.send(Ok(serde_json::json!({"step": 1}))).await;
            let _ = sender.send(Err(eyre::eyre!("step 2 failed"))).await;
            Ok(())
        }
        .boxed()
    }
}

fn register_mock_tools(scheduler: &ToolScheduler) {
    let mut tools = scheduler.tools.write().unwrap();
    tools.entry("mock_multi_step".to_string()).or_insert_with(|| Arc::new(MockMultiStepTool));
    tools.entry("mock_single".to_string()).or_insert_with(|| Arc::new(MockSingleTool));
    tools.entry("mock_slow_single".to_string()).or_insert_with(|| Arc::new(MockSlowSingleTool));
    tools.entry("mock_multi_step_error".to_string()).or_insert_with(|| Arc::new(MockMultiStepErrorTool));
}

fn unique_call_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{}_{}", prefix, COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Helper to request a tool and get the UI stream (using new split API)
async fn request_and_get_stream(
    handler: &mut ToolApiHandler,
    tool_name: &str,
    payload: Value,
    call_id: String,
) -> ToolResultStream {
    handler.request(tool_name.to_string(), payload, call_id).await;
    let (internal_stream, ui_stream) = handler
        .take_last_future_as_streams()
        .expect("Should have pending future after request");
    handler.add_pending_stream(internal_stream);
    ui_stream
}

#[test]
fn test_format_tool_name_snake_case() {
    assert_eq!(
        format_tool_name("encode_function_call"),
        "Encode function call"
    );
    assert_eq!(format_tool_name("get_current_time"), "Get current time");
    assert_eq!(format_tool_name("send_transaction"), "Send transaction");
}

#[test]
fn test_format_tool_name_non_snake_case() {
    assert_eq!(format_tool_name("MyTool"), "My tool");
    assert_eq!(format_tool_name("GetTime"), "Get time");
    assert_eq!(format_tool_name("encode"), "Encode");
}

#[test]
fn test_format_tool_name_caching() {
    let result1 = format_tool_name("test_tool");
    let result2 = format_tool_name("test_tool");
    assert!(std::ptr::eq(result1, result2));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_typed_scheduler_unknown_tool_and_streaming() {
    // Verifies unknown tools return an error via the streaming ACK path.
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    let mut handler = scheduler.get_handler();

    let json = serde_json::json!({"test": "data"});
    let mut tool_stream = request_and_get_stream(
        &mut handler,
        "unknown_tool",
        json,
        "stream_1".to_string(),
    ).await;

    let message = tool_stream.next().await;
    assert!(message.is_some(), "Should receive stream message");

    let (call_id, result) = message.unwrap();
    assert_eq!(call_id, "stream_1");
    assert!(result.is_err(), "Result should be an Err for unknown tool");

    let error_msg = result.unwrap_err();
    assert!(error_msg.contains("Unknown tool"), "Message should mention unknown tool: {}", error_msg);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_step_tool_first_chunk() {
    // Tests that multi-step tools return first chunk via UI stream
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.tool_info.insert("mock_multi_step".to_string(), (true, "Mock multi step".to_string()));

    let call_id = unique_call_id("multi_step");
    let json = serde_json::json!({});
    let mut stream = request_and_get_stream(
        &mut handler,
        "mock_multi_step",
        json,
        call_id.clone(),
    ).await;

    // Get first chunk via UI stream
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
async fn test_single_tool_uses_oneshot() {
    // Ensures single-result tools deliver their payload through the shared oneshot/stream path.
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.tool_info.insert("mock_single".to_string(), (false, "Mock single".to_string()));

    let call_id = unique_call_id("single");
    let json = serde_json::json!({});
    let mut stream = request_and_get_stream(
        &mut handler,
        "mock_single",
        json,
        call_id.clone(),
    ).await;

    let result = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Timeout")
        .expect("Should receive result via stream");
    let (recv_id, value) = result;
    assert_eq!(recv_id, call_id);
    let value = value.expect("Result should be Ok");
    assert_eq!(value.get("result").unwrap(), "single");

    // Stream should be exhausted
    assert!(stream.next().await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_single_tool_waits_for_completion() {
    // Verifies poll_next_stream does not emit a placeholder when a single-result tool is still running.
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.tool_info.insert("mock_slow_single".to_string(), (false, "Mock slow single".to_string()));

    let call_id = unique_call_id("slow_single");
    let json = serde_json::json!({});
    let mut stream = request_and_get_stream(
        &mut handler,
        "mock_slow_single",
        json,
        call_id.clone(),
    ).await;

    // poll_next_stream should remain pending while the tool is still executing
    let pending = tokio::time::timeout(Duration::from_millis(20), handler.poll_futures_to_streams()).await;
    assert!(pending.is_err(), "poll_next_stream returned before tool completed");

    let (recv_id, value) = stream.next().await.expect("Stream should yield result");
    assert_eq!(recv_id, call_id);
    let value = value.expect("Result should be Ok");
    assert_eq!(value.get("result").unwrap(), "slow");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_step_flag_detection() {
    // Verifies the cached tool metadata correctly distinguishes multi-step vs single tools.
    let scheduler = ToolScheduler::get_or_init().await.unwrap();
    register_mock_tools(&scheduler);

    let mut handler = scheduler.get_handler();
    handler.tool_info.insert("mock_multi_step".to_string(), (true, "test".to_string()));
    handler.tool_info.insert("mock_single".to_string(), (false, "test".to_string()));

    assert!(handler.is_multi_step("mock_multi_step"));
    assert!(!handler.is_multi_step("mock_single"));
    assert!(!handler.is_multi_step("nonexistent"));
}

mod future_tests {
    use eyre::Result;
    use futures::TryFutureExt;

    async fn might_fail(i: u32) -> Result<u32> {
        if i.is_multiple_of(2) {
            Ok(i * 2)
        } else {
            Err(eyre::eyre!("odd number"))
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_future_error_handling() {
        let fut = might_fail(3);
        let fut2 = fut.map_err(|e| e.wrap_err("error"));
        match fut2.await {
            Ok(v) => println!("ok: {v}"),
            Err(e) => println!("err: {e}"),
        }
    }
}
