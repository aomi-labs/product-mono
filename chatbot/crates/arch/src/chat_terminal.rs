use crate::{ContractApiClient, ContractRequest, ContractResponse};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Abstraction for a stateless tool that can be executed by the agent.
pub trait AomiTool {
    type Output: Send + 'static;

    fn name(&self) -> String;
    fn description(&self) -> String;
    fn execute(&self, input: String) -> ToolResult<Self::Output>;
}

/// Future-aware wrapper for tool execution results.
pub struct ToolResult<T> {
    inner: Pin<Box<dyn Future<Output = T> + Send>>,
}

impl<T> ToolResult<T> {
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = T> + Send + 'static,
    {
        Self { inner: Box::pin(future) }
    }
}

impl<T> Future for ToolResult<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}

impl<T> fmt::Debug for ToolResult<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolResult").finish_non_exhaustive()
    }
}

/// Contract lookup tool that wraps ApiClient
#[derive(Clone)]
pub struct ContractTool {
    api_client: ContractApiClient,
}

impl ContractTool {
    pub fn new() -> Self {
        Self {
            api_client: ContractApiClient::new(),
        }
    }
}

impl AomiTool for ContractTool {
    type Output = Result<ContractResponse, String>;

    fn name(&self) -> String {
        "get_contract".to_string()
    }

    fn description(&self) -> String {
        "Look up contract information by contract ID".to_string()
    }

    fn execute(&self, input: String) -> ToolResult<Self::Output> {
        // Parse input as JSON to extract contract request
        let request: ContractRequest = match serde_json::from_str(&input) {
            Ok(req) => req,
            Err(e) => {
                return ToolResult::new(async move {
                    Err(format!("Failed to parse input: {}", e))
                });
            }
        };

        let api_client = self.api_client.clone();
        ToolResult::new(async move {
            match api_client.get_contract(request).await {
                Ok(response) => Ok(response),
                Err(e) => Err(e.to_string()),
            }
        })
    }
}

impl Default for ContractTool {
    fn default() -> Self {
        Self::new()
    }
}

#[tokio::test]
async fn test_contract_tool() {
    let tool = ContractTool::new();
    let tool_res = tool.execute("{\"contract_id\": \"0x1234567890\", \"query\": \"balance\"}".to_string());
    
    assert!(tool_res.await.is_ok());
}