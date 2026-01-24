use crate::{AomiTool, AomiToolArgs, ToolCallCtx, ToolScheduler, with_topic};
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc::Sender;

// ============================================================================
// Mock Tool Parameters
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockToolParameters {
    pub input: String,
}

impl AomiToolArgs for MockToolParameters {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Test input"
                }
            },
            "required": ["input"]
        }))
    }
}

// Parameters shared with the multi-step mock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockMultiStepParameters {
    pub input: String,
}

impl AomiToolArgs for MockMultiStepParameters {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Test input"
                }
            },
            "required": ["input"]
        }))
    }
}

// ============================================================================
// MockSingleTool - Standard Rig Tool (single result)
// ============================================================================

#[derive(Debug, Clone)]
pub struct MockSingleTool;

impl AomiTool for MockSingleTool {
    const NAME: &'static str = "mock_single";

    type Args = MockToolParameters;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Mock single-result tool for testing"
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<Value>> + Send {
        async move { Ok(json!({ "result": "single" })) }
    }
}

// ============================================================================
// MockSlowSingleTool - Rig Tool with delay
// ============================================================================

#[derive(Debug, Clone)]
pub struct MockSlowSingleTool;

impl AomiTool for MockSlowSingleTool {
    const NAME: &'static str = "mock_slow_single";

    type Args = MockToolParameters;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Mock slow single-result tool for testing"
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<Value>> + Send {
        async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(json!({ "result": "slow" }))
        }
    }
}

// ============================================================================
// MockErrorTool - Tool that returns an error
// ============================================================================

#[derive(Debug, Clone)]
pub struct MockErrorTool;

impl AomiTool for MockErrorTool {
    const NAME: &'static str = "mock_error";

    type Args = MockToolParameters;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Mock tool that returns an error for testing"
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<Value>> + Send {
        async move { Err(eyre::eyre!("mock error")) }
    }
}

// ============================================================================
// MockMultiStepTool - Streams multiple chunks (AnyApiTool path)
// ============================================================================

#[derive(Debug, Clone)]
pub struct MockAsyncTool {
    pub name: &'static str,
    pub chunks: Vec<Value>,
    pub error_at: Option<usize>,
}

impl Default for MockAsyncTool {
    fn default() -> Self {
        Self {
            name: "mock_async",
            chunks: vec![
                json!({"step": 1, "status": "started"}),
                json!({"step": 2, "status": "in_progress"}),
                json!({"step": 3, "status": "completed"}),
            ],
            error_at: None,
        }
    }
}

impl MockAsyncTool {
    pub fn with_error_at(mut self, idx: usize) -> Self {
        self.error_at = Some(idx);
        self
    }
}

impl AomiTool for MockAsyncTool {
    const NAME: &'static str = "mock_async";

    type Args = MockToolParameters;
    type Output = Value;
    type Error = ToolError;

    fn support_async(&self) -> bool {
        true
    }

    fn description(&self) -> &'static str {
        "Mock multi-step tool for scheduler tests"
    }

    fn run_async(
        &self,
        sender: Sender<(eyre::Result<Value>, bool)>,
        _ctx: ToolCallCtx,
        request: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        let chunks = self.chunks.clone();
        let error_at = self.error_at;

        async move {
            let total_chunks = chunks.len();
            for (idx, mut chunk) in chunks.into_iter().enumerate() {
                let has_more = idx + 1 < total_chunks;
                if let Some(obj) = chunk.as_object_mut() {
                    obj.entry("input".to_string())
                        .or_insert_with(|| json!(request.input.clone()));
                }

                if Some(idx) == error_at {
                    let _ = sender
                        .send((Err(eyre::eyre!("chunk error at {}", idx)), false))
                        .await;
                    break;
                } else {
                    let _ = sender.send((Ok(chunk), has_more)).await;
                }
            }
        }
    }
}

// ============================================================================
// Registration helpers
// ============================================================================

/// Register mock tools using the AomiTool::metadata() interface
pub fn register_mock_tools(scheduler: &ToolScheduler) {
    scheduler
        .register_tool(&MockSingleTool)
        .expect("Failed to register MockSingleTool");
    scheduler
        .register_tool(&MockSlowSingleTool)
        .expect("Failed to register MockSlowSingleTool");
    scheduler
        .register_tool(&MockErrorTool)
        .expect("Failed to register MockErrorTool");
}

pub fn register_mock_async_tool(scheduler: &ToolScheduler, tool: Option<MockAsyncTool>) {
    let tool = tool.unwrap_or_default();
    scheduler
        .register_tool(&tool)
        .expect("Failed to register mock async tool");
}
