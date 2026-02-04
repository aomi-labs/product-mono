//! ProviderManager - Multi-chain fork orchestration with UUID-based instance tracking
//!
//! This module provides a centralized manager for blockchain providers, supporting both
//! managed Anvil instances and external RPC endpoints. Key features:
//!
//! - Config-driven architecture via `providers.toml`
//! - UUID-based instance tracking with profiling metrics
//! - Lazy-loaded, cached RootProvider for RPC access
//! - Multi-fork Backend support for EVM execution

use crate::instance::{InstanceInfo, InstanceMetricsSnapshot, ManagedInstance};
use alloy::network::AnyNetwork;
use alloy_provider::RootProvider;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ============================================================================
// Query Types
// ============================================================================

/// Query for finding instances by chain_id and/or block_number
#[derive(Clone, Debug, Default)]
pub struct ForkQuery {
    pub chain_id: Option<u64>,
    pub block_number: Option<u64>,
}

impl ForkQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = Some(chain_id);
        self
    }

    pub fn with_block_number(mut self, block_number: u64) -> Self {
        self.block_number = Some(block_number);
        self
    }
}

// ============================================================================
// ProviderManager
// ============================================================================

/// Central manager for blockchain providers
///
/// Manages multiple chain forks with UUID-based tracking, lazy provider loading,
/// and usage metrics.
pub struct ProviderManager {
    pub(crate) instances: RwLock<HashMap<Uuid, Arc<ManagedInstance>>>,
    pub(crate) name_to_id: RwLock<HashMap<String, Uuid>>,
}

impl Default for ProviderManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderManager {
    /// Create an empty ProviderManager
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
            name_to_id: RwLock::new(HashMap::new()),
        }
    }

    // ========================================================================
    // Provider Access (Pattern I)
    // ========================================================================

    /// Get a RootProvider by chain_id and/or block_number
    ///
    /// Filter logic:
    /// - Both None → return first available (sorted by name for determinism)
    /// - Only chain_id → exact match on chain_id
    /// - Only block_number → exact match on block_number
    /// - Both → exact match on both
    pub async fn get_provider(
        &self,
        chain_id: Option<u64>,
        block_number: Option<u64>,
    ) -> Result<Arc<RootProvider<AnyNetwork>>> {
        let instance = self.find_instance(chain_id, block_number).ok_or_else(|| {
            anyhow::anyhow!(
                "No instance matching query: chain_id={:?}, block_number={:?}",
                chain_id,
                block_number
            )
        })?;
        instance.get_or_create_provider()
    }

    /// Get a RootProvider by instance name
    pub async fn get_provider_by_name(&self, name: &str) -> Result<Arc<RootProvider<AnyNetwork>>> {
        let instance = self
            .get_instance_by_name(name)
            .ok_or_else(|| anyhow::anyhow!("No instance found with name '{}'", name))?;
        instance.get_or_create_provider()
    }

    // ========================================================================
    // Instance Queries
    // ========================================================================

    /// Get an instance by UUID
    pub fn get_instance(&self, id: Uuid) -> Option<Arc<ManagedInstance>> {
        let instances = self.instances.read().unwrap();
        instances.get(&id).cloned()
    }

    /// Get an instance by name
    pub fn get_instance_by_name(&self, name: &str) -> Option<Arc<ManagedInstance>> {
        let name_to_id = self.name_to_id.read().unwrap();
        let id = name_to_id.get(name)?;
        let instances = self.instances.read().unwrap();
        instances.get(id).cloned()
    }

    /// Get an instance snapshot by name
    pub fn get_instance_info_by_name(&self, name: &str) -> Option<InstanceInfo> {
        self.get_instance_by_name(name)
            .map(|instance| InstanceInfo::from(&*instance))
    }

    /// List all instances (sorted by name for determinism)
    pub fn list_instances(&self) -> Vec<InstanceInfo> {
        let instances = self.instances.read().unwrap();
        let mut infos: Vec<InstanceInfo> = instances
            .values()
            .map(|i| InstanceInfo::from(&**i))
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    /// Return all configured network endpoints keyed by instance name.
    pub fn get_networks(&self) -> HashMap<String, String> {
        let instances = self.instances.read().unwrap();
        let networks: HashMap<String, String> = instances
            .values()
            .map(|instance| (instance.name().to_string(), instance.endpoint().to_string()))
            .collect();
        networks
    }

    /// Find an instance matching the query criteria
    ///
    /// When multiple instances match the query, priority is given to:
    /// 1. Ethereum mainnet (chain_id = 1)
    /// 2. Anvil default test chain (chain_id = 31337)
    /// 3. Lowest chain_id first
    pub(crate) fn find_instance(
        &self,
        chain_id: Option<u64>,
        block_number: Option<u64>,
    ) -> Option<Arc<ManagedInstance>> {
        const ETHEREUM_CHAIN_ID: u64 = 1;
        const ANVIL_DEFAULT_CHAIN_ID: u64 = 31337;

        let instances = self.instances.read().unwrap();

        let matches: Vec<&Arc<ManagedInstance>> = instances
            .values()
            .filter(|instance| {
                let chain_match = chain_id.is_none_or(|id| instance.chain_id() == id);
                let block_match = block_number.is_none_or(|bn| instance.block_number() == bn);
                chain_match && block_match
            })
            .collect();

        if matches.is_empty() {
            return None;
        }

        // If only one match, return it directly
        if matches.len() == 1 {
            return Some(Arc::clone(matches[0]));
        }

        // Multiple matches: prioritize by chain_id
        // 1. Look for Ethereum mainnet (chain_id = 1)
        if let Some(eth) = matches.iter().find(|i| i.chain_id() == ETHEREUM_CHAIN_ID) {
            return Some(Arc::clone(eth));
        }

        // 2. Look for Anvil default test chain (chain_id = 31337)
        if let Some(anvil) = matches
            .iter()
            .find(|i| i.chain_id() == ANVIL_DEFAULT_CHAIN_ID)
        {
            return Some(Arc::clone(anvil));
        }

        // 3. Sort by chain_id (lowest first) and return first
        let mut sorted = matches;
        sorted.sort_by_key(|i| i.chain_id());
        sorted.first().map(|i| Arc::clone(i))
    }

    /// Get an instance snapshot by query parameters
    pub fn get_instance_info_by_query(
        &self,
        chain_id: Option<u64>,
        block_number: Option<u64>,
    ) -> Option<InstanceInfo> {
        self.find_instance(chain_id, block_number)
            .map(|i| InstanceInfo::from(&*i))
    }

    // ========================================================================
    // Profiling
    // ========================================================================

    /// Get metrics snapshot for a specific instance
    pub fn get_metrics(&self, id: Uuid) -> Option<InstanceMetricsSnapshot> {
        let instances = self.instances.read().unwrap();
        instances.get(&id).map(|i| i.metrics_snapshot())
    }

    /// Get metrics snapshots for all instances
    pub fn get_all_metrics(&self) -> HashMap<Uuid, InstanceMetricsSnapshot> {
        let instances = self.instances.read().unwrap();
        instances
            .iter()
            .map(|(id, instance)| (*id, instance.metrics_snapshot()))
            .collect()
    }

    /// Get the number of managed instances
    pub fn instance_count(&self) -> usize {
        self.instances.read().unwrap().len()
    }
}

// ============================================================================
// Backend Support
// ============================================================================

use foundry_evm::backend::{Backend, DatabaseExt};
use foundry_evm::fork::CreateFork;
use foundry_evm::opts::EvmOpts;

impl ProviderManager {
    /// Get a Backend with support for multiple forks
    ///
    /// Each ForkQuery in the vector finds one instance to include as a fork.
    /// The first query's instance becomes the primary fork.
    pub async fn get_backend(&self, forks: Vec<ForkQuery>) -> Result<Backend> {
        if forks.is_empty() {
            anyhow::bail!("At least one fork query required");
        }

        let mut matched_instances = Vec::new();
        for query in &forks {
            let instance = self
                .find_instance(query.chain_id, query.block_number)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No instance matching query: chain_id={:?}, block_number={:?}",
                        query.chain_id,
                        query.block_number
                    )
                })?;

            instance.record_backend_access();
            matched_instances.push(instance);
        }

        let first = &matched_instances[0];
        let first_fork = create_fork_config(first).await?;

        let mut backend = tokio::task::spawn_blocking(move || {
            std::thread::spawn(move || Backend::spawn(Some(first_fork)))
                .join()
                .expect("backend thread panicked")
        })
        .await?
        .map_err(|e| anyhow::anyhow!("Backend spawn failed: {}", e))?;

        for instance in matched_instances.iter().skip(1) {
            let fork_config = create_fork_config(instance).await?;
            backend
                .create_fork(fork_config)
                .map_err(|e| anyhow::anyhow!("Failed to create fork: {}", e))?;
        }

        Ok(backend)
    }
}

/// Create a CreateFork configuration from instance metadata
async fn create_fork_config(instance: &ManagedInstance) -> Result<CreateFork> {
    let evm_opts = EvmOpts {
        fork_url: Some(instance.endpoint().to_string()),
        fork_block_number: Some(instance.block_number()),
        ..Default::default()
    };

    let env = evm_opts
        .evm_env()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create EVM environment: {}", e))?;

    Ok(CreateFork {
        enable_caching: true,
        url: instance.endpoint().to_string(),
        env,
        evm_opts,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProvidersConfig;

    #[test]
    fn test_fork_query_builder() {
        let query = ForkQuery::new().with_chain_id(1).with_block_number(12345);

        assert_eq!(query.chain_id, Some(1));
        assert_eq!(query.block_number, Some(12345));
    }

    #[test]
    fn test_provider_manager_new() {
        let manager = ProviderManager::new();
        assert_eq!(manager.instance_count(), 0);
        assert!(manager.list_instances().is_empty());
    }

    #[tokio::test]
    async fn test_provider_manager_from_empty_config() {
        let config = ProvidersConfig::default();
        let manager = ProviderManager::from_config(config).await.unwrap();
        assert_eq!(manager.instance_count(), 0);
    }
}
