use crate::scheduler::ToolHandler;
use crate::streams::ToolStream;
use crate::{AsyncTool, ToolScheduler};
use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc::Sender;

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

impl Tool for MockSingleTool {
    const NAME: &'static str = "mock_single";
    type Args = MockToolParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Mock single-result tool for testing".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Test input"
                    }
                },
                "required": ["input"]
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(r#"{"result": "single"}"#.to_string())
    }
}

// ============================================================================
// MockSlowSingleTool - Rig Tool with delay
// ============================================================================

#[derive(Debug, Clone)]
pub struct MockSlowSingleTool;

impl Tool for MockSlowSingleTool {
    const NAME: &'static str = "mock_slow_single";
    type Args = MockToolParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Mock slow single-result tool for testing".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Test input"
                    }
                },
                "required": ["input"]
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(r#"{"result": "slow"}"#.to_string())
    }
}

// ============================================================================
// MockErrorTool - Tool that returns an error
// ============================================================================

#[derive(Debug, Clone)]
pub struct MockErrorTool;

impl Tool for MockErrorTool {
    const NAME: &'static str = "mock_error";
    type Args = MockToolParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Mock tool that returns an error for testing".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Test input"
                    }
                },
                "required": ["input"]
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        Err(ToolError::ToolCallError("mock error".into()))
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

impl AsyncTool for MockMultiStepTool {
    type ApiRequest = MockMultiStepParameters;
    type Error = ToolError;

    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Mock multi-step tool for scheduler tests"
    }

    fn validate(&self, request: &Self::ApiRequest) -> eyre::Result<()> {
        if request.input.is_empty() {
            Err(eyre::eyre!("input required"))
        } else {
            Ok(())
        }
    }

    fn call_stream(
        &self,
        request: Self::ApiRequest,
        sender: Sender<eyre::Result<Value>>,
    ) -> Pin<Box<dyn Future<Output = eyre::Result<()>> + Send>> {
        let chunks = self.chunks.clone();
        let error_at = self.error_at;

        Box::pin(async move {
            for (idx, mut chunk) in chunks.into_iter().enumerate() {
                // Enrich chunk with input for assertions
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
            Ok(())
        })
    }

    fn validate_async_result(&self, value: &Value) -> eyre::Result<Value> {
        if value.get("step").is_some() {
            Ok(value.clone())
        } else {
            Err(eyre::eyre!("missing step field"))
        }
    }
}

// ============================================================================
// Registration helpers
// ============================================================================

/// Register mock tools using the standard Rig Tool registration path
pub fn register_mock_tools(scheduler: &ToolScheduler) {
    scheduler
        .register_tool(MockSingleTool)
        .expect("Failed to register MockSingleTool");
    scheduler
        .register_tool(MockSlowSingleTool)
        .expect("Failed to register MockSlowSingleTool");
    scheduler
        .register_tool(MockErrorTool)
        .expect("Failed to register MockErrorTool");
}

pub fn register_mock_multi_step_tool(scheduler: &ToolScheduler, tool: Option<MockMultiStepTool>) {
    let tool = tool.unwrap_or_default();
    scheduler
        .register_multi_step_tool(tool)
        .expect("Failed to register multi-step tool");
}

pub fn unique_call_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{}_{}", prefix, COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Helper to request a tool and get the UI stream (using new split API)
pub async fn request_and_get_stream(
    handler: &mut ToolHandler,
    tool_name: &str,
    payload: Value,
    call_id: String,
) -> ToolStream {
    handler
        .request(tool_name.to_string(), payload, call_id)
        .await;
    // resolve_last_call now internally adds bg_stream to ongoing_streams
    handler
        .resolve_last_call()
        .expect("Should have pending future after request")
}
