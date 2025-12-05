use alloy_primitives::{Address, Bytes, U256, hex};
use anyhow::Result;
use foundry_common::fmt::UIfmt;
use foundry_compilers::ProjectCompileOutput;
use foundry_evm::{
    backend::Backend, inspectors::cheatcodes::BroadcastableTransactions, opts::EvmOpts,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tracing::warn;

use super::{
    compiler::ContractCompiler,
    runner::{ContractRunner, ExecutionResult},
};

/// Configuration for contract operations (compilation and execution)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractConfig {
    /// Foundry Config (shared across components via Arc)
    #[serde(with = "arc_config_serde")]
    pub foundry_config: Arc<foundry_config::Config>,

    /// Compiler: Disable automatic solc version detection
    pub no_auto_detect: bool,

    /// EVM: Options for EVM execution (includes fork configuration)
    pub evm_opts: EvmOpts,
    /// EVM: Enable traces for contract execution
    pub traces: bool,
    /// EVM: Initial balance for the sender account
    pub initial_balance: Option<U256>,

    /// Session identifier
    pub id: Option<String>,
}

/// Custom serde module for Arc<Config>
mod arc_config_serde {
    use foundry_config::Config;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<S>(arc: &Arc<Config>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        arc.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<Config>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Config::deserialize(deserializer).map(Arc::new)
    }
}

impl ContractConfig {
    /// Create a new contract config with the given foundry config and sensible defaults
    pub fn new(foundry_config: foundry_config::Config, id: Option<String>) -> Self {
        Self {
            foundry_config: Arc::new(foundry_config),
            no_auto_detect: false,
            evm_opts: EvmOpts {
                memory_limit: 128 * 1024 * 1024, // 128MB memory limit
                ..Default::default()
            },
            traces: false,
            initial_balance: None,
            id,
        }
    }
}

impl Default for ContractConfig {
    fn default() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/contract");
        let foundry_config = foundry_config::Config::load_with_root(root.clone()).unwrap_or_else(
            |err| {
                warn!(
                    "Failed to load foundry.toml from {}: {}. Falling back to default config.",
                    root.display(),
                    err
                );
                foundry_config::Config::default()
            },
        );

        let mut evm_opts = EvmOpts {
            memory_limit: 128 * 1024 * 1024, // 128MB memory limit
            ..Default::default()
        };
        let fork_override = std::env::var("AOMI_FORK_RPC").ok();
        if let Some(url) = fork_override.or_else(|| foundry_config.eth_rpc_url.clone()) {
            evm_opts.fork_url = Some(url);
        }

        Self {
            foundry_config: Arc::new(foundry_config),
            no_auto_detect: false,
            evm_opts,
            traces: false,
            initial_balance: None,
            id: None,
        }
    }
}

/// A contract session that combines compilation and execution
pub struct ContractSession {
    /// The compiler instance
    pub compiler: ContractCompiler,
    /// The runner instance
    pub runner: Option<ContractRunner>,
    /// Contract configuration
    pub config: ContractConfig,
    /// Cached compilation outputs
    compiled_contracts: HashMap<String, ProjectCompileOutput>,
    /// Deployed contract addresses
    deployed_contracts: HashMap<String, Address>,
}

impl ContractSession {
    /// Create a new contract session
    pub async fn new(config: ContractConfig) -> Result<Self> {
        let compiler = ContractCompiler::new(&config)?;

        Ok(Self {
            compiler,
            runner: None,
            config,
            compiled_contracts: HashMap::new(),
            deployed_contracts: HashMap::new(),
        })
    }

    /// Create a new contract session with default configuration
    pub async fn default() -> Result<Self> {
        Self::new(ContractConfig::default()).await
    }

    /// Get or create the EVM runner
    pub async fn get_runner(&mut self) -> Result<&mut ContractRunner> {
        if self.runner.is_none() {
            self.runner = Some(ContractRunner::new(&self.config).await?);
        }
        Ok(self.runner.as_mut().unwrap())
    }

    /// Compile a contract from source code
    pub fn compile_source(
        &mut self,
        name: String,
        source_path: PathBuf,
        content: String,
    ) -> Result<&ProjectCompileOutput> {
        let output = self.compiler.compile_source(source_path, content)?;
        self.compiled_contracts.insert(name.clone(), output);
        Ok(self.compiled_contracts.get(&name).unwrap())
    }

    /// Compile a contract from a file
    pub fn compile_file(
        &mut self,
        name: String,
        file_path: PathBuf,
    ) -> Result<&ProjectCompileOutput> {
        let output = self.compiler.compile_file(file_path)?;
        self.compiled_contracts.insert(name.clone(), output);
        Ok(self.compiled_contracts.get(&name).unwrap())
    }

    /// Deploy a compiled contract
    pub async fn deploy_contract(
        &mut self,
        compilation_name: &str,
        contract_name: &str,
    ) -> Result<Address> {
        let output = self
            .compiled_contracts
            .get(compilation_name)
            .ok_or_else(|| {
                anyhow::anyhow!("No compilation found with name '{}'", compilation_name)
            })?;

        let bytecode = self.compiler.get_contract_bytecode(output, contract_name)?;
        let runner = self.get_runner().await?;

        let (address, _result) = runner.deploy(Bytes::from(bytecode))?;
        self.deployed_contracts
            .insert(format!("{}:{}", compilation_name, contract_name), address);

        Ok(address)
    }

    /// Call a function on a deployed contract
    pub async fn call_contract(
        &mut self,
        address: Address,
        calldata: Bytes,
        value: Option<U256>,
    ) -> Result<ExecutionResult> {
        let runner = self.get_runner().await?;
        runner.call(address, calldata, value.unwrap_or(U256::ZERO))
    }

    /// Call a function on a deployed contract without committing
    pub async fn call_contract_static(
        &mut self,
        address: Address,
        calldata: Bytes,
        value: Option<U256>,
    ) -> Result<ExecutionResult> {
        let runner = self.get_runner().await?;
        runner.call_static(address, calldata, value.unwrap_or(U256::ZERO))
    }

    /// Compile and deploy a contract in one operation
    pub async fn compile_and_deploy(
        &mut self,
        name: String,
        source_path: PathBuf,
        content: String,
        contract_name: &str,
    ) -> Result<Address> {
        self.compile_source(name.clone(), source_path, content)?;
        self.deploy_contract(&name, contract_name).await
    }

    /// Compile and deploy a contract from a file
    pub async fn compile_and_deploy_file(
        &mut self,
        name: String,
        file_path: PathBuf,
        contract_name: &str,
    ) -> Result<Address> {
        self.compile_file(name.clone(), file_path)?;
        self.deploy_contract(&name, contract_name).await
    }

    /// Get the address of a deployed contract
    pub fn get_deployed_address(
        &self,
        compilation_name: &str,
        contract_name: &str,
    ) -> Option<Address> {
        self.deployed_contracts
            .get(&format!("{}:{}", compilation_name, contract_name))
            .copied()
    }

    /// Get all deployed contract addresses
    pub fn get_all_deployed(&self) -> &HashMap<String, Address> {
        &self.deployed_contracts
    }

    /// Get the ABI for a deployed contract
    pub fn get_contract_abi(&self, compilation_name: &str, contract_name: &str) -> Result<String> {
        let output = self
            .compiled_contracts
            .get(compilation_name)
            .ok_or_else(|| {
                anyhow::anyhow!("No compilation found with name '{}'", compilation_name)
            })?;

        self.compiler.get_contract_abi(output, contract_name)
    }

    /// Get a compiled contract output
    pub fn get_compilation(&self, name: &str) -> Option<&ProjectCompileOutput> {
        self.compiled_contracts.get(name)
    }

    /// Get all compiled contracts
    pub fn get_all_compilations(&self) -> &HashMap<String, ProjectCompileOutput> {
        &self.compiled_contracts
    }

    /// Clear all compiled contracts
    pub fn clear_compilations(&mut self) {
        self.compiled_contracts.clear();
    }

    /// Clear all deployed contracts
    pub fn clear_deployments(&mut self) {
        self.deployed_contracts.clear();
    }

    /// Reset the session (clear all state)
    pub fn reset(&mut self) {
        self.compiled_contracts.clear();
        self.deployed_contracts.clear();
        self.runner = None;
    }

    /// Get the current EVM backend (if runner is initialized)
    pub fn get_backend(&self) -> Option<&Backend> {
        self.runner.as_ref().map(|r| r.executor.backend())
    }

    /// Set the sender address for transactions
    pub async fn set_sender(&mut self, sender: Address) -> Result<()> {
        let runner = self.get_runner().await?;
        runner.set_sender(sender);
        Ok(())
    }

    /// Get account balance
    pub async fn get_balance(&mut self, address: Address) -> Result<U256> {
        let runner = self.get_runner().await?;
        runner.get_balance(address)
    }

    /// Set account balance
    pub async fn set_balance(&mut self, address: Address, balance: U256) -> Result<()> {
        let runner = self.get_runner().await?;
        runner.set_balance(address, balance)
    }

    /// Get broadcastable transactions from the last execution result and optionally broadcast them
    ///
    /// This function extracts transactions that were recorded during `vm.startBroadcast()/vm.stopBroadcast()`
    /// calls in Forge scripts. When `broadcast` is true, these transactions are executed on the EVM backend.
    ///
    /// # Arguments
    /// * `execution_result` - The result from calling a contract (e.g., from `call_contract`)
    /// * `broadcast` - If true, execute the transactions on the EVM backend. If false, just return them for inspection.
    ///
    /// # Returns
    /// The list of broadcastable transactions
    pub async fn get_broadcastable_transactions(
        &mut self,
        execution_result: &ExecutionResult,
        broadcast: bool,
    ) -> Result<BroadcastableTransactions> {
        let transactions = execution_result.broadcastable_transactions.clone();

        if broadcast && !transactions.is_empty() {
            println!(
                "\n=== Broadcasting {} transactions to EVM backend ===",
                transactions.len()
            );

            let runner = self.get_runner().await?;

            for (idx, btx) in transactions.iter().enumerate() {
                println!("\n=== Transaction {} ===", idx + 1);

                // Print transaction details in a pretty format
                println!("{}", btx.transaction.pretty());

                if let Some(rpc) = &btx.rpc {
                    println!("\nRPC URL: {}", rpc);
                }
                println!();

                // Extract transaction details for execution
                let _from = btx
                    .transaction
                    .from()
                    .ok_or_else(|| anyhow::anyhow!("Transaction missing 'from' field"))?;
                let calldata = Bytes::from(
                    btx.transaction
                        .input()
                        .ok_or_else(|| anyhow::anyhow!("Transaction missing input data"))?
                        .to_vec(),
                );
                let value = btx.transaction.value().unwrap_or(U256::ZERO);

                // Check if this is a contract deployment or a regular call
                let to_kind = btx.transaction.to();

                // Execute the transaction on the backend
                println!("--- Execution Result ---");
                match to_kind {
                    Some(alloy_primitives::TxKind::Call(to_address)) => {
                        // Regular call
                        let result = runner.call(to_address, calldata, value)?;

                        if result.success {
                            println!("✓ Transaction executed successfully");
                            println!("  Gas used: {}", result.gas_used);
                            if !result.returned.is_empty() {
                                println!("  Return data: 0x{}", hex::encode(&result.returned));
                            }
                        } else {
                            println!("✗ Transaction FAILED");
                            if !result.returned.is_empty() {
                                println!("  Revert data: 0x{}", hex::encode(&result.returned));
                            }
                        }
                    }
                    Some(alloy_primitives::TxKind::Create) | None => {
                        // Contract deployment
                        let (deployed_address, result) = runner.deploy(calldata)?;

                        if result.success {
                            println!("✓ Contract deployed successfully");
                            println!("  Address: {:?}", deployed_address);
                            println!("  Gas used: {}", result.gas_used);
                        } else {
                            println!("✗ Deployment FAILED");
                        }
                    }
                }
            }

            println!("\n=== Broadcast complete ===\n");
        }

        Ok(transactions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, keccak256};

    fn decode_return(bytes: &Bytes) -> U256 {
        let mut buf = [0u8; 32];
        let src = bytes.as_ref();
        let len = src.len().min(32);
        buf[32 - len..].copy_from_slice(&src[src.len() - len..]);
        U256::from_be_bytes(buf)
    }

    async fn build_session() -> ContractSession {
        ContractSession::new(ContractConfig::default())
            .await
            .expect("session should build")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deploys_and_calls_compiled_contract() {
        let mut session = build_session().await;

        let source = r#"
            // SPDX-License-Identifier: UNLICENSED
            pragma solidity ^0.8.20;

            contract Constant {
                function value() external pure returns (uint256) {
                    return 42;
                }
            }
        "#;

        session
            .compile_source(
                "demo".to_string(),
                PathBuf::from("Constant.sol"),
                source.to_string(),
            )
            .expect("compile succeeds");

        let address = session
            .deploy_contract("demo", "Constant")
            .await
            .expect("deployment succeeds");
        assert_ne!(address, Address::ZERO);

        let selector = Bytes::from(keccak256("value()".as_bytes())[0..4].to_vec());

        let exec_result = session
            .call_contract(address, selector.clone(), None)
            .await
            .expect("call succeeds");
        assert!(exec_result.success);
        assert_eq!(decode_return(&exec_result.returned), U256::from(42u64));

        let static_result = session
            .call_contract_static(address, selector, None)
            .await
            .expect("static call succeeds");
        assert!(static_result.success);
        assert_eq!(decode_return(&static_result.returned), U256::from(42u64));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deploy_contract_errors_without_compilation() {
        let mut session = build_session().await;
        let err = session
            .deploy_contract("missing", "Constant")
            .await
            .expect_err("expected missing compilation error");
        assert!(err.to_string().contains("No compilation found"));
    }
}
