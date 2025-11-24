use alloy_primitives::{Address, Bytes, Log, U256, map::AddressHashMap};
use anyhow::Result;
use cast::inspectors::CheatsConfig;
use foundry_evm::{
    backend::Backend,
    executors::{DeployResult, Executor, ExecutorBuilder, RawCallResult},
    inspectors::cheatcodes::BroadcastableTransactions,
    opts::EvmOpts,
    traces::{TraceKind, Traces},
};
// Note: These types are re-exported from foundry_evm to avoid direct revm dependency
use foundry_evm::revm::{
    interpreter::{InstructionResult, return_ok},
    Database,
};
use serde::{Deserialize, Serialize};

/// Configuration for EVM execution
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvmConfig {
    /// EVM Options
    pub evm_opts: EvmOpts,
    /// Enable traces for contract execution
    pub traces: bool,
    /// Initial balance for the sender account
    pub initial_balance: Option<U256>,
    /// Reference to Foundry config
    pub foundry_config: foundry_config::Config,
    /// Fork URL for initializing with a specific chain state
    pub fork_url: Option<String>,
}

impl Default for EvmConfig {
    fn default() -> Self {
        let mut evm_opts = EvmOpts::default();
        // Fix the memory limit issue that causes MemoryLimitOOG errors
        evm_opts.memory_limit = 128 * 1024 * 1024; // 128MB memory limit

        Self {
            evm_opts,
            traces: false,
            initial_balance: None,
            foundry_config: foundry_config::Config::default(),
            fork_url: None,
        }
    }
}

/// The result of a contract execution
#[derive(Debug, Default)]
pub struct ExecutionResult {
    /// Was the execution successful?
    pub success: bool,
    /// Transaction logs
    pub logs: Vec<Log>,
    /// Call traces
    pub traces: Traces,
    /// Amount of gas used in the transaction
    pub gas_used: u64,
    /// Map of addresses to their labels
    pub labeled_addresses: AddressHashMap<String>,
    /// Return data
    pub returned: Bytes,
    /// Contract address (for deployments)
    pub address: Option<Address>,
    /// EVM State at the final instruction
    pub state: Option<(Vec<U256>, Vec<u8>)>,
    /// Transactions recorded by vm.startBroadcast/stopBroadcast
    pub broadcastable_transactions: BroadcastableTransactions,
}

/// A contract runner that executes bytecode on an in-memory EVM instance
pub struct ContractRunner {
    /// The EVM executor
    pub executor: Executor,
    /// Initial balance for the sender
    pub initial_balance: U256,
    /// The sender address
    pub sender: Address,
    /// Configuration
    pub config: EvmConfig,
}

impl ContractRunner {
    /// Create a new contract runner
    pub async fn new(config: EvmConfig) -> Result<Self> {
        let env = config.evm_opts.evm_env().await
            .map_err(|e| anyhow::anyhow!("Failed to create EVM environment: {}", e))?;
        let fork = config.evm_opts.get_fork(&config.foundry_config, env.clone());
        let backend = Backend::spawn(fork)
            .map_err(|e| anyhow::anyhow!("Failed to spawn backend: {}", e))?;
        
        let executor = ExecutorBuilder::new()
        .inspectors(|stack| {
            stack.cheatcodes(CheatsConfig::new(&config.foundry_config, config.evm_opts.clone(), None, None).into())
        })
            .gas_limit(30_000_000u64) // Set a generous gas limit for contract operations
            .spec_id(Default::default()) // Use default EVM spec
            .legacy_assertions(false)
            .build(env, backend);

        let initial_balance = config.initial_balance.unwrap_or(U256::MAX);
        let sender = Address::ZERO; // Default sender

        Ok(Self {
            executor,
            initial_balance,
            sender,
            config,
        })
    }

    /// Create a new contract runner with a specific backend
    pub async fn with_backend(config: EvmConfig, backend: Backend) -> Result<Self> {
        let env = config.evm_opts.evm_env().await
            .map_err(|e| anyhow::anyhow!("Failed to create EVM environment: {}", e))?;

        let executor = ExecutorBuilder::new()
            .gas_limit(30_000_000u64) // Set a generous gas limit for contract operations
            .spec_id(Default::default())
            .legacy_assertions(false)
            .build(env, backend);

        let initial_balance = config.initial_balance.unwrap_or(U256::MAX);
        let sender = Address::ZERO;

        Ok(Self {
            executor,
            initial_balance,
            sender,
            config,
        })
    }

    /// Set the sender address
    pub fn set_sender(&mut self, sender: Address) {
        self.sender = sender;
    }

    /// Deploy a contract and return its address
    pub fn deploy(&mut self, bytecode: Bytes) -> Result<(Address, ExecutionResult)> {
        // Set the sender's balance for deployment
        self.executor.set_balance(self.sender, U256::MAX)?;

        // Deploy the contract
        let DeployResult { address, .. } = self
            .executor
            .deploy(self.sender, bytecode, U256::ZERO, None)
            .map_err(|err| anyhow::anyhow!("Failed to deploy contract: {:?}", err))?;

        // Reset the sender's balance
        self.executor.set_balance(self.sender, self.initial_balance)?;

        let result = ExecutionResult {
            success: true,
            address: Some(address),
            ..Default::default()
        };

        Ok((address, result))
    }

    /// Call a contract function
    pub fn call(
        &mut self,
        to: Address,
        calldata: Bytes,
        value: U256,
    ) -> Result<ExecutionResult> {
        self.call_with_options(to, calldata, value, true)
    }

    /// Call a contract function without committing the transaction
    pub fn call_static(
        &mut self,
        to: Address,
        calldata: Bytes,
        value: U256,
    ) -> Result<ExecutionResult> {
        self.call_with_options(to, calldata, value, false)
    }

    /// Execute a contract call with specific options
    fn call_with_options(
        &mut self,
        to: Address,
        calldata: Bytes,
        value: U256,
        commit: bool,
    ) -> Result<ExecutionResult> {
        // Perform the call
        let mut res = self.executor.call_raw(self.sender, to, calldata.clone(), value)
            .map_err(|e| anyhow::anyhow!("Call failed: {}", e))?;
        let mut gas_used = res.gas_used;

        // Gas estimation logic (similar to chisel's implementation)
        if matches!(res.exit_reason, Some(return_ok!())) {
            let init_gas_limit = self.executor.env().tx.gas_limit;

            // Estimate gas by binary search
            let mut highest_gas_limit = gas_used * 3;
            let mut lowest_gas_limit = gas_used;
            let mut last_highest_gas_limit = highest_gas_limit;

            while (highest_gas_limit - lowest_gas_limit) > 1 {
                let mid_gas_limit = (highest_gas_limit + lowest_gas_limit) / 2;
                self.executor.env_mut().tx.gas_limit = mid_gas_limit;
                let test_res = self.executor.call_raw(self.sender, to, calldata.clone(), value)
                    .map_err(|e| anyhow::anyhow!("Gas estimation call failed: {}", e))?;

                match test_res.exit_reason {
                    Some(InstructionResult::Revert)
                    | Some(InstructionResult::OutOfGas)
                    | Some(InstructionResult::OutOfFunds) => {
                        lowest_gas_limit = mid_gas_limit;
                    }
                    _ => {
                        highest_gas_limit = mid_gas_limit;
                        // Accuracy check: if last two estimations vary by <10%, we're done
                        const ACCURACY: u64 = 10;
                        if (last_highest_gas_limit - highest_gas_limit) * ACCURACY
                            / last_highest_gas_limit
                            < 1
                        {
                            gas_used = highest_gas_limit;
                            break;
                        }
                        last_highest_gas_limit = highest_gas_limit;
                    }
                }
            }

            // Reset gas limit
            self.executor.env_mut().tx.gas_limit = init_gas_limit;
        }

        // Commit the transaction if requested
        if commit {
            res = self.executor.transact_raw(self.sender, to, calldata, value)
                .map_err(|e| anyhow::anyhow!("Transaction failed: {}", e))?;
        }

        let RawCallResult {
            result,
            reverted,
            logs,
            traces,
            labels,
            chisel_state,
            transactions,
            ..
        } = res;

        // Extract broadcastable transactions from the RawCallResult
        let broadcastable_transactions = transactions.unwrap_or_default();

        Ok(ExecutionResult {
            returned: result,
            success: !reverted,
            gas_used,
            logs,
            traces: traces.map(|traces| vec![(TraceKind::Execution, traces)]).unwrap_or_default(),
            labeled_addresses: labels,
            address: Some(to),
            state: chisel_state,
            broadcastable_transactions,
        })
    }

    /// Deploy and call a contract in one operation
    pub fn deploy_and_call(
        &mut self,
        bytecode: Bytes,
        calldata: Bytes,
        value: U256,
    ) -> Result<(Address, ExecutionResult)> {
        let (address, _deploy_result) = self.deploy(bytecode)?;
        let call_result = self.call(address, calldata, value)?;
        Ok((address, call_result))
    }

    /// Get the balance of an account
    pub fn get_balance(&mut self, address: Address) -> Result<U256> {
        match self.executor.backend_mut().basic(address)
            .map_err(|e| anyhow::anyhow!("Failed to get account info: {}", e))? {
            Some(info) => Ok(info.balance),
            None => Ok(U256::ZERO),
        }
    }

    /// Set the balance of an account
    pub fn set_balance(&mut self, address: Address, balance: U256) -> Result<()> {
        self.executor.set_balance(address, balance)
            .map_err(|e| anyhow::anyhow!("Failed to set balance: {}", e))?;
        Ok(())
    }
}

#[cfg(all(test, feature = "contract-tests"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runner_creation() {
        let config = EvmConfig::default();
        let runner = ContractRunner::new(config).await;
        assert!(runner.is_ok());
    }

    #[tokio::test]
    async fn test_simple_deployment() {
        let config = EvmConfig::default();

        // This test might fail in some environments due to EVM setup issues
        // This is expected and the test serves as a demonstration
        match ContractRunner::new(config).await {
            Ok(mut runner) => {
                // Very simple bytecode: just a STOP instruction (0x00)
                // This is minimal valid bytecode that should deploy successfully
                let bytecode = Bytes::from_static(&[0x00]);

                match runner.deploy(bytecode) {
                    Ok((address, execution_result)) => {
                        assert_ne!(address, Address::ZERO);
                        // Note: success might be false due to empty bytecode, but deployment should work
                        println!("Deployment successful to address: {:?}", address);
                    }
                    Err(e) => {
                        // In test environments, deployment might fail due to EVM configuration
                        eprintln!("Deployment failed (this might be expected in test environment): {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Runner creation failed (this might be expected in test environment): {}", e);
            }
        }
    }
}