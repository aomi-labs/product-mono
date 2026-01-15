use crate::{AomiTool, ToolScheduler};
use crate::ToolMetadata;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::{mpsc::Sender, oneshot};

// ============================================================================
// Mock Tool Parameters
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockToolParameters {
    pub input: String,
}

// Parameters shared with the multi-step mock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockMultiStepParameters {
    pub input: String,
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

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Test input"
                }
            },
            "required": ["input"]
        })
    }

    fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let _ = sender.send(Ok(json!({ "result": "single" })));
        }
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

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Test input"
                }
            },
            "required": ["input"]
        })
    }

    fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = sender.send(Ok(json!({ "result": "slow" })));
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

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Test input"
                }
            },
            "required": ["input"]
        })
    }

    fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let _ = sender.send(Err(eyre::eyre!("mock error")));
        }
    }
}

// ============================================================================
// MockMultiStepTool - Streams multiple chunks (AnyApiTool path)
// ============================================================================

#[derive(Debug, Clone)]
pub struct MockMultiStepTool {
    pub name: &'static str,
    pub chunks: Vec<Value>,
    pub error_at: Option<usize>,
}

impl Default for MockMultiStepTool {
    fn default() -> Self {
        Self {
            name: "mock_multi_step",
            chunks: vec![
                json!({"step": 1, "status": "started"}),
                json!({"step": 2, "status": "in_progress"}),
                json!({"step": 3, "status": "completed"}),
            ],
            error_at: None,
        }
    }
}

impl MockMultiStepTool {
    pub fn with_error_at(mut self, idx: usize) -> Self {
        self.error_at = Some(idx);
        self
    }
}

impl AomiTool for MockMultiStepTool {
    const NAME: &'static str = "mock_multi_step";

    type Args = MockMultiStepParameters;
    type Output = Value;
    type Error = ToolError;

    fn support_async(&self) -> bool {
        true
    }

    fn description(&self) -> &'static str {
        "Mock multi-step tool for scheduler tests"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Test input"
                }
            },
            "required": ["input"]
        })
    }

    fn run_async(
        &self,
        sender: Sender<eyre::Result<Value>>,
        request: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        let chunks = self.chunks.clone();
        let error_at = self.error_at;

        async move {
            for (idx, mut chunk) in chunks.into_iter().enumerate() {
                if let Some(obj) = chunk.as_object_mut() {
                    obj.entry("input".to_string())
                        .or_insert_with(|| json!(request.input.clone()));
                }

                if Some(idx) == error_at {
                    let _ = sender
                        .send(Err(eyre::eyre!("chunk error at {}", idx)))
                        .await;
                    break;
                } else {
                    let _ = sender.send(Ok(chunk)).await;
                }
            }
        }
    }
}

// ============================================================================
// Registration helpers
// ============================================================================

/// Register mock tools using the standard Rig Tool registration path
pub fn register_mock_tools(scheduler: &ToolScheduler) {
    scheduler
        .register_metadata(ToolMetadata::new(
            MockSingleTool::NAME.to_string(),
            "default".to_string(),
            "Mock single".to_string(),
            false,
        ))
        .expect("Failed to register MockSingleTool");
    scheduler
        .register_metadata(ToolMetadata::new(
            MockSlowSingleTool::NAME.to_string(),
            "default".to_string(),
            "Mock slow single".to_string(),
            false,
        ))
        .expect("Failed to register MockSlowSingleTool");
    scheduler
        .register_metadata(ToolMetadata::new(
            MockErrorTool::NAME.to_string(),
            "default".to_string(),
            "Mock error".to_string(),
            false,
        ))
        .expect("Failed to register MockErrorTool");
}

pub fn register_mock_multi_step_tool(scheduler: &ToolScheduler, tool: Option<MockMultiStepTool>) {
    let tool = tool.unwrap_or_default();
    scheduler
        .register_metadata(ToolMetadata::new(
            tool.name.to_string(),
            "default".to_string(),
            "Mock multi step".to_string(),
            true,
        ))
        .expect("Failed to register multi-step tool");
}
