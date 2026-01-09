use alloy_primitives::{Address, Bytes, U256};
use cast::backend::DatabaseExt;
use dashmap::DashMap;
use eyre::{eyre, Result};
use foundry_evm::{
    backend::Backend,
    executors::{Executor, ExecutorBuilder},
    fork::CreateFork,
    opts::EvmOpts,
};
use std::{collections::{HashMap, HashSet, VecDeque}, sync::Arc};
use tokio::sync::Mutex;

use super::config::ChainId;
use crate::contract::runner::ExecutionResult;

/// Per-plan execution backend managing multi-chain fork state
/// One backend instance per execution plan
pub struct ExecutionBackend {
    /// Shared backend (manages all forks for this plan)
    backend: Arc<Mutex<Backend>>,

    /// Per-chain IMMUTABLE configs (read-only, nodes copy from here)
    evm_opts: Arc<HashMap<ChainId, EvmOpts>>,

    /// Per-chain executors (one per chain)
    executors: Arc<DashMap<ChainId, Mutex<Executor>>>,

    /// Fork ID mapping
    fork_ids: Arc<DashMap<ChainId, foundry_evm::backend::LocalForkId>>,

    /// Base foundry config
    base_config: Arc<foundry_config::Config>,
}

impl ExecutionBackend {
    /// Create new execution backend and initialize forks for all target chains
    pub async fn new(
        target_chains: &HashSet<ChainId>,
        base_foundry_config: &foundry_config::Config,
    ) -> Result<Self> {
        let fork_ids = DashMap::new();
        let executors = DashMap::new();
        let mut evm_opts_map = HashMap::new();

        let provider_manager = super::ProviderManager::new();
        target_chains.clone().iter().for_each(|chain_id| {
            let evm_opts = provider_manager.get_evm_opts(*chain_id);
            evm_opts_map.insert(*chain_id, evm_opts);
        });

        let backend = provider_manager.get_backend(&target_chains.iter().map(|id| *id).collect::<Vec<_>>());

        Ok(Self {
            backend: Arc::new(Mutex::new(backend)),
            evm_opts: Arc::new(evm_opts_map),
            executors: Arc::new(executors),
            fork_ids: Arc::new(fork_ids),
            base_config: Arc::new(base_foundry_config.clone()),
        })
    }

    /// Get immutable EvmOpts for a chain (nodes copy from this)
    pub fn get_evm_opts(&self, chain_id: ChainId) -> Result<EvmOpts> {
        self.evm_opts
            .get(&chain_id)
            .cloned()
            .ok_or_else(|| eyre!("No EvmOpts for chain {}", chain_id))
    }

    /// Get or create executor for a chain
    async fn get_or_create_executor(&self, chain_id: ChainId) -> Result<()> {
        // Check if executor already exists
        if self.executors.contains_key(&chain_id) {
            return Ok(());
        }

        // Create new executor for this chain
        let evm_opts = self.get_evm_opts(chain_id)?;
        let env = evm_opts.evm_env().await?;

        // Create fork
        let fork = CreateFork {
            enable_caching: true,
            url: evm_opts.fork_url.clone().unwrap_or_default(),
            env: env.clone(),
            evm_opts: evm_opts.clone(),
        };

        let mut backend_guard = self.backend.lock().await;
        let backend: &mut Backend = &mut *backend_guard;
        let fork_id = backend.create_fork(fork)?;
        self.fork_ids.insert(chain_id, fork_id);

        // Build executor
        let executor = ExecutorBuilder::new()
            .gas_limit(evm_opts.gas_limit().max(30_000_000))
            .spec_id(self.base_config.evm_spec_id())
            .legacy_assertions(self.base_config.legacy_assertions)
            .build(env, (*backend).clone());

        self.executors.insert(chain_id, Mutex::new(executor));

        Ok(())
    }

    /// Deploy contract on specific chain
    pub async fn deploy(
        &self,
        chain_id: ChainId,
        sender: Address,
        bytecode: Bytes,
        value: U256,
    ) -> Result<(Address, ExecutionResult)> {
        // Ensure executor exists
        self.get_or_create_executor(chain_id).await?;

        // Get executor
        let executor_ref = self.executors.get(&chain_id)
            .ok_or_else(|| eyre!("Executor not found for chain {}", chain_id))?;
        let mut exec = executor_ref.lock().await;

        // Set sender balance
        exec.set_balance(sender, U256::MAX)?;

        // Deploy
        let deploy_result = exec.deploy(sender, bytecode, value, None)?;

        // Build execution result
        let result = ExecutionResult {
            success: true,
            logs: vec![],
            traces: vec![],
            gas_used: deploy_result.gas_used,
            labeled_addresses: Default::default(),
            returned: Bytes::new(),
            address: Some(deploy_result.address),
            state: None,
            broadcastable_transactions: VecDeque::new(),
        };

        Ok((deploy_result.address, result))
    }

    /// Call contract on specific chain
    pub async fn call(
        &self,
        chain_id: ChainId,
        sender: Address,
        to: Address,
        calldata: Bytes,
        value: U256,
    ) -> Result<ExecutionResult> {
        // Ensure executor exists
        self.get_or_create_executor(chain_id).await?;

        // Get executor
        let executor_ref = self.executors.get(&chain_id)
            .ok_or_else(|| eyre!("Executor not found for chain {}", chain_id))?;
        let exec = executor_ref.lock().await;

        // Execute call
        let raw_result = exec.call_raw(sender, to, calldata, value)?;

        // Build execution result
        let result = ExecutionResult {
            success: !raw_result.reverted,
            logs: raw_result.logs,
            traces: raw_result.traces.map(|t| vec![(foundry_evm::traces::TraceKind::Execution, t)]).unwrap_or_default(),
            gas_used: raw_result.gas_used,
            labeled_addresses: raw_result.labels,
            returned: raw_result.result,
            address: Some(to),
            state: raw_result.chisel_state,
            broadcastable_transactions: raw_result.transactions.unwrap_or_default(),
        };

        Ok(result)
    }

    /// Execute a closure against a chain-specific executor with tx context set.
    pub async fn execute_on_chain<F, T>(
        &self,
        chain_id: ChainId,
        sender: Address,
        calldata: Bytes,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce(&mut Executor) -> Result<T>,
    {
        self.get_or_create_executor(chain_id).await?;

        let executor_ref = self
            .executors
            .get(&chain_id)
            .ok_or_else(|| eyre!("Executor not found for chain {}", chain_id))?;
        let mut exec = executor_ref.lock().await;

        let prev_caller = exec.env().tx.caller;
        let prev_data = exec.env().tx.data.clone();

        exec.env_mut().tx.caller = sender;
        exec.env_mut().tx.data = calldata;

        let result = f(&mut exec);

        // Restore previous tx context to avoid leaking across calls.
        exec.env_mut().tx.caller = prev_caller;
        exec.env_mut().tx.data = prev_data;

        result
    }
}
