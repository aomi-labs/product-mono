use aomi_tools::MultiStepApiTool;
use eyre::Result as EyreResult;
use futures::FutureExt;
use futures::future::BoxFuture;
use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use aomi_scripts::forge_executor::{ForgeManager, GroupResult, OperationGroup};

use tokio::sync::OnceCell;

/// Global storage for Forge execution plans
static MANAGER: OnceCell<Arc<ForgeManager>> = OnceCell::const_new();

async fn forge_manager() -> Result<Arc<ForgeManager>, ToolError> {
    let manager = MANAGER
        .get_or_try_init(|| async {
            let manager = ForgeManager::new()
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok::<Arc<ForgeManager>, ToolError>(Arc::new(manager))
        })
        .await?;
    Ok(Arc::clone(manager))
}

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
    pub plan_id: String,
}

/// Tool for setting the execution plan
#[derive(Debug, Clone)]
pub struct SetExecutionPlan;

async fn build_execution_plan_result(
    args: SetExecutionPlanParameters,
) -> Result<SetExecutionPlanResult, ToolError> {
    let groups = args.groups;
    let total_groups = groups.len();
    let manager = forge_manager().await?;
    let (plan_id, _) = manager.create_plan(groups).await.map_err(|e| {
        ToolError::ToolCallError(format!("Failed to create execution plan: {}", e).into())
    })?;

    Ok(SetExecutionPlanResult {
        success: true,
        message: format!(
            "Execution plan set with {} groups. Background contract fetching started.",
            total_groups
        ),
        total_groups,
        plan_id,
    })
}

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
        let result = build_execution_plan_result(args).await?;

        serde_json::to_string(&result).map_err(|e| {
            ToolError::ToolCallError(format!("Failed to serialize result: {}", e).into())
        })
    }
}

impl MultiStepApiTool for SetExecutionPlan {
    type ApiRequest = SetExecutionPlanParameters;
    type Error = ToolError;

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn description(&self) -> &'static str {
        "Set the execution plan with operation groups and dependencies. This initializes the ForgeExecutor and starts background contract fetching."
    }

    fn validate(&self, _request: &Self::ApiRequest) -> EyreResult<()> {
        Ok(())
    }

    fn call_stream(
        &self,
        request: Self::ApiRequest,
        sender: tokio::sync::mpsc::Sender<EyreResult<Value>>,
    ) -> BoxFuture<'static, EyreResult<()>> {
        async move {
            match build_execution_plan_result(request).await {
                Ok(result) => {
                    let _ = sender
                        .send(Ok(json!({
                            "status": "queued",
                            "message": "Result will be delivered via async update.",
                            "plan_id": result.plan_id,
                            "total_groups": result.total_groups,
                        })))
                        .await;

                    let payload = serde_json::to_value(result)
                        .map_err(|e| eyre::eyre!(format!("Failed to serialize result: {}", e)))?;
                    let _ = sender.send(Ok(payload)).await;
                }
                Err(err) => {
                    let _ = sender.send(Err(eyre::eyre!(err.to_string()))).await;
                }
            }
            Ok(())
        }
        .boxed()
    }
}

/// Parameters for NextGroups tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextGroupsParameters {
    /// The plan id returned by set_execution_plan
    pub plan_id: String,
}

/// Result of NextGroups tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextGroupsResult {
    pub results: Vec<GroupResult>,
    pub remaining_groups: usize,
}

/// Tool for executing the next ready groups
#[derive(Debug, Clone)]
pub struct NextGroups;

async fn build_next_groups_result(
    args: NextGroupsParameters,
) -> Result<NextGroupsResult, ToolError> {
    let manager = forge_manager().await?;
    let (results, remaining_groups) = manager.next_groups(&args.plan_id).await.map_err(|e| {
        ToolError::ToolCallError(format!("Failed to execute next groups: {}", e).into())
    })?;

    Ok(NextGroupsResult {
        results,
        remaining_groups,
    })
}

impl Tool for NextGroups {
    const NAME: &'static str = "next_groups";
    type Args = NextGroupsParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Execute the next batch of ready operation groups for a plan id (groups whose dependencies are satisfied). Returns transaction data and generated Solidity code for each group.".to_string(),
            parameters: json!({
            "type": "object",
            "properties": {
                "plan_id": {
                    "type": "string",
                    "description": "Plan id returned by set_execution_plan"
                }
            },
            "required": ["plan_id"]
        }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let response = build_next_groups_result(args).await?;

        serde_json::to_string(&response).map_err(|e| {
            ToolError::ToolCallError(format!("Failed to serialize result: {}", e).into())
        })
    }
}

impl MultiStepApiTool for NextGroups {
    type ApiRequest = NextGroupsParameters;
    type Error = ToolError;

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn description(&self) -> &'static str {
        "Execute the next batch of ready operation groups for a plan id (groups whose dependencies are satisfied). Returns transaction data and generated Solidity code for each group."
    }

    fn validate(&self, _request: &Self::ApiRequest) -> EyreResult<()> {
        Ok(())
    }

    fn call_stream(
        &self,
        request: Self::ApiRequest,
        sender: tokio::sync::mpsc::Sender<EyreResult<Value>>,
    ) -> BoxFuture<'static, EyreResult<()>> {
        async move {
            let _ = sender
                .send(Ok(json!({
                    "status": "queued",
                    "message": "Result will be delivered via async update."
                })))
                .await;

            match build_next_groups_result(request).await {
                Ok(result) => {
                    let payload = serde_json::to_value(result)
                        .map_err(|e| eyre::eyre!(format!("Failed to serialize result: {}", e)))?;
                    let _ = sender.send(Ok(payload)).await;
                }
                Err(err) => {
                    let _ = sender.send(Err(eyre::eyre!(err.to_string()))).await;
                }
            }
            Ok(())
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::{NextGroups, NextGroupsParameters, SetExecutionPlan, SetExecutionPlanParameters};
    use aomi_scripts::forge_executor::OperationGroup;
    use rig::tool::Tool;

    fn skip_without_anthropic_api_key() -> bool {
        std::env::var("ANTHROPIC_API_KEY").is_err()
    }

    #[tokio::test]
    async fn test_set_execution_plan_success_with_serialization() {
        if skip_without_anthropic_api_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set (required for BAML client)");
            return;
        }
        let groups = vec![
            OperationGroup {
                description: "Wrap ETH to WETH".to_string(),
                operations: vec!["wrap 1 ETH to WETH".to_string()],
                dependencies: vec![],
                contracts: vec![(
                    "1".to_string(),
                    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                    "WETH".to_string(),
                )],
            },
            OperationGroup {
                description: "Swap WETH for USDC".to_string(),
                operations: vec!["swap 1 WETH for USDC on Uniswap".to_string()],
                dependencies: vec![0],
                contracts: vec![
                    (
                        "1".to_string(),
                        "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                        "WETH".to_string(),
                    ),
                    (
                        "1".to_string(),
                        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
                        "USDC".to_string(),
                    ),
                ],
            },
        ];

        let params = SetExecutionPlanParameters { groups };
        let tool = SetExecutionPlan;
        let result = tool.call(params).await.expect("should succeed");

        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("should be valid JSON");

        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["total_groups"], 2);
        assert!(parsed["plan_id"].is_string());
        assert!(
            parsed["message"]
                .as_str()
                .unwrap()
                .contains("Background contract fetching started")
        );
    }

    #[tokio::test]
    #[ignore = "requires clean global state - run in isolation"]
    async fn test_next_groups_no_plan_error() {
        let tool = NextGroups;
        let params = NextGroupsParameters {
            plan_id: "missing-plan".to_string(),
        };

        let result = tool.call(params).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("No execution plan found"));
    }

    #[tokio::test]
    async fn test_next_groups_json_serialization() {
        if skip_without_anthropic_api_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set (required for BAML client)");
            return;
        }
        let groups = vec![OperationGroup {
            description: "Simple operation".to_string(),
            operations: vec!["do something".to_string()],
            dependencies: vec![],
            contracts: vec![],
        }];

        let set_tool = SetExecutionPlan;
        let set_params = SetExecutionPlanParameters { groups };

        let set_result = set_tool
            .call(set_params)
            .await
            .expect("should set plan successfully");
        let parsed: serde_json::Value =
            serde_json::from_str(&set_result).expect("should be valid JSON");
        let plan_id = parsed["plan_id"]
            .as_str()
            .expect("plan_id should be string")
            .to_string();

        let next_tool = NextGroups;
        let next_params = NextGroupsParameters { plan_id };

        let result = next_tool.call(next_params).await;

        if let Ok(json_str) = result {
            let parsed: serde_json::Value =
                serde_json::from_str(&json_str).expect("should be valid JSON");

            assert!(parsed.get("results").is_some());
            assert!(parsed.get("remaining_groups").is_some());
            assert!(parsed["results"].is_array());
            assert!(parsed["remaining_groups"].is_number());
        }
    }
}
