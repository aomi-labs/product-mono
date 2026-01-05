//! ProviderManager - Multi-chain fork orchestration with UUID-based instance tracking
//!
//! This module provides a centralized manager for blockchain providers, supporting both
//! managed Anvil instances and external RPC endpoints. Key features:
//!
//! - Config-driven architecture via `providers.toml`
//! - UUID-based instance tracking with profiling metrics
//! - Lazy-loaded, cached RootProvider for RPC access
//! - Multi-fork Backend support for EVM execution

use crate::config::{AnvilInstanceConfig, ExternalConfig, ProvidersConfig};
use crate::instance::{fetch_block_number, AnvilInstance};
use alloy::network::AnyNetwork;
use alloy_provider::RootProvider;
use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;

// ============================================================================
// Instance Types
// ============================================================================

/// Source of a managed instance - either a spawned Anvil process or an external RPC
#[derive(Debug)]
pub enum InstanceSource {
    /// Managed Anvil process
    Anvil(Mutex<AnvilInstance>),
    /// External RPC endpoint
    External,
}

/// Metrics for tracking instance usage
pub struct InstanceMetrics {
    provider_requests: AtomicU64,
    backend_requests: AtomicU64,
    last_provider_access: RwLock<Option<Instant>>,
    last_backend_access: RwLock<Option<Instant>>,
}

impl Default for InstanceMetrics {
    fn default() -> Self {
        Self {
            provider_requests: AtomicU64::new(0),
            backend_requests: AtomicU64::new(0),
            last_provider_access: RwLock::new(None),
            last_backend_access: RwLock::new(None),
        }
    }
}

impl InstanceMetrics {
    fn record_provider_access(&self) {
        self.provider_requests.fetch_add(1, Ordering::Relaxed);
        *self.last_provider_access.write().unwrap() = Some(Instant::now());
    }

    fn record_backend_access(&self) {
        self.backend_requests.fetch_add(1, Ordering::Relaxed);
        *self.last_backend_access.write().unwrap() = Some(Instant::now());
    }

    /// Create a snapshot of the current metrics
    pub fn snapshot(&self) -> InstanceMetricsSnapshot {
        InstanceMetricsSnapshot {
            provider_requests: self.provider_requests.load(Ordering::Relaxed),
            backend_requests: self.backend_requests.load(Ordering::Relaxed),
            last_provider_access: *self.last_provider_access.read().unwrap(),
            last_backend_access: *self.last_backend_access.read().unwrap(),
        }
    }
}

/// Clonable snapshot of instance metrics
#[derive(Clone, Debug)]
pub struct InstanceMetricsSnapshot {
    pub provider_requests: u64,
    pub backend_requests: u64,
    pub last_provider_access: Option<Instant>,
    pub last_backend_access: Option<Instant>,
}

/// A managed blockchain instance (Anvil or External)
pub struct ManagedInstance {
    /// Unique identifier
    id: Uuid,
    /// Human-readable name (from config key)
    name: String,
    /// Chain ID
    chain_id: u64,
    /// Block number at initialization
    block_number: u64,
    /// RPC endpoint URL
    endpoint: String,
    /// Source (Anvil or External)
    source: InstanceSource,
    /// Cached RootProvider (lazy-loaded)
    provider: OnceCell<Arc<RootProvider<AnyNetwork>>>,
    /// Creation timestamp
    created_at: Instant,
    /// Usage metrics
    metrics: InstanceMetrics,
}

impl ManagedInstance {
    /// Create a new managed instance from an Anvil process
    fn from_anvil(name: String, instance: AnvilInstance) -> Self {
        let endpoint = instance.endpoint().to_string();
        Self {
            id: Uuid::new_v4(),
            name,
            chain_id: instance.chain_id(),
            block_number: instance.block_number(),
            endpoint,
            source: InstanceSource::Anvil(Mutex::new(instance)),
            provider: OnceCell::new(),
            created_at: Instant::now(),
            metrics: InstanceMetrics::default(),
        }
    }

    /// Create a new managed instance from an external RPC endpoint
    fn from_external(name: String, chain_id: u64, block_number: u64, rpc_url: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            chain_id,
            block_number,
            endpoint: rpc_url,
            source: InstanceSource::External,
            provider: OnceCell::new(),
            created_at: Instant::now(),
            metrics: InstanceMetrics::default(),
        }
    }

    /// Get the instance UUID
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get the instance name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the chain ID
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Get the block number
    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    /// Get the RPC endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Check if this is a managed (Anvil) instance
    pub fn is_managed(&self) -> bool {
        matches!(self.source, InstanceSource::Anvil(_))
    }

    /// Get the creation timestamp
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    /// Get a snapshot of the metrics
    pub fn metrics_snapshot(&self) -> InstanceMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Get or create a cached RootProvider
    fn get_or_create_provider(&self) -> Result<Arc<RootProvider<AnyNetwork>>> {
        self.metrics.record_provider_access();

        let provider = self.provider.get_or_try_init(|| {
            let url = self.endpoint().parse().context("Invalid RPC URL")?;
            let provider = RootProvider::<AnyNetwork>::new_http(url);
            Ok::<_, anyhow::Error>(Arc::new(provider))
        })?;

        Ok(Arc::clone(provider))
    }

    /// Shutdown the instance (kills Anvil process if managed)
    async fn shutdown(&self) -> Result<()> {
        if let InstanceSource::Anvil(instance) = &self.source {
            let mut guard = instance.lock().await;
            guard.kill().await?;
        }
        Ok(())
    }
}

/// Read-only snapshot of instance information
#[derive(Clone, Debug)]
pub struct InstanceInfo {
    pub id: Uuid,
    pub name: String,
    pub chain_id: u64,
    pub block_number: u64,
    pub is_managed: bool,
    pub endpoint: String,
    pub created_at: Instant,
}

impl From<&ManagedInstance> for InstanceInfo {
    fn from(instance: &ManagedInstance) -> Self {
        Self {
            id: instance.id,
            name: instance.name.clone(),
            chain_id: instance.chain_id,
            block_number: instance.block_number,
            is_managed: instance.is_managed(),
            endpoint: instance.endpoint().to_string(),
            created_at: instance.created_at,
        }
    }
}

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
    instances: RwLock<HashMap<Uuid, Arc<ManagedInstance>>>,
    name_to_id: RwLock<HashMap<String, Uuid>>,
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

    /// Create a ProviderManager from a ProvidersConfig
    ///
    /// This spawns all configured Anvil instances and registers external endpoints.
    pub async fn from_config(config: ProvidersConfig) -> Result<Self> {
        let manager = Self::new();

        // Spawn Anvil instances
        for (name, instance_config) in config.anvil_instances {
            manager
                .spawn_anvil(name.clone(), instance_config)
                .await
                .with_context(|| format!("Failed to spawn Anvil instance '{}'", name))?;
        }

        // Register external endpoints
        for (name, external_config) in config.external {
            manager
                .register_external(name.clone(), external_config)
                .await
                .with_context(|| format!("Failed to register external endpoint '{}'", name))?;
        }

        Ok(manager)
    }

    /// Create a ProviderManager from a config file
    pub async fn from_config_file(path: impl AsRef<Path>) -> Result<Self> {
        let config = ProvidersConfig::from_file(path)?;
        config.validate()?;
        Self::from_config(config).await
    }

    /// Create a ProviderManager from the default providers.toml path
    pub async fn from_default_config() -> Result<Self> {
        Self::from_config_file("providers.toml").await
    }

    // ========================================================================
    // Lifecycle Management
    // ========================================================================

    /// Spawn a new Anvil instance
    pub async fn spawn_anvil(&self, name: String, config: AnvilInstanceConfig) -> Result<Uuid> {
        let params = config.to_anvil_params();
        let anvil_instance = AnvilInstance::spawn(params).await?;

        let instance = Arc::new(ManagedInstance::from_anvil(name.clone(), anvil_instance));
        let id = instance.id;

        let instance_name = name.clone();
        {
            let mut name_to_id = self.name_to_id.write().unwrap();
            if name_to_id.contains_key(&name) {
                anyhow::bail!("Instance name '{}' already exists", name);
            }
            let mut instances = self.instances.write().unwrap();
            instances.insert(id, instance);
            name_to_id.insert(name, id);
        }

        tracing::info!(id = %id, name = %instance_name, "Spawned Anvil instance");
        Ok(id)
    }

    /// Register an external RPC endpoint
    pub async fn register_external(&self, name: String, config: ExternalConfig) -> Result<Uuid> {
        let block_number = fetch_block_number(&config.rpc_url)
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch block number from external endpoint '{}'",
                    name
                )
            })?;

        let instance = Arc::new(ManagedInstance::from_external(
            name.clone(),
            config.chain_id,
            block_number,
            config.rpc_url,
        ));
        let id = instance.id;

        {
            let mut name_to_id = self.name_to_id.write().unwrap();
            if name_to_id.contains_key(&name) {
                anyhow::bail!("Instance name '{}' already exists", name);
            }
            let mut instances = self.instances.write().unwrap();
            instances.insert(id, instance);
            name_to_id.insert(name.clone(), id);
        }

        tracing::info!(id = %id, name = %name, chain_id = config.chain_id, "Registered external endpoint");
        Ok(id)
    }

    /// Shutdown a specific instance by ID
    pub async fn shutdown_instance(&self, id: Uuid) -> Result<()> {
        let instance = {
            let mut name_to_id = self.name_to_id.write().unwrap();
            name_to_id.retain(|_, v| *v != id);
            let mut instances = self.instances.write().unwrap();
            instances.remove(&id)
        };

        if let Some(instance) = instance {
            instance.shutdown().await?;

            tracing::info!(id = %id, "Shutdown instance");
        }

        Ok(())
    }

    /// Shutdown all instances
    pub async fn shutdown_all(&self) -> Result<()> {
        let instances: Vec<Arc<ManagedInstance>> = {
            let mut name_to_id = self.name_to_id.write().unwrap();
            name_to_id.clear();
            let mut guard = self.instances.write().unwrap();
            guard.drain().map(|(_, v)| v).collect()
        };

        for instance in instances {
            let _ = instance.shutdown().await;
        }

        tracing::info!("Shutdown all instances");
        Ok(())
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
            .map(|instance| (instance.name.clone(), instance.endpoint.clone()))
            .collect();
        networks
    }

    /// Find an instance matching the query criteria
    fn find_instance(
        &self,
        chain_id: Option<u64>,
        block_number: Option<u64>,
    ) -> Option<Arc<ManagedInstance>> {
        let instances = self.instances.read().unwrap();

        // Collect matching instances
        let mut matches: Vec<&Arc<ManagedInstance>> = instances
            .values()
            .filter(|instance| {
                let chain_match = chain_id.map_or(true, |id| instance.chain_id == id);
                let block_match = block_number.map_or(true, |bn| instance.block_number == bn);
                chain_match && block_match
            })
            .collect();

        // Sort by name for determinism
        matches.sort_by(|a, b| a.name.cmp(&b.name));

        matches.first().map(|i| Arc::clone(i))
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

        // Find all matching instances
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

            // Record backend access
            instance.metrics.record_backend_access();
            matched_instances.push(instance);
        }

        // Create Backend with first fork
        let first = &matched_instances[0];
        let first_fork = create_fork_config(first).await?;

        let mut backend = tokio::task::spawn_blocking(move || {
            std::thread::spawn(move || Backend::spawn(Some(first_fork)))
                .join()
                .expect("backend thread panicked")
        })
        .await?
        .map_err(|e| anyhow::anyhow!("Backend spawn failed: {}", e))?;

        // Add remaining forks
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
    let mut evm_opts = EvmOpts::default();
    evm_opts.fork_url = Some(instance.endpoint().to_string());
    evm_opts.fork_block_number = Some(instance.block_number);

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

    #[test]
    fn test_fork_query_builder() {
        let query = ForkQuery::new()
            .with_chain_id(1)
            .with_block_number(12345);

        assert_eq!(query.chain_id, Some(1));
        assert_eq!(query.block_number, Some(12345));
    }

    #[test]
    fn test_instance_metrics_snapshot() {
        let metrics = InstanceMetrics::default();

        metrics.record_provider_access();
        metrics.record_provider_access();
        metrics.record_backend_access();

        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.provider_requests, 2);
        assert_eq!(snapshot.backend_requests, 1);
        assert!(snapshot.last_provider_access.is_some());
        assert!(snapshot.last_backend_access.is_some());
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
