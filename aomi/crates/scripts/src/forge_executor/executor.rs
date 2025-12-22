use alloy_primitives::{Bytes, U256, keccak256};
use anyhow::{Result, anyhow};
use dashmap::DashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::contract::session::{ContractConfig, ContractSession};

use super::assembler::{AssemblyConfig, ScriptAssembler};
use super::plan::{ExecutionPlan, OperationGroup};
use super::resources::SharedForgeResources;
use super::source_fetcher::SourceFetcher;
use super::types::{GroupResult, GroupResultInner, TransactionData};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

const SHARED_SESSION_KEY: &str = "shared_session";

/// ForgeExecutor2 - stateful, dependency-aware executor
pub struct ForgeExecutor {
    pub plan: ExecutionPlan,
    source_fetcher: Arc<SourceFetcher>,
    baml_client: Arc<aomi_baml::BamlClient>,
    contract_config: ContractConfig,
    contract_sessions: Arc<DashMap<String, Arc<Mutex<ContractSession>>>>,
}

impl ForgeExecutor {
    /// Create new executor and start background source fetching
    pub async fn new(groups: Vec<OperationGroup>) -> Result<Self> {
        let shared = Arc::new(SharedForgeResources::new().await?);
        Self::new_with_resources(groups, shared).await
    }

    pub async fn new_with_resources(
        groups: Vec<OperationGroup>,
        shared: Arc<SharedForgeResources>,
    ) -> Result<Self> {
        let plan = ExecutionPlan::from(groups.clone());
        tracing::debug!("ForgeExecutor new with plan: {:?}", plan);

        let all_contracts = Self::collect_unique_contracts(&groups);
        let source_fetcher = shared.source_fetcher();
        source_fetcher.request_fetch(all_contracts);

        let target_chain_ids = Self::collect_target_chain_ids(&groups);
        let baml_client = shared.baml_client();
        let fork_snapshot = Self::init_fork_provider(&target_chain_ids).await?;
        let contract_config = Self::build_contract_config(&fork_snapshot);
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
        let ready_indices = self.plan.next_ready_batch();
        if ready_indices.is_empty() {
            return Ok(vec![]); // No more groups to execute
        }

        let ready_groups: Vec<&OperationGroup> = ready_indices
            .iter()
            .map(|&idx| &self.plan.groups[idx])
            .collect();

        Self::wait_for_contract_sources(&self.source_fetcher, &ready_indices, &ready_groups)
            .await?;

        self.plan.mark_in_progress(&ready_indices);

        let tasks = Self::spawn_group_tasks(
            &self.plan,
            &ready_indices,
            &self.source_fetcher,
            &self.baml_client,
            &self.contract_sessions,
            &self.contract_config,
        );

        Self::collect_group_results(&mut self.plan, &ready_indices, tasks).await
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
        baml_client: Arc<aomi_baml::BamlClient>,
        contract_sessions: Arc<DashMap<String, Arc<Mutex<ContractSession>>>>,
        contract_config: ContractConfig,
    ) -> Result<GroupResult> {
        tracing::info!(
            group_idx,
            description = %group.description,
            "starting group execution"
        );

        let sources = source_fetcher.get_contracts_for_group(&group).await?;
        tracing::info!(
            group_idx,
            source_count = sources.len(),
            "fetched contract sources"
        );

        let extracted_infos = Self::run_baml_extract(&baml_client, &group, &sources).await?;
        tracing::info!(
            group_idx,
            contract_count = extracted_infos.len(),
            "baml extract complete"
        );

        let script_block =
            Self::run_baml_generate_script(&baml_client, &group, &extracted_infos).await?;
        tracing::info!(group_idx, "baml script generation complete");
        tracing::debug!("script_block: {:?}", script_block);

        let generated_code = Self::assemble_script(&script_block)?;
        tracing::info!(
            group_idx,
            code_size = generated_code.len(),
            "assembly complete"
        );
        tracing::debug!("generated_code: {:?}", generated_code);

        // Optional fast path for tests: skip on-chain execution and just return the script.
        if std::env::var("FORGE_TEST_SKIP_EXECUTION").is_ok() {
            tracing::debug!(
                group_idx,
                "skipping execution (FORGE_TEST_SKIP_EXECUTION set)"
            );

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

        let script_path = PathBuf::from(format!("script_group_{}.sol", group_idx));
        let session =
            Self::get_or_create_shared_session(&contract_sessions, &contract_config, &script_path)
                .await?;
        let mut session = session.lock().await;

        let script_address = match Self::compile_and_deploy_script(
            &mut session,
            group_idx,
            &script_path,
            &generated_code,
        )
        .await
        {
            Ok(address) => address,
            Err(err) => {
                return Ok(Self::build_failed_result(
                    group_idx,
                    group,
                    err.to_string(),
                    generated_code,
                    vec![],
                ));
            }
        };

        if let Err(err) = Self::fund_broadcaster(&mut session, group_idx).await {
            return Ok(Self::build_failed_result(
                group_idx,
                group,
                err.to_string(),
                generated_code,
                vec![],
            ));
        }

        let execution_result =
            match Self::execute_run(&mut session, group_idx, script_address).await {
                Ok(result) => result,
                Err(err) => {
                    return Ok(Self::build_failed_result(
                        group_idx,
                        group,
                        err.to_string(),
                        generated_code,
                        vec![],
                    ));
                }
            };

        let has_transactions = !execution_result.broadcastable_transactions.is_empty();

        tracing::debug!(
            group_idx,
            has_transactions,
            tx_count = execution_result.broadcastable_transactions.len(),
            "checking for broadcastable transactions"
        );

        let broadcastable = execution_result.broadcastable_transactions.clone();

        let transactions = Self::build_transactions(&broadcastable);

        if !execution_result.success {
            let error_msg = if !execution_result.returned.is_empty() {
                let returned_hex = alloy_primitives::hex::encode(&execution_result.returned);
                if let Some(decoded) = decode_revert_reason(&execution_result.returned) {
                    format!("Script execution failed: {} (0x{})", decoded, returned_hex)
                } else {
                    format!("Script execution failed. Return data: 0x{}", returned_hex)
                }
            } else {
                "Script execution failed without revert data".to_string()
            };

            tracing::warn!(
                group_idx,
                error = %error_msg,
                tx_count = transactions.len(),
                "execution failed"
            );

            return Ok(Self::build_failed_result(
                group_idx,
                group,
                error_msg,
                generated_code,
                transactions,
            ));
        }

        Ok(Self::build_done_result(
            group_idx,
            group,
            generated_code,
            transactions,
        ))
    }

    /// Retry a fallible async operation a limited number of times with a fixed backoff.
    async fn with_retry<F, Fut, T>(mut f: F, attempts: usize, delay: Duration) -> Result<T>
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

    fn collect_unique_contracts(groups: &[OperationGroup]) -> Vec<(String, String, String)> {
        groups
            .iter()
            .flat_map(|g| g.contracts.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    fn collect_target_chain_ids(groups: &[OperationGroup]) -> HashSet<String> {
        groups
            .iter()
            .flat_map(|g| g.contracts.iter().map(|(chain_id, _, _)| chain_id.clone()))
            .collect()
    }

    async fn init_fork_provider(
        target_chain_ids: &HashSet<String>,
    ) -> Result<aomi_anvil::ForkSnapshot> {
        let explicit_fork_url = std::env::var("AOMI_FORK_RPC")
            .or_else(|_| std::env::var("ETH_RPC_URL"))
            .unwrap_or_else(|_| "http://localhost:8545".to_string());
        let fork_snapshot = if aomi_anvil::is_fork_provider_initialized() {
            aomi_anvil::fork_snapshot().ok_or_else(|| {
                anyhow!("Fork provider initialized but no snapshot is available; reset and retry")
            })?
        } else if !explicit_fork_url.is_empty() {
            tracing::info!(
                "Fork provider not initialized, using RPC from AOMI_FORK_RPC/ETH_RPC_URL or default localhost:8545"
            );
            aomi_anvil::from_external(explicit_fork_url.clone())
                .await
                .map_err(|e| {
                    anyhow!(
                        "Failed to initialize fork provider from {}: {}",
                        explicit_fork_url,
                        e
                    )
                })?
        } else {
            let requires_real_fork = target_chain_ids.iter().any(|id| id != "31337");
            if requires_real_fork {
                anyhow::bail!(
                    "No fork RPC configured (set AOMI_FORK_RPC or ETH_RPC_URL) \
                    but execution plan targets chain(s): {:?}",
                    target_chain_ids
                );
            }

            tracing::info!("Fork provider not initialized, using default local Anvil");
            aomi_anvil::init_fork_provider(aomi_anvil::ForksConfig::default())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize fork provider: {}", e))?
        };

        Ok(fork_snapshot)
    }

    fn build_contract_config(fork_snapshot: &aomi_anvil::ForkSnapshot) -> ContractConfig {
        let mut contract_config = ContractConfig::default();
        let fork_url = fork_snapshot.endpoint().to_string();

        contract_config.evm_opts.fork_url = Some(fork_url.clone());
        contract_config.evm_opts.no_storage_caching = true; // Disable caching to ensure fresh state
        tracing::info!("ForgeExecutor using fork URL: {}", fork_url);

        contract_config
    }

    async fn wait_for_contract_sources(
        source_fetcher: &SourceFetcher,
        ready_indices: &[usize],
        ready_groups: &[&OperationGroup],
    ) -> Result<()> {
        let wait_deadline = Instant::now();
        let wait_limit = Duration::from_secs(60);
        while !source_fetcher.are_contracts_ready(ready_groups).await {
            if wait_deadline.elapsed() > wait_limit {
                let missing = source_fetcher
                    .missing_contracts(ready_groups)
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

            let missing = source_fetcher.missing_contracts(ready_groups).await;
            if !missing.is_empty() {
                source_fetcher.request_fetch(missing);
            }

            sleep(Duration::from_millis(500)).await;
        }

        Ok(())
    }

    fn spawn_group_tasks(
        plan: &ExecutionPlan,
        ready_indices: &[usize],
        source_fetcher: &Arc<SourceFetcher>,
        baml_client: &Arc<aomi_baml::BamlClient>,
        contract_sessions: &Arc<DashMap<String, Arc<Mutex<ContractSession>>>>,
        contract_config: &ContractConfig,
    ) -> Vec<tokio::task::JoinHandle<Result<GroupResult>>> {
        let mut tasks = Vec::new();

        for &group_idx in ready_indices {
            let group = plan.groups[group_idx].clone();
            let source_fetcher = source_fetcher.clone();
            let baml_client = baml_client.clone();
            let contract_sessions = contract_sessions.clone();
            let contract_config = contract_config.clone();

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

        tasks
    }

    async fn collect_group_results(
        plan: &mut ExecutionPlan,
        ready_indices: &[usize],
        tasks: Vec<tokio::task::JoinHandle<Result<GroupResult>>>,
    ) -> Result<Vec<GroupResult>> {
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
                        plan.mark_done(group_idx, transactions.clone(), generated_code.clone());
                    }
                    results.push(result);
                }
                Ok(Err(e)) => {
                    plan.mark_failed(group_idx, e.to_string());
                    results.push(Self::build_failed_result(
                        group_idx,
                        plan.groups[group_idx].clone(),
                        e.to_string(),
                        String::new(),
                        vec![],
                    ));
                }
                Err(e) => {
                    plan.mark_failed(group_idx, e.to_string());
                    results.push(Self::build_failed_result(
                        group_idx,
                        plan.groups[group_idx].clone(),
                        e.to_string(),
                        String::new(),
                        vec![],
                    ));
                }
            }
        }

        Ok(results)
    }

    async fn run_baml_extract(
        baml_client: &aomi_baml::BamlClient,
        group: &OperationGroup,
        sources: &[aomi_baml::ContractSource],
    ) -> Result<Vec<aomi_baml::ExtractedContractInfo>> {
        let extracted_infos = Self::with_retry(
            || baml_client.extract_contract_info(&group.operations, sources),
            3,
            Duration::from_secs(8),
        )
        .await?;
        Ok(extracted_infos)
    }

    async fn run_baml_generate_script(
        baml_client: &aomi_baml::BamlClient,
        group: &OperationGroup,
        extracted_infos: &[aomi_baml::ExtractedContractInfo],
    ) -> Result<aomi_baml::ScriptBlock> {
        Self::with_retry(
            || baml_client.generate_script(&group.operations, extracted_infos),
            3,
            Duration::from_secs(8),
        )
        .await
    }

    fn assemble_script(script_block: &aomi_baml::ScriptBlock) -> Result<String> {
        let config = AssemblyConfig::default();
        ScriptAssembler::assemble(vec![], script_block, config)
    }

    async fn compile_and_deploy_script(
        session: &mut ContractSession,
        group_idx: usize,
        script_path: &Path,
        generated_code: &str,
    ) -> Result<alloy_primitives::Address> {
        session.compile_source(
            format!("group_{}", group_idx),
            script_path.to_path_buf(),
            generated_code.to_string(),
        )?;
        tracing::debug!(group_idx, "compilation finished");

        tracing::debug!(group_idx, "deploying script");
        let script_address = session
            .deploy_contract(&format!("group_{}", group_idx), "AomiScript")
            .await?;
        tracing::debug!(group_idx, address = ?script_address, "deployed script");
        Ok(script_address)
    }

    async fn execute_run(
        session: &mut ContractSession,
        group_idx: usize,
        script_address: alloy_primitives::Address,
    ) -> Result<crate::contract::runner::ExecutionResult> {
        let run_selector = keccak256("run()")[0..4].to_vec();
        let execution_result = session
            .call_contract(script_address, Bytes::from(run_selector), None)
            .await?;
        tracing::debug!(
            group_idx,
            success = execution_result.success,
            gas_used = execution_result.gas_used,
            returned_len = execution_result.returned.len(),
            "run() executed"
        );

        if !execution_result.success {
            tracing::warn!(
                group_idx,
                returned_hex = %alloy_primitives::hex::encode(&execution_result.returned),
                gas_used = execution_result.gas_used,
                logs_count = execution_result.logs.len(),
                traces_count = execution_result.traces.len(),
                "execution failed - details"
            );

            for (trace_kind, arena) in &execution_result.traces {
                tracing::debug!(
                    group_idx,
                    trace_kind = ?trace_kind,
                    nodes_count = arena.nodes().len(),
                    "execution trace"
                );
            }
        }

        Ok(execution_result)
    }

    async fn get_or_create_shared_session(
        contract_sessions: &Arc<DashMap<String, Arc<Mutex<ContractSession>>>>,
        contract_config: &ContractConfig,
        script_path: &PathBuf,
    ) -> Result<Arc<Mutex<ContractSession>>> {
        let session = if let Some(existing) = contract_sessions.get(SHARED_SESSION_KEY) {
            existing.clone()
        } else {
            let new_session = Arc::new(Mutex::new(
                ContractSession::new(contract_config.clone()).await?,
            ));
            contract_sessions.insert(SHARED_SESSION_KEY.to_string(), new_session.clone());
            tracing::info!("new shared session created for: {:?}", script_path);
            new_session
        };

        Ok(session)
    }

    async fn fund_broadcaster(session: &mut ContractSession, group_idx: usize) -> Result<()> {
        use alloy_primitives::Address as AlloyAddress;
        use std::str::FromStr;
        let broadcaster = AlloyAddress::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266")
            .expect("valid broadcaster address");
        session
            .set_balance(broadcaster, alloy_primitives::U256::MAX)
            .await?;
        session.set_sender(broadcaster).await?;
        tracing::debug!(
            group_idx,
            broadcaster = ?broadcaster,
            "funded broadcaster with unlimited ETH"
        );
        Ok(())
    }

    fn build_transactions(
        broadcastable: &foundry_evm::inspectors::cheatcodes::BroadcastableTransactions,
    ) -> Vec<TransactionData> {
        broadcastable
            .iter()
            .map(|btx| TransactionData {
                from: btx.transaction.from().map(|addr| format!("{:#x}", addr)),
                to: btx.transaction.to().and_then(|kind| match kind {
                    alloy_primitives::TxKind::Call(addr) => Some(format!("{:#x}", addr)),
                    alloy_primitives::TxKind::Create => None,
                }),
                value: format!("0x{:x}", btx.transaction.value().unwrap_or(U256::ZERO)),
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
            .collect()
    }

    fn build_failed_result(
        group_idx: usize,
        group: OperationGroup,
        error_msg: String,
        generated_code: String,
        transactions: Vec<TransactionData>,
    ) -> GroupResult {
        GroupResult {
            group_index: group_idx,
            description: group.description,
            operations: group.operations,
            inner: GroupResultInner::Failed {
                error: error_msg,
                generated_code,
                transactions,
            },
        }
    }

    fn build_done_result(
        group_idx: usize,
        group: OperationGroup,
        generated_code: String,
        transactions: Vec<TransactionData>,
    ) -> GroupResult {
        GroupResult {
            group_index: group_idx,
            description: group.description,
            operations: group.operations,
            inner: GroupResultInner::Done {
                transactions,
                generated_code,
            },
        }
    }
}

/// Attempt to decode a standard Error(string) revert reason.
fn decode_revert_reason(data: &[u8]) -> Option<String> {
    const ERROR_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0];
    if data.len() < 4 || data[..4] != ERROR_SELECTOR {
        return None;
    }
    // Skip selector and offset (first 36 bytes), then length-prefixed string.
    if data.len() < 68 {
        return None;
    }
    let mut len_bytes = [0u8; 32];
    len_bytes.copy_from_slice(&data[36..68]);
    let str_len = U256::from_be_bytes(len_bytes).to::<usize>();
    let start: usize = 68;
    let end = start.saturating_add(str_len);
    if end > data.len() {
        return None;
    }
    Some(String::from_utf8_lossy(&data[start..end]).into_owned())
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
        // Attempt to call NextGroups without setting a plan first
        let tool = NextGroups;
        let params = NextGroupsParameters {
            plan_id: "missing-plan".to_string(),
        };

        let result = tool.call(params).await;

        // Should return an error
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

        // Now call NextGroups (it will fail to execute due to missing BAML/contracts, but we can test serialization)
        let next_tool = NextGroups;
        let next_params = NextGroupsParameters { plan_id };

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
                generated_code: String::new(),
                transactions: vec![],
            },
        };

        let json = serde_json::to_string(&failed_result).expect("should serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");

        assert_eq!(parsed["group_index"], 1);
        assert!(parsed["inner"]["Failed"].is_object());
        assert_eq!(parsed["inner"]["Failed"]["error"], "Contract not found");
        assert!(parsed["inner"]["Failed"]["transactions"].is_array());
    }
}
