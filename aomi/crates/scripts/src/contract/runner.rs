use alloy_primitives::{Address, Bytes, Log, U256, map::AddressHashMap};
use anyhow::Result;
use cast::inspectors::CheatsConfig;
use foundry_evm::{
    backend::Backend,
    executors::{DeployResult, Executor, ExecutorBuilder, RawCallResult},
    inspectors::cheatcodes::BroadcastableTransactions,
    traces::{TraceKind, Traces},
};
// Note: These types are re-exported from foundry_evm to avoid direct revm dependency
use foundry_evm::revm::{
    Database,
    interpreter::{InstructionResult, return_ok},
};

use super::session::ContractConfig;

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
    /// Reference to the contract configuration
    pub config: std::sync::Arc<super::session::ContractConfig>,
    /// The EVM executor
    pub executor: Executor,
    /// The sender address (mutable runtime state)
    pub sender: Address,
}

impl ContractRunner {
    /// Create a new contract runner
    pub async fn new(config: &ContractConfig) -> Result<Self> {
        // Clone and configure EVM options with better retry/timeout settings
        let mut evm_opts = config.evm_opts.clone();
        if evm_opts.fork_url.is_some() {
            evm_opts.fork_retries = Some(5);
            evm_opts.fork_retry_backoff = Some(1000); // milliseconds
        }
        let env = evm_opts
            .evm_env()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create EVM environment: {}", e))?;
        let fork = evm_opts.get_fork(&config.foundry_config, env.clone());
        tracing::info!("Attempting to spawn backend with fork: {:?}", fork);
        let backend = tokio::task::spawn_blocking(move || {
            std::thread::spawn(move || Backend::spawn(fork))
                .join()
                .expect("backend thread panicked")
        })
        .await?
        .map_err(|e| anyhow::anyhow!("Backend spawn failed: {}", e))?;
        tracing::info!("Backend spawned successfully");

        let executor = ExecutorBuilder::new()
            .inspectors(|stack| {
                stack
                    .trace_mode(foundry_evm::traces::TraceMode::Call) // Enable tracing like forge script
                    .networks(evm_opts.networks)
                    .create2_deployer(evm_opts.create2_deployer)
                    .cheatcodes(
                        CheatsConfig::new(&config.foundry_config, evm_opts.clone(), None, None)
                            .into(),
                    )
            })
            .gas_limit(evm_opts.gas_limit().max(30_000_000)) // Use evm_opts gas limit or default to 30M
            .spec_id(config.foundry_config.evm_spec_id())
            .legacy_assertions(config.foundry_config.legacy_assertions)
            .build(env, backend);

        let sender = Address::ZERO; // Default sender
        let config_arc = std::sync::Arc::new(config.clone());

        Ok(Self {
            config: config_arc,
            executor,
            sender,
        })
    }

    /// Create a new contract runner with a specific backend
    pub async fn with_backend(config: &ContractConfig, backend: Backend) -> Result<Self> {
        let env = config
            .evm_opts
            .evm_env()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create EVM environment: {}", e))?;

        let executor = ExecutorBuilder::new()
            .inspectors(|stack| {
                stack
                    .trace_mode(foundry_evm::traces::TraceMode::Call) // Enable tracing like forge script
                    .networks(config.evm_opts.networks)
                    .create2_deployer(config.evm_opts.create2_deployer)
                    .cheatcodes(
                        CheatsConfig::new(
                            &config.foundry_config,
                            config.evm_opts.clone(),
                            None,
                            None,
                        )
                        .into(),
                    )
            })
            .gas_limit(config.evm_opts.gas_limit().max(30_000_000)) // Use evm_opts gas limit or default to 30M
            .spec_id(config.foundry_config.evm_spec_id())
            .legacy_assertions(config.foundry_config.legacy_assertions)
            .build(env, backend);

        let sender = Address::ZERO;
        let config_arc = std::sync::Arc::new(config.clone());

        Ok(Self {
            config: config_arc,
            executor,
            sender,
        })
    }

    /// Set the sender address
    pub fn set_sender(&mut self, sender: Address) {
        self.sender = sender;
    }

    /// Deploy a contract and return its address
    pub fn deploy(&mut self, bytecode: Bytes) -> Result<(Address, ExecutionResult)> {
        let initial_balance = self.config.initial_balance.unwrap_or(U256::MAX);

        // Set the sender's balance for deployment
        self.executor.set_balance(self.sender, U256::MAX)?;

        // Deploy the contract
        let DeployResult { address, .. } = self
            .executor
            .deploy(self.sender, bytecode, U256::ZERO, None)
            .map_err(|err| anyhow::anyhow!("Failed to deploy contract: {:?}", err))?;

        // Reset the sender's balance
        self.executor.set_balance(self.sender, initial_balance)?;

        let result = ExecutionResult {
            success: true,
            address: Some(address),
            ..Default::default()
        };

        Ok((address, result))
    }

    /// Call a contract function
    pub fn call(&mut self, to: Address, calldata: Bytes, value: U256) -> Result<ExecutionResult> {
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
        let mut res = self
            .executor
            .call_raw(self.sender, to, calldata.clone(), value)
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
                let test_res = self
                    .executor
                    .call_raw(self.sender, to, calldata.clone(), value)
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
            res = self
                .executor
                .transact_raw(self.sender, to, calldata, value)
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
            traces: traces
                .map(|traces| vec![(TraceKind::Execution, traces)])
                .unwrap_or_default(),
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
        match self
            .executor
            .backend_mut()
            .basic(address)
            .map_err(|e| anyhow::anyhow!("Failed to get account info: {}", e))?
        {
            Some(info) => Ok(info.balance),
            None => Ok(U256::ZERO),
        }
    }

    /// Set the balance of an account
    pub fn set_balance(&mut self, address: Address, balance: U256) -> Result<()> {
        self.executor
            .set_balance(address, balance)
            .map_err(|e| anyhow::anyhow!("Failed to set balance: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256, hex};

    fn constant_return_contract() -> Bytes {
        let creation_and_runtime =
            hex::decode("600a600c600039600a6000f3602a60005260206000f3").unwrap();
        Bytes::from(creation_and_runtime)
    }

    fn decode_u256(bytes: &Bytes) -> U256 {
        let mut buf = [0u8; 32];
        let slice = bytes.as_ref();
        let len = slice.len().min(32);
        buf[32 - len..].copy_from_slice(&slice[slice.len() - len..]);
        U256::from_be_bytes(buf)
    }

    async fn build_runner() -> ContractRunner {
        let config = ContractConfig::default();
        ContractRunner::new(&config)
            .await
            .expect("runner should initialize")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deploy_and_call_returns_expected_value() {
        // Skip test if ETH_RPC_URL env var is not set
        if std::env::var("ETH_RPC_URL").is_err() {
            eprintln!("Skipping deploy_and_call_returns_expected_value: ETH_RPC_URL not set");
            return;
        }

        let mut runner = build_runner().await;
        let (address, deploy_result) = runner
            .deploy(constant_return_contract())
            .expect("deployment succeeds");

        assert!(deploy_result.success);
        assert_eq!(deploy_result.address, Some(address));

        let call_result = runner
            .call(address, Bytes::new(), U256::ZERO)
            .expect("call should succeed");

        assert!(call_result.success);
        assert_eq!(call_result.address, Some(address));
        assert_eq!(decode_u256(&call_result.returned), U256::from(42u64));
        assert!(call_result.broadcastable_transactions.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn call_static_preserves_state() {
        // Skip test if ETH_RPC_URL env var is not set
        if std::env::var("ETH_RPC_URL").is_err() {
            eprintln!("Skipping call_static_preserves_state: ETH_RPC_URL not set");
            return;
        }

        let mut runner = build_runner().await;
        let (address, _) = runner
            .deploy(constant_return_contract())
            .expect("deployment succeeds");

        let static_result = runner
            .call_static(address, Bytes::new(), U256::ZERO)
            .expect("static call succeeds");

        assert!(static_result.success);
        assert_eq!(decode_u256(&static_result.returned), U256::from(42u64));
        assert_eq!(static_result.address, Some(address));
        assert!(static_result.broadcastable_transactions.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn set_and_get_balance_round_trip() {
        // Skip test if ETH_RPC_URL env var is not set
        if std::env::var("ETH_RPC_URL").is_err() {
            eprintln!("Skipping set_and_get_balance_round_trip: ETH_RPC_URL not set");
            return;
        }

        let mut runner = build_runner().await;
        let target = Address::from([0x11u8; 20]);
        let value = U256::from(1337u64);

        runner
            .set_balance(target, value)
            .expect("setting balance succeeds");
        let fetched = runner.get_balance(target).expect("balance fetch succeeds");
        assert_eq!(fetched, value);
    }
}
