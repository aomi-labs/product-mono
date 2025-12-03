use alloy_primitives::{keccak256, Bytes, U256};
use anyhow::Result;
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

        // 2. Get the actual groups
        let ready_groups: Vec<&OperationGroup> = ready_indices
            .iter()
            .map(|&idx| &self.plan.groups[idx])
            .collect();

        // 3. Wait for all contracts to be fetched
        // TODO: add a timeout
        while !self.source_fetcher.are_contracts_ready(&ready_groups).await {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
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
        // 1. Get contract sources
        let sources = source_fetcher.get_contracts_for_group(&group).await?;

        // 2. BAML Phase 1: Extract contract info
        let extracted_infos = baml_client
            .extract_contract_info(&group.operations, &sources)
            .await?;

        // 3. BAML Phase 2: Generate script
        let script_block = baml_client
            .generate_script(&group.operations, &extracted_infos)
            .await?;

        // 4. Assemble complete Forge script
        let config = AssemblyConfig::default();
        let generated_code = ScriptAssembler::assemble(vec![], &script_block, config)?;

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

        // 6. Deploy the script contract
        let script_address = session
            .deploy_contract(&format!("group_{}", group_idx), "AomiScript")
            .await?;

        // 7. Call the run() function
        let run_selector = keccak256("run()")[0..4].to_vec();
        let execution_result = session
            .call_contract(script_address, Bytes::from(run_selector), None)
            .await?;

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
}

impl Drop for ForgeExecutor {
    fn drop(&mut self) {
        self.shutdown();
    }
}
