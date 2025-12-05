use alloy_primitives::{keccak256, Bytes, U256};
use anyhow::{anyhow, Result};
use dashmap::DashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::clients::external_clients;
use crate::contract::session::{ContractConfig, ContractSession};
use crate::forge_script_builder::AssemblyConfig;

use super::assembler::ScriptAssembler;
use super::plan::{ExecutionPlan, OperationGroup};
use super::source_fetcher::SourceFetcher;
use super::types::{GroupResult, GroupResultInner, TransactionData};
use tokio::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// ForgeExecutor2 - stateful, dependency-aware executor
pub struct ForgeExecutor {
    pub plan: ExecutionPlan,
    source_fetcher: Arc<SourceFetcher>,
    baml_client: Arc<crate::baml::BamlClient>,
    contract_config: ContractConfig,
    contract_sessions: Arc<DashMap<String, Arc<Mutex<ContractSession>>>>,
}

impl ForgeExecutor {
    /// Create new executor and start background source fetching
    pub async fn new(groups: Vec<OperationGroup>) -> Result<Self> {
        let plan = ExecutionPlan::from(groups.clone());

        // Extract all unique contracts
        let all_contracts: Vec<(String, String, String)> = groups
            .iter()
            .flat_map(|g| g.contracts.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        // Create long-running source fetcher service
        let source_fetcher = Arc::new(SourceFetcher::new());

        // Submit fetch requests immediately (non-blocking)
        source_fetcher.request_fetch(all_contracts);

        // Create BAML client from global external clients
        let clients = external_clients().await;
        let baml_client = clients
            .baml_client()
            .map_err(|e| anyhow::anyhow!("BAML client unavailable: {}", e))?;

        // Create contract config using repo foundry.toml
        let contract_config = ContractConfig::default();
        let contract_sessions = Arc::new(DashMap::new());

        Ok(Self {
            plan,
            source_fetcher,
            baml_client,
            contract_config,
            contract_sessions,
        })
    }

    /// Execute next batch of ready groups concurrently
    pub async fn next_groups(&mut self) -> Result<Vec<GroupResult>> {
        // 1. Get indices of ready groups
        let ready_indices = self.plan.next_ready_batch();
        if ready_indices.is_empty() {
            return Ok(vec![]); // No more groups to execute
        }
        let skip_baml = std::env::var("AOMI_SKIP_BAML").is_ok();

        // 2. Get the actual groups
        let ready_groups: Vec<&OperationGroup> = ready_indices
            .iter()
            .map(|&idx| &self.plan.groups[idx])
            .collect();

        // 3. Wait for all contracts to be fetched unless offline mode is enabled
        if !skip_baml {
            let wait_deadline = Instant::now();
            let wait_limit = Duration::from_secs(60);
            while !self.source_fetcher.are_contracts_ready(&ready_groups).await {
                if wait_deadline.elapsed() > wait_limit {
                    let missing = self
                        .source_fetcher
                        .missing_contracts(&ready_groups)
                        .await
                        .iter()
                        .map(|(chain, addr, name)| format!("{}:{} ({})", chain, addr, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    anyhow::bail!(
                        "Timed out waiting for contract sources for groups {:?}. Missing: {}",
                        ready_indices,
                        missing
                    );
                }

                // Re-request missing contracts in case prior fetch failed.
                let missing = self.source_fetcher.missing_contracts(&ready_groups).await;
                if !missing.is_empty() {
                    self.source_fetcher.request_fetch(missing);
                }

                sleep(Duration::from_millis(500)).await;
            }
        }

        // 4. Mark groups as in progress
        self.plan.mark_in_progress(&ready_indices);

        // 5. Execute all groups concurrently
        let mut tasks = Vec::new();

        for &group_idx in &ready_indices {
            let group = self.plan.groups[group_idx].clone();
            let source_fetcher = self.source_fetcher.clone();
            let baml_client = self.baml_client.clone();
            let contract_sessions = self.contract_sessions.clone();
            let contract_config = self.contract_config.clone();

            let task = tokio::spawn(async move {
                Self::execute_single_group(
                    group_idx,
                    group,
                    source_fetcher,
                    baml_client,
                    contract_sessions,
                    contract_config,
                )
                .await
            });

            tasks.push(task);
        }

        // 6. Wait for all tasks to complete
        // TODO: make it async
        let mut results = Vec::new();
        for (i, task) in tasks.into_iter().enumerate() {
            let group_idx = ready_indices[i];
            match task.await {
                Ok(Ok(result)) => {
                    if let GroupResultInner::Done {
                        ref transactions,
                        ref generated_code,
                    } = result.inner
                    {
                        self.plan
                            .mark_done(group_idx, transactions.clone(), generated_code.clone());
                    }
                    results.push(result);
                }
                Ok(Err(e)) => {
                    self.plan.mark_failed(group_idx, e.to_string());
                    results.push(GroupResult {
                        group_index: group_idx,
                        description: self.plan.groups[group_idx].description.clone(),
                        operations: self.plan.groups[group_idx].operations.clone(),
                        inner: GroupResultInner::Failed {
                            error: e.to_string(),
                        },
                    });
                }
                Err(e) => {
                    self.plan.mark_failed(group_idx, e.to_string());
                    results.push(GroupResult {
                        group_index: group_idx,
                        description: self.plan.groups[group_idx].description.clone(),
                        operations: self.plan.groups[group_idx].operations.clone(),
                        inner: GroupResultInner::Failed {
                            error: e.to_string(),
                        },
                    });
                }
            }
        }

        Ok(results)
    }

    /// Stop background workers and drop cached sessions.
    pub fn shutdown(&self) {
        self.source_fetcher.shutdown();
        self.contract_sessions.clear();
    }

    /// Execute a single group (called concurrently)
    async fn execute_single_group(
        group_idx: usize,
        group: OperationGroup,
        source_fetcher: Arc<SourceFetcher>,
        baml_client: Arc<crate::baml::BamlClient>,
        contract_sessions: Arc<DashMap<String, Arc<Mutex<ContractSession>>>>,
        contract_config: ContractConfig,
    ) -> Result<GroupResult> {
        let debug = std::env::var("AOMI_DEBUG_EXECUTOR").is_ok();
        let skip_baml = std::env::var("AOMI_SKIP_BAML").is_ok();
        if debug {
            println!("group {group_idx}: starting execution - {}", group.description);
        }
        if skip_baml {
            if debug {
                println!("group {group_idx}: skipping BAML (AOMI_SKIP_BAML set)");
            }
            let generated_code = "pragma solidity ^0.8.20; contract AomiScript { function run() public {} }".to_string();
            return Ok(GroupResult {
                group_index: group_idx,
                description: group.description,
                operations: group.operations,
                inner: GroupResultInner::Done {
                    transactions: vec![],
                    generated_code,
                },
            });
        }

        // 1. Get contract sources
        let sources = source_fetcher.get_contracts_for_group(&group).await?;
        if debug {
            println!(
                "group {group_idx}: fetched {} contract sources",
                sources.len()
            );
        }

        // 2. BAML Phase 1: Extract contract info
        let extracted_infos = Self::with_retry(|| {
            baml_client.extract_contract_info(&group.operations, &sources)
        }, 3, Duration::from_secs(8)).await?;
        if debug {
            println!(
                "group {group_idx}: baml extract complete ({} contracts)",
                extracted_infos.len()
            );
        }

        // 3. BAML Phase 2: Generate script
        let script_block = Self::with_retry(|| {
            baml_client.generate_script(&group.operations, &extracted_infos)
        }, 3, Duration::from_secs(8)).await?;
        if debug {
            println!("group {group_idx}: baml script generation complete");
        }

        // 4. Assemble complete Forge script
        let config = AssemblyConfig::default();
        let generated_code = ScriptAssembler::assemble(vec![], &script_block, config)?;
        if debug {
            println!(
                "group {group_idx}: assembly complete ({} bytes)",
                generated_code.len()
            );
        }

        // Optional fast path for tests: skip on-chain execution and just return the script.
        if std::env::var("AOMI_SKIP_EXECUTION").is_ok() {
            if debug {
                println!("group {group_idx}: skipping execution (AOMI_SKIP_EXECUTION set)");
            }
            return Ok(GroupResult {
                group_index: group_idx,
                description: group.description,
                operations: group.operations,
                inner: GroupResultInner::Done {
                    transactions: vec![],
                    generated_code,
                },
            });
        }

        // 5. Compile the script
        let script_path = PathBuf::from(format!("script_group_{}.sol", group_idx));
        let session_key = format!("group_{}", group_idx);
        let session = if let Some(existing) = contract_sessions.get(&session_key) {
            existing.clone()
        } else {
            let new_session =
                Arc::new(Mutex::new(ContractSession::new(contract_config.clone()).await?));
            contract_sessions.insert(session_key.clone(), new_session.clone());
            new_session
        };
        let mut session = session.lock().await;

        session.compile_source(
            format!("group_{}", group_idx),
            script_path.clone(),
            generated_code.clone(),
        )?;
        if debug {
            println!("group {group_idx}: compilation finished");
        }

        // 6. Deploy the script contract
        if debug {
            println!("group {group_idx}: deploying script");
        }
        let script_address = session
            .deploy_contract(&format!("group_{}", group_idx), "AomiScript")
            .await?;
        if debug {
            println!("group {group_idx}: deployed script at {script_address:?}");
        }

        // 7. Call the run() function
        let run_selector = keccak256("run()")[0..4].to_vec();
        if debug {
            println!("group {group_idx}: invoking run()");
        }
        let execution_result = session
            .call_contract(script_address, Bytes::from(run_selector), None)
            .await?;
        if debug {
            println!("group {group_idx}: run() executed (success: {})", execution_result.success);
        }

        // 8. Extract broadcastable transactions
        let transactions = execution_result
            .broadcastable_transactions
            .iter()
            .map(|btx| TransactionData {
                from: btx
                    .transaction
                    .from()
                    .map(|addr| format!("{:#x}", addr)),
                to: btx.transaction.to().and_then(|kind| match kind {
                    alloy_primitives::TxKind::Call(addr) => Some(format!("{:#x}", addr)),
                    alloy_primitives::TxKind::Create => None,
                }),
                value: format!(
                    "0x{:x}",
                    btx.transaction
                        .value()
                        .unwrap_or(U256::ZERO)
                ),
                data: format!(
                    "0x{}",
                    alloy_primitives::hex::encode(
                        btx.transaction.input().unwrap_or(&Default::default())
                    )
                ),
                rpc_url: btx
                    .rpc
                    .clone()
                    .or_else(|| std::env::var("AOMI_FORK_RPC").ok())
                    .unwrap_or_default(),
            })
            .collect();

        Ok(GroupResult {
            group_index: group_idx,
            description: group.description,
            operations: group.operations,
            inner: GroupResultInner::Done {
                transactions,
                generated_code,
            },
        })
    }

    /// Retry a fallible async operation a limited number of times with a fixed backoff.
    async fn with_retry<F, Fut, T>(
        mut f: F,
        attempts: usize,
        delay: Duration,
    ) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last_err = None;
        for attempt in 0..attempts {
            match f().await {
                Ok(res) => return Ok(res),
                Err(e) => {
                    last_err = Some(e);
                    if attempt + 1 < attempts {
                        sleep(delay).await;
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("operation failed")))
    }
}

impl Drop for ForgeExecutor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use crate::forge_executor::plan::OperationGroup;
    use crate::forge_executor::tools::{
        NextGroups, NextGroupsParameters, SetExecutionPlan, SetExecutionPlanParameters,
    };
    use crate::forge_executor::types::{GroupResult, GroupResultInner, TransactionData};
    use rig::tool::Tool;
    use serde_json;

    #[tokio::test]
    async fn test_set_execution_plan_success_with_serialization() {
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
                dependencies: vec![0], // Depends on group 0
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

        let params = SetExecutionPlanParameters {
            groups: groups.clone(),
        };

        let tool = SetExecutionPlan;
        let result = tool.call(params).await.expect("should succeed");

        // Verify it's valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("should be valid JSON");

        // Verify structure
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["total_groups"], 2);
        assert!(parsed["message"]
            .as_str()
            .unwrap()
            .contains("Background contract fetching started"));
    }

    #[tokio::test]
    async fn test_next_groups_no_plan_error() {
        // Attempt to call NextGroups without setting a plan first
        let tool = NextGroups;
        let params = NextGroupsParameters {};

        let result = tool.call(params).await;

        // Should return an error
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("No execution plan set"));
    }

    #[tokio::test]
    async fn test_next_groups_json_serialization() {
        // First, set up a plan
        let groups = vec![OperationGroup {
            description: "Simple operation".to_string(),
            operations: vec!["do something".to_string()],
            dependencies: vec![],
            contracts: vec![],
        }];

        let set_tool = SetExecutionPlan;
        let set_params = SetExecutionPlanParameters {
            groups: groups.clone(),
        };

        set_tool
            .call(set_params)
            .await
            .expect("should set plan successfully");

        // Now call NextGroups (it will fail to execute due to missing BAML/contracts, but we can test serialization)
        let next_tool = NextGroups;
        let next_params = NextGroupsParameters {};

        let result = next_tool.call(next_params).await;

        // The call will likely fail due to missing dependencies, but if it returns a result,
        // verify it's valid JSON
        if let Ok(json_str) = result {
            let parsed: serde_json::Value =
                serde_json::from_str(&json_str).expect("should be valid JSON");

            // Verify structure
            assert!(parsed.get("results").is_some());
            assert!(parsed.get("remaining_groups").is_some());

            // results should be an array
            assert!(parsed["results"].is_array());

            // remaining_groups should be a number
            assert!(parsed["remaining_groups"].is_number());
        }
    }

    #[test]
    fn test_group_result_serialization() {
        // Test Done variant
        let done_result = GroupResult {
            group_index: 0,
            description: "Test operation".to_string(),
            operations: vec!["op1".to_string(), "op2".to_string()],
            inner: GroupResultInner::Done {
                transactions: vec![TransactionData {
                    from: Some("0x123".to_string()),
                    to: Some("0x456".to_string()),
                    value: "0x1000".to_string(),
                    data: "0xabcd".to_string(),
                    rpc_url: "http://localhost:8545".to_string(),
                }],
                generated_code: "pragma solidity ^0.8.0;".to_string(),
            },
        };

        let json = serde_json::to_string(&done_result).expect("should serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");

        assert_eq!(parsed["group_index"], 0);
        assert_eq!(parsed["description"], "Test operation");
        assert_eq!(parsed["operations"].as_array().unwrap().len(), 2);
        assert!(parsed["inner"]["Done"].is_object());
        assert_eq!(
            parsed["inner"]["Done"]["transactions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        // Test Failed variant
        let failed_result = GroupResult {
            group_index: 1,
            description: "Failed operation".to_string(),
            operations: vec!["bad_op".to_string()],
            inner: GroupResultInner::Failed {
                error: "Contract not found".to_string(),
            },
        };

        let json = serde_json::to_string(&failed_result).expect("should serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");

        assert_eq!(parsed["group_index"], 1);
        assert!(parsed["inner"]["Failed"].is_object());
        assert_eq!(parsed["inner"]["Failed"]["error"], "Contract not found");
    }
}
