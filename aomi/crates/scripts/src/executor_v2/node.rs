use alloy_primitives::{Address, Bytes, keccak256, U256};
use eyre::{eyre, Result};
use foundry_evm::traces::TraceKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::{GroupArtifacts, GroupConfig, ExecutionBackend};
use crate::contract::compiler::ContractCompiler;
use crate::contract::runner::ExecutionResult;
use crate::contract::session::ContractConfig;
use crate::forge_executor::source_fetcher::SourceFetcher;
use crate::forge_executor::types::{GroupResult, GroupResultInner, TransactionData};
use crate::forge_executor::plan::OperationGroup;
use crate::forge_executor::assembler::{AssemblyConfig, ScriptAssembler};

enum ReviewDecision {
    Accept,
    #[allow(dead_code)]
    NeedsEdit(String),
}

enum AuditDecision {
    Accept,
    #[allow(dead_code)]
    Restart(String),
}

pub struct GroupNode {
    /// Group identifier
    pub group_id: String,

    /// The operation group being executed (JSON strings of to-do list)
    pub group: OperationGroup,

    /// This group's compiled contracts and deployments
    pub artifacts: GroupArtifacts,

    /// OWNED config (NOT shared, NOT Arc)
    config: GroupConfig,

    /// SHARED backend reference (per-plan)
    backend: Arc<ExecutionBackend>,

    /// SHARED global resources
    source_fetcher: Arc<SourceFetcher>,
    baml_client: Arc<aomi_baml::BamlClient>,
}

impl GroupNode {
    pub fn new(
        group_id: String,
        group: OperationGroup,
        config: GroupConfig,
        backend: Arc<ExecutionBackend>,
        source_fetcher: Arc<SourceFetcher>,
        baml_client: Arc<aomi_baml::BamlClient>,
    ) -> Self {
        Self {
            group_id,
            group,
            artifacts: GroupArtifacts::new(),
            config,
            backend,
            source_fetcher,
            baml_client,
        }
    }

    /// Main execution loop - runs to completion or failure
    pub async fn run(mut self) -> GroupResult {
        let group_idx = self.group_id.parse::<usize>().unwrap_or(0);

        tracing::info!(
            group_idx,
            description = %self.group.description,
            "starting group execution"
        );

        // 1. Fetch contract sources
        let sources = match self.fetch_sources().await {
            Ok(s) => {
                tracing::info!(
                    group_idx,
                    source_count = s.len(),
                    "fetched contract sources"
                );
                s
            }
            Err(e) => return self.build_failed_result(e.to_string(), String::new(), vec![]),
        };

        // 2. BAML generate (optimistic path)
        let mut generated_code = match self.generate_script(&sources).await {
            Ok(code) => code,
            Err(e) => return self.build_failed_result(e.to_string(), String::new(), vec![]),
        };

        if let ReviewDecision::NeedsEdit(reason) = self.review_script(&generated_code, &sources) {
            match self.edit_script(&generated_code, &reason).await {
                Ok(code) => generated_code = code,
                Err(e) => return self.build_failed_result(e.to_string(), String::new(), vec![]),
            }
        }

        // Optional fast path for tests: skip on-chain execution
        if std::env::var("FORGE_TEST_SKIP_EXECUTION").is_ok() {
            tracing::debug!(
                group_idx,
                "skipping execution (FORGE_TEST_SKIP_EXECUTION set)"
            );

            return GroupResult {
                group_index: group_idx,
                description: self.group.description.clone(),
                operations: self.group.operations.clone(),
                inner: GroupResultInner::Done {
                    transactions: vec![],
                    generated_code,
                },
            };
        }

        // 3. Compile and deploy (optimistic path)
        let bytecode = match self.compile_script(&generated_code).await {
            Ok(bytecode) => bytecode,
            Err(e) => return self.build_failed_result(e.to_string(), generated_code, vec![]),
        };

        let script_address = match self.deploy_script(bytecode).await {
            Ok(addr) => {
                tracing::debug!(group_idx, address = ?addr, "deployed script");
                addr
            }
            Err(e) => return self.build_failed_result(e.to_string(), generated_code, vec![]),
        };

        // 4. Execute run()
        let execution_result = match self.execute_script(script_address).await {
            Ok(result) => {
                tracing::debug!(
                    group_idx,
                    success = result.success,
                    gas_used = result.gas_used,
                    returned_len = result.returned.len(),
                    "run() executed"
                );
                result
            }
            Err(e) => return self.build_failed_result(e.to_string(), generated_code, vec![]),
        };

        if let AuditDecision::Restart(reason) = self.audit_results(&execution_result) {
            return self.build_failed_result(reason, generated_code, vec![]);
        }

        // 5. Build transactions
        let transactions = self.build_transactions(&execution_result);

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

            return self.build_failed_result(error_msg, generated_code, transactions);
        }

        GroupResult {
            group_index: group_idx,
            description: self.group.description.clone(),
            operations: self.group.operations.clone(),
            inner: GroupResultInner::Done {
                transactions,
                generated_code,
            },
        }
    }

    pub async fn fetch_sources(&self) -> Result<Vec<aomi_baml::ContractSource>> {
        self.source_fetcher
            .get_contracts_for_group(&self.group)
            .await
            .map_err(|e| eyre!("Failed to fetch sources: {}", e))
    }

    async fn generate_script(
        &self,
        sources: &[aomi_baml::ContractSource],
    ) -> Result<String> {
        let extracted_infos = self.run_baml_extract(sources).await?;
        let script_block = self.run_baml_generate_script(&extracted_infos).await?;
        self.assemble_script(&script_block)
    }

    fn review_script(
        &self,
        _generated_code: &str,
        _sources: &[aomi_baml::ContractSource],
    ) -> ReviewDecision {
        ReviewDecision::Accept
    }

    async fn edit_script(&self, _generated_code: &str, _reason: &str) -> Result<String> {
        unimplemented!("ScriptApp edit loop not wired yet");
    }

    async fn run_baml_extract(
        &self,
        sources: &[aomi_baml::ContractSource],
    ) -> Result<Vec<aomi_baml::ExtractedContractInfo>> {
        Self::with_retry(
            || async {
                self.baml_client
                    .extract_contract_info(&self.group.operations, sources)
                    .await
                    .map_err(|e| eyre!("BAML extract failed: {}", e))
            },
            3,
            Duration::from_secs(8),
        )
        .await
    }

    async fn run_baml_generate_script(
        &self,
        extracted_infos: &[aomi_baml::ExtractedContractInfo],
    ) -> Result<aomi_baml::ScriptBlock> {
        Self::with_retry(
            || async {
                self.baml_client
                    .generate_script(&self.group.operations, extracted_infos)
                    .await
                    .map_err(|e| eyre!("BAML generate script failed: {}", e))
            },
            3,
            Duration::from_secs(8),
        )
        .await
    }

    fn assemble_script(&self, script_block: &aomi_baml::ScriptBlock) -> Result<String> {
        let config = AssemblyConfig::default();
        ScriptAssembler::assemble(vec![], script_block, config)
            .map_err(|e| eyre!("Failed to assemble script: {}", e))
    }

    pub async fn compile_script(&mut self, generated_code: &str) -> Result<Vec<u8>> {
        // Compile using local compiler (per-node, not shared)
        let contract_config = self.config_as_contract_config();
        let compiler = ContractCompiler::new(&contract_config)
            .map_err(|e| eyre!("Failed to create compiler: {}", e))?;

        let script_path = PathBuf::from(format!("script_{}.sol", self.group_id));
        let output = compiler
            .compile_source(script_path.clone(), generated_code.to_string())
            .map_err(|e| eyre!("Failed to compile: {}", e))?;

        self.artifacts.add_compilation(format!("group_{}", self.group_id), output.clone());
        tracing::debug!(group_id = %self.group_id, "compilation finished");

        let bytecode = compiler
            .get_contract_bytecode(&output, "AomiScript")
            .map_err(|e| eyre!("Failed to get bytecode: {}", e))?;

        Ok(bytecode)
    }

    async fn deploy_script(&mut self, bytecode: Vec<u8>) -> Result<Address> {
        // Deploy using shared backend
        let chain_id = self.get_primary_chain_id()?;
        let sender = Address::from_slice(&hex::decode("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap());

        let address = self
            .backend
            .execute_on_chain(chain_id, sender, Bytes::from(bytecode.clone()), |exec| {
                exec.set_balance(sender, U256::MAX)?;
                let deploy_result =
                    exec.deploy(sender, Bytes::from(bytecode), U256::ZERO, None)?;
                Ok(deploy_result.address)
            })
            .await?;

        self.artifacts.add_deployment(format!("script:AomiScript"), address);

        Ok(address)
    }

    async fn execute_script(&self, script_address: Address) -> Result<ExecutionResult> {
        let chain_id = self.get_primary_chain_id()?;
        let run_selector = keccak256("run()")[0..4].to_vec();

        let calldata = Bytes::from(run_selector);

        self.execute_contract_on_chain(chain_id, script_address, calldata, U256::ZERO)
            .await
    }

    pub async fn execute_contract_on_chain(
        &self,
        chain_id: u64,
        contract: Address,
        calldata: Bytes,
        value: U256,
    ) -> Result<ExecutionResult> {
        let sender = Address::from_slice(&hex::decode("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap());
        let calldata_copy = calldata.clone();

        self.backend
            .execute_on_chain(chain_id, sender, calldata, |exec| {
                let raw_result =
                    exec.call_raw(sender, contract, calldata_copy, value)?;

                Ok(ExecutionResult {
                    success: !raw_result.reverted,
                    logs: raw_result.logs,
                    traces: raw_result
                        .traces
                        .map(|t| vec![(TraceKind::Execution, t)])
                        .unwrap_or_default(),
                    gas_used: raw_result.gas_used,
                    labeled_addresses: raw_result.labels,
                    returned: raw_result.result,
                    address: Some(contract),
                    state: raw_result.chisel_state,
                    broadcastable_transactions: raw_result.transactions.unwrap_or_default(),
                })
            })
            .await
    }

    pub async fn fetch_contract_source(
        &self,
        chain_id: u64,
        address: &str,
        name: Option<&str>,
    ) -> Result<aomi_baml::ContractSource> {
        let name = name.unwrap_or("Contract").to_string();
        self.source_fetcher
            .fetch_contract_now(chain_id.to_string(), address.to_string(), name)
            .await
            .map_err(|e| eyre!("Failed to fetch contract: {}", e))
    }

    fn audit_results(&self, _execution_result: &ExecutionResult) -> AuditDecision {
        AuditDecision::Accept
    }

    fn build_transactions(&self, execution_result: &ExecutionResult) -> Vec<TransactionData> {
        execution_result
            .broadcastable_transactions
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
        &self,
        error: String,
        generated_code: String,
        transactions: Vec<TransactionData>,
    ) -> GroupResult {
        let group_idx = self.group_id.parse::<usize>().unwrap_or(0);
        GroupResult {
            group_index: group_idx,
            description: self.group.description.clone(),
            operations: self.group.operations.clone(),
            inner: GroupResultInner::Failed {
                error,
                generated_code,
                transactions,
            },
        }
    }

    fn get_primary_chain_id(&self) -> Result<u64> {
        self.group
            .contracts
            .first()
            .map(|(chain_id, _, _)| chain_id.parse::<u64>())
            .unwrap_or(Ok(1))
            .map_err(|e| eyre!("Invalid chain_id: {}", e))
    }

    fn config_as_contract_config(&self) -> ContractConfig {
        ContractConfig {
            foundry_config: Arc::new(self.config.foundry_config.clone()),
            no_auto_detect: self.config.no_auto_detect,
            evm_opts: self.config.evm_opts.clone(),
            traces: false,
            initial_balance: None,
            id: self.config.id.clone(),
        }
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
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| eyre!("operation failed")))
    }
}

/// Helper to decode revert reason from raw bytes
fn decode_revert_reason(data: &Bytes) -> Option<String> {
    if data.len() < 4 {
        return None;
    }

    // Check for Error(string) selector: 0x08c379a0
    let error_selector = &data[0..4];
    if error_selector == [0x08, 0xc3, 0x79, 0xa0] && data.len() > 68 {
        // Try to extract the string from ABI encoding
        // Skip selector (4) + offset (32) + length (32) = 68 bytes
        let string_start = 68;
        if let Some(string_data) = data.get(string_start..) {
            if let Ok(s) = String::from_utf8(string_data.to_vec()) {
                // Trim null bytes
                return Some(s.trim_matches('\0').to_string());
            }
        }
    }

    None
}
