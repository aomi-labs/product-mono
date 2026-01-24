use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, with_topic};
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use aomi_scripts::forge_executor::{ForgeManager, GroupResult, OperationGroup};

use tokio::sync::{OnceCell, mpsc};

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

impl AomiToolArgs for SetExecutionPlanParameters {
    fn schema() -> Value {
        with_topic(json!({
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
        }))
    }
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

impl AomiTool for SetExecutionPlan {
    const NAME: &'static str = "set_execution_plan";

    type Args = SetExecutionPlanParameters;
    type Output = Value;
    type Error = ToolError;

    fn support_async(&self) -> bool {
        true
    }

    fn description(&self) -> &'static str {
        "[Async Tool] Set the execution plan with operation groups and dependencies. This initializes the ForgeExecutor and starts background contract fetching, resturns asynchrounly after the plan is set."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        request: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<Value>> + Send {
        async move {
            build_execution_plan_result(request)
                .await
                .map(|result| {
                    serde_json::to_value(result)
                        .unwrap_or_else(|e| json!({"error": e.to_string()}))
                })
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }

    fn run_async(
        &self,
        sender: mpsc::Sender<(eyre::Result<Value>, bool)>,
        _ctx: ToolCallCtx,
        request: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            match build_execution_plan_result(request).await {
                Ok(result) => {
                    // Only send the actual result - sync ACK is handled by AomiToolWrapper
                    let payload = serde_json::to_value(result)
                        .map_err(|e| eyre::eyre!(format!("Failed to serialize result: {}", e)));
                    let _ = sender.send((payload, false)).await;
                }
                Err(err) => {
                    let _ = sender
                        .send((Err(eyre::eyre!(err.to_string())), false))
                        .await;
                }
            }
        }
    }
}

/// Parameters for NextGroups tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextGroupsParameters {
    /// The plan id returned by set_execution_plan
    pub plan_id: String,
}

impl AomiToolArgs for NextGroupsParameters {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "plan_id": {
                    "type": "string",
                    "description": "Plan id returned by set_execution_plan"
                }
            },
            "required": ["plan_id"]
        }))
    }
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

impl AomiTool for NextGroups {
    const NAME: &'static str = "next_groups";

    type Args = NextGroupsParameters;
    type Output = Value;
    type Error = ToolError;

    fn support_async(&self) -> bool {
        true
    }

    fn description(&self) -> &'static str {
        "[Async Tool] Execute the next batch of ready operation groups for a plan id (groups whose dependencies are satisfied). Returns transaction data and generated Solidity code for each group, returns asynchronously after the groups are executed."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        request: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<Value>> + Send {
        async move {
            build_next_groups_result(request)
                .await
                .map(|result| {
                    serde_json::to_value(result)
                        .unwrap_or_else(|e| json!({"error": e.to_string()}))
                })
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }

    fn run_async(
        &self,
        sender: mpsc::Sender<(eyre::Result<Value>, bool)>,
        _ctx: ToolCallCtx,
        request: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            // Sync ACK is handled by AomiToolWrapper - only send actual result
            match build_next_groups_result(request).await {
                Ok(result) => {
                    let payload = serde_json::to_value(result)
                        .map_err(|e| eyre::eyre!(format!("Failed to serialize result: {}", e)));
                    let _ = sender.send((payload, false)).await;
                }
                Err(err) => {
                    let _ = sender
                        .send((Err(eyre::eyre!(err.to_string())), false))
                        .await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NextGroups, NextGroupsParameters, SetExecutionPlan, SetExecutionPlanParameters};
    use aomi_scripts::forge_executor::OperationGroup;
    use aomi_tools::{AomiTool, CallMetadata, ToolCallCtx};
    use serde_json::Value;
    use std::{env, path::PathBuf};
    async fn run_sync_tool<T>(tool: T, args: T::Args) -> Result<Value, String>
    where
        T: AomiTool<Output = Value>,
    {
        let ctx = ToolCallCtx {
            session_id: "test_session".to_string(),
            metadata: CallMetadata::new(
                T::NAME.to_string(),
                T::NAMESPACE.to_string(),
                "test_call".to_string(),
                None,
                tool.is_async(),
            ),
        };
        tool.run_sync(ctx, args).await.map_err(|err| err.to_string())
    }

    fn skip_without_anthropic_api_key() -> bool {
        std::env::var("ANTHROPIC_API_KEY").is_err()
    }

    fn ensure_providers_toml() {
        if env::var("PROVIDERS_TOML").is_ok() {
            return;
        }

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for ancestor in manifest_dir.ancestors() {
            let candidate = ancestor.join("providers.toml");
            if candidate.exists() {
                unsafe {
                    env::set_var("PROVIDERS_TOML", candidate);
                }
                return;
            }
        }

        panic!("providers.toml not found in ancestors of CARGO_MANIFEST_DIR");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_set_execution_plan_success_with_serialization() {
        if skip_without_anthropic_api_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set (required for BAML client)");
            return;
        }
        ensure_providers_toml();
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
        let result = match run_sync_tool(tool, params).await {
            Ok(value) => value,
            Err(err) => {
                if err.contains("Failed to spawn Anvil instance") {
                    eprintln!("Skipping: Anvil not available ({err})");
                    return;
                }
                panic!("should succeed: {err}");
            }
        };

        let parsed = result;

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

        let result = run_sync_tool(tool, params).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("No execution plan found"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_next_groups_json_serialization() {
        if skip_without_anthropic_api_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set (required for BAML client)");
            return;
        }
        ensure_providers_toml();
        let groups = vec![OperationGroup {
            description: "Simple operation".to_string(),
            operations: vec!["do something".to_string()],
            dependencies: vec![],
            contracts: vec![],
        }];

        let set_tool = SetExecutionPlan;
        let set_params = SetExecutionPlanParameters { groups };

        let set_result = match run_sync_tool(set_tool, set_params).await {
            Ok(value) => value,
            Err(err) => {
                if err.contains("Failed to spawn Anvil instance") {
                    eprintln!("Skipping: Anvil not available ({err})");
                    return;
                }
                panic!("should set plan successfully: {err}");
            }
        };
        let plan_id = set_result["plan_id"]
            .as_str()
            .expect("plan_id should be string")
            .to_string();

        let next_tool = NextGroups;
        let next_params = NextGroupsParameters { plan_id };

        let result = run_sync_tool(next_tool, next_params).await;

        if let Ok(json_str) = result {
            let parsed = json_str;

            assert!(parsed.get("results").is_some());
            assert!(parsed.get("remaining_groups").is_some());
            assert!(parsed["results"].is_array());
            assert!(parsed["remaining_groups"].is_number());
        }
    }
}
