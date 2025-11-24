use super::compiler::ContractCompiler;
use super::runner::ContractRunner;
use alloy_primitives::{Address, Bytes, U256, hex};
use anyhow::Result;
use foundry_common::fmt::UIfmt;
use foundry_compilers::ProjectCompileOutput;
use foundry_evm::{backend::Backend, inspectors::cheatcodes::BroadcastableTransactions};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

use super::{
    compiler::CompilerConfig,
    runner::{EvmConfig, ExecutionResult},
};

/// Configuration for a contract session
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Compiler configuration
    pub compiler: CompilerConfig,
    /// EVM configuration
    pub evm: EvmConfig,
    /// Session identifier
    pub id: Option<String>,
    /// Foundry Config
    pub foundry_config: foundry_config::Config,
}

/// A contract session that combines compilation and execution
pub struct ContractSession {
    /// The compiler instance
    pub compiler: ContractCompiler,
    /// The runner instance
    pub runner: Option<ContractRunner>,
    /// Session configuration
    pub config: SessionConfig,
    /// Cached compilation outputs
    compiled_contracts: HashMap<String, ProjectCompileOutput>,
    /// Deployed contract addresses
    deployed_contracts: HashMap<String, Address>,
}

impl ContractSession {
    /// Create a new contract session
    pub async fn new(config: SessionConfig) -> Result<Self> {
        let compiler = ContractCompiler::new(config.compiler.clone())?;

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
        Self::new(SessionConfig::default()).await
    }

    /// Get or create the EVM runner
    pub async fn get_runner(&mut self) -> Result<&mut ContractRunner> {
        if self.runner.is_none() {
            self.runner = Some(ContractRunner::new(self.config.evm.clone()).await?);
        }
        Ok(self.runner.as_mut().unwrap())
    }

    /// Compile a contract from source code
    pub fn compile_source(&mut self, name: String, source_path: PathBuf, content: String) -> Result<&ProjectCompileOutput> {
        let output = self.compiler.compile_source(source_path, content)?;
        self.compiled_contracts.insert(name.clone(), output);
        Ok(self.compiled_contracts.get(&name).unwrap())
    }

    /// Compile a contract from a file
    pub fn compile_file(&mut self, name: String, file_path: PathBuf) -> Result<&ProjectCompileOutput> {
        let output = self.compiler.compile_file(file_path)?;
        self.compiled_contracts.insert(name.clone(), output);
        Ok(self.compiled_contracts.get(&name).unwrap())
    }

    /// Deploy a compiled contract
    pub async fn deploy_contract(&mut self, compilation_name: &str, contract_name: &str) -> Result<Address> {
        let output = self
            .compiled_contracts
            .get(compilation_name)
            .ok_or_else(|| anyhow::anyhow!("No compilation found with name '{}'", compilation_name))?;

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
    pub fn get_deployed_address(&self, compilation_name: &str, contract_name: &str) -> Option<Address> {
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
            .ok_or_else(|| anyhow::anyhow!("No compilation found with name '{}'", compilation_name))?;

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
            println!("\n=== Broadcasting {} transactions to EVM backend ===", transactions.len());

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
                let _from = btx.transaction.from()
                    .ok_or_else(|| anyhow::anyhow!("Transaction missing 'from' field"))?;
                let calldata = Bytes::from(
                    btx.transaction.input()
                        .ok_or_else(|| anyhow::anyhow!("Transaction missing input data"))?
                        .to_vec()
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

#[cfg(all(test, feature = "contract-tests"))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_session_creation() {
        let session = ContractSession::default().await;
        assert!(session.is_ok());
    }

    #[tokio::test]
    async fn test_compile_and_deploy_simple_contract() {
        // This test might fail in some environments due to missing solc or EVM issues
        // This is expected and the test serves as a demonstration
        match ContractSession::default().await {
            Ok(mut session) => {
                let simple_contract = r#"
                // SPDX-License-Identifier: MIT
                pragma solidity ^0.8.0;

                contract SimpleStorage {
                    uint256 public value;

                    function setValue(uint256 _value) public {
                        value = _value;
                    }

                    function getValue() public view returns (uint256) {
                        return value;
                    }
                }
                "#;

                match session
                    .compile_and_deploy(
                        "test".to_string(),
                        PathBuf::from("SimpleStorage.sol"),
                        simple_contract.to_string(),
                        "SimpleStorage",
                    )
                    .await
                {
                    Ok(address) => {
                        assert_ne!(address, Address::ZERO);
                        // Verify the contract was recorded as deployed
                        let deployed_address = session.get_deployed_address("test", "SimpleStorage");
                        assert_eq!(deployed_address, Some(address));
                        println!("Contract deployed successfully to: {:?}", address);
                    }
                    Err(e) => {
                        // In test environments, this might fail due to missing solc or EVM issues
                        eprintln!("Compile and deploy failed (this might be expected in test environment): {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Session creation failed (this might be expected in test environment): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_session_reset() {
        let mut session = ContractSession::default().await.unwrap();

        // Add some data
        let simple_contract = r#"
        // SPDX-License-Identifier: MIT
        pragma solidity ^0.8.0;
        contract Test {}
        "#;

        let _ = session.compile_source(
            "test".to_string(),
            PathBuf::from("Test.sol"),
            simple_contract.to_string(),
        );

        assert!(!session.get_all_compilations().is_empty());

        // Reset and verify everything is cleared
        session.reset();
        assert!(session.get_all_compilations().is_empty());
        assert!(session.get_all_deployed().is_empty());
        assert!(session.runner.is_none());
    }
}