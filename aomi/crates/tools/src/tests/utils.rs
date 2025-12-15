use crate::scheduler::ToolApiHandler;
use crate::tool_stream::ToolResultStream;
use crate::types::AnyApiTool;
use crate::ToolScheduler;
use eyre::Result;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

// Mock multi-step tool for testing
pub struct MockMultiStepTool;

impl AnyApiTool for MockMultiStepTool {
    fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
        async { Ok(serde_json::json!({"step": 1})) }.boxed()
    }

    fn validate_json(&self, _payload: &Value) -> Result<()> {
        Ok(())
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
pub struct MockSingleTool;

impl AnyApiTool for MockSingleTool {
    fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
        async { Ok(serde_json::json!({"result": "single"})) }.boxed()
    }

    fn validate_json(&self, _payload: &Value) -> Result<()> {
        Ok(())
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
pub struct MockSlowSingleTool;

impl AnyApiTool for MockSlowSingleTool {
    fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
        async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(serde_json::json!({"result": "slow"}))
        }
        .boxed()
    }

    fn validate_json(&self, _payload: &Value) -> Result<()> {
        Ok(())
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
pub struct MockMultiStepErrorTool;

impl AnyApiTool for MockMultiStepErrorTool {
    fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
        async { Err(eyre::eyre!("error")) }.boxed()
    }

    fn validate_json(&self, _payload: &Value) -> Result<()> {
        Ok(())
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

pub fn register_mock_tools(scheduler: &ToolScheduler) {
    scheduler.test_register_any_tool("mock_multi_step", Arc::new(MockMultiStepTool));
    scheduler.test_register_any_tool("mock_single", Arc::new(MockSingleTool));
    scheduler.test_register_any_tool("mock_slow_single", Arc::new(MockSlowSingleTool));
    scheduler.test_register_any_tool(
        "mock_multi_step_error",
        Arc::new(MockMultiStepErrorTool),
    );
}

pub fn unique_call_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{}_{}", prefix, COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Helper to request a tool and get the UI stream (using new split API)
pub async fn request_and_get_stream(
    handler: &mut ToolApiHandler,
    tool_name: &str,
    payload: Value,
    call_id: String,
) -> ToolResultStream {
    handler
        .request(tool_name.to_string(), payload, call_id)
        .await;
    let (internal_stream, ui_stream) = handler
        .take_last_call_as_streams()
        .expect("Should have pending future after request");
    handler.add_ongoing_stream(internal_stream);
    ui_stream
}
