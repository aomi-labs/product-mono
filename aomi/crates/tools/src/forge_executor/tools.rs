use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::forge_executor;

use super::executor::ForgeExecutor;
use super::plan::OperationGroup;
use super::types::GroupResult;

/// Global storage for the ForgeExecutor
static EXECUTOR: once_cell::sync::Lazy<Arc<Mutex<Option<ForgeExecutor>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

/// Parameters for SetExecutionPlan tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetExecutionPlanParameters {
    /// The operation groups to execute
    pub groups: Vec<OperationGroup>,
}

/// Result of SetExecutionPlan tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetExecutionPlanResult {
    pub success: bool,
    pub message: String,
    pub total_groups: usize,
}

/// Tool for setting the execution plan
#[derive(Debug, Clone)]
pub struct SetExecutionPlan;

impl Tool for SetExecutionPlan {
    const NAME: &'static str = "set_execution_plan";
    type Args = SetExecutionPlanParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Set the execution plan with operation groups and dependencies. This initializes the ForgeExecutor and starts background contract fetching.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "groups": {
                        "type": "array",
                        "description": "Array of operation groups to execute",
                        "items": {
                            "type": "object",
                            "properties": {
                                "description": {
                                    "type": "string",
                                    "description": "Human-readable description of this group"
                                },
                                "operations": {
                                    "type": "array",
                                    "description": "List of operations in natural language",
                                    "items": { "type": "string" }
                                },
                                "dependencies": {
                                    "type": "array",
                                    "description": "Indices of groups this group depends on",
                                    "items": { "type": "integer" }
                                },
                                "contracts": {
                                    "type": "array",
                                    "description": "Contracts needed for this group",
                                    "items": {
                                        "type": "array",
                                        "description": "Tuple of (chain_id, address, name)",
                                        "items": { "type": "string" }
                                    }
                                }
                            },
                            "required": ["description", "operations", "dependencies", "contracts"]
                        }
                    }
                },
                "required": ["groups"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let groups = args.groups;
        let total_groups = groups.len();

        // Create new executor
        let executor = ForgeExecutor::new(groups).await.map_err(|e| {
            ToolError::ToolCallError(format!("Failed to create ForgeExecutor: {}", e).into())
        })?;

        // Store executor globally
        let mut global_executor = EXECUTOR.lock().await;
        if let Some(prev) = global_executor.take() {
            prev.shutdown();
        }
        *global_executor = Some(executor);

        let result = SetExecutionPlanResult {
            success: true,
            message: format!(
                "Execution plan set with {} groups. Background contract fetching started.",
                total_groups
            ),
            total_groups,
        };

        Ok(serde_json::to_string(&result).map_err(|e| {
            ToolError::ToolCallError(format!("Failed to serialize result: {}", e).into())
        })?)
    }
}

/// Parameters for NextGroups tool (no parameters needed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextGroupsParameters {}

/// Result of NextGroups tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextGroupsResult {
    pub results: Vec<GroupResult>,
    pub remaining_groups: usize,
}

/// Tool for executing the next ready groups
#[derive(Debug, Clone)]
pub struct NextGroups;

impl Tool for NextGroups {
    const NAME: &'static str = "next_groups";
    type Args = NextGroupsParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Execute the next batch of ready operation groups (groups whose dependencies are satisfied). Returns transaction data and generated Solidity code for each group.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Get executor from global storage
        let mut global_executor = EXECUTOR.lock().await;
        let executor = global_executor
            .as_mut()
            .ok_or_else(|| {
                ToolError::ToolCallError(
                    "No execution plan set. Call set_execution_plan first.".into(),
                )
            })?;

        // Execute next groups
        let results = executor.next_groups().await.map_err(|e| {
            ToolError::ToolCallError(format!("Failed to execute next groups: {}", e).into())
        })?;

        // Calculate remaining groups
        let remaining_groups = executor
            .plan
            .statuses
            .iter()
            .filter(|s| matches!(s, forge_executor::plan::GroupStatus::Todo))
            .count();

        let response = NextGroupsResult {
            results,
            remaining_groups,
        };

        Ok(serde_json::to_string(&response).map_err(|e| {
            ToolError::ToolCallError(format!("Failed to serialize result: {}", e).into())
        })?)
    }
}
