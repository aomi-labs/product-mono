//! aomi-anvil - Multi-chain fork orchestration with ProviderManager
//!
//! This crate provides a centralized manager for blockchain providers, supporting both
//! managed Anvil instances and external RPC endpoints.
//!
//! # Features
//!
//! - **Config-driven architecture**: Define chains in `providers.toml`
//! - **UUID-based instance tracking**: Each instance has a unique identifier for profiling
//! - **Lazy-loaded providers**: RootProviders are created on-demand and cached
//! - **Multi-fork support**: Create Backends with multiple chain forks
//!
//! # Example
//!
//! ```rust,ignore
//! use aomi_anvil::{ProviderManager, ForkQuery};
//!
//! // Load from config file
//! let manager = ProviderManager::from_config_file("providers.toml").await?;
//!
//! // Get provider by chain_id
//! let eth_provider = manager.get_provider(Some(1), None).await?;
//!
//! // Or by name
//! let eth_provider = manager.get_provider_by_name("ethereum").await?;
//!
//! // Get single-fork backend
//! let backend = manager.get_backend(vec![
//!     ForkQuery { chain_id: Some(1), block_number: None }
//! ]).await?;
//!
//! // Get multi-fork backend
//! let multi_backend = manager.get_backend(vec![
//!     ForkQuery { chain_id: Some(1), block_number: None },    // Ethereum
//!     ForkQuery { chain_id: Some(10), block_number: None },   // Optimism
//!     ForkQuery { chain_id: Some(42161), block_number: None } // Arbitrum
//! ]).await?;
//! ```

mod config;
mod instance;
mod lifecycle;
mod manager;

use alloy::network::AnyNetwork;
use alloy_provider::RootProvider;
use anyhow::Result;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::OnceCell;

// Re-export config types
pub use config::{AnvilInstanceConfig, ExternalConfig, ProvidersConfig};

// Re-export instance types
pub use instance::{InstanceInfo, InstanceMetricsSnapshot, InstanceSource, ManagedInstance};

// Re-export manager types
pub use manager::{ForkQuery, ProviderManager};

/// Load a ProviderManager from the default providers.toml path.
static DEFAULT_MANAGER: Lazy<OnceCell<Arc<ProviderManager>>> = Lazy::new(OnceCell::new);

pub async fn default_manager() -> Result<Arc<ProviderManager>> {
    DEFAULT_MANAGER
        .get_or_try_init(|| async { ProviderManager::from_default_config().await.map(Arc::new) })
        .await
        .map(Arc::clone)
}

/// Load the default RootProvider from providers.toml.
pub async fn default_provider() -> Result<Arc<RootProvider<AnyNetwork>>> {
    let manager = default_manager().await?;
    manager.get_provider(None, None).await
}

/// Load all configured network endpoints from providers.toml.
pub async fn default_networks() -> Result<HashMap<String, String>> {
    let manager = default_manager().await?;
    Ok(manager.get_networks())
}

/// Load the default provider endpoint from providers.toml.
pub async fn default_endpoint() -> Result<String> {
    let manager = default_manager().await?;
    manager
        .get_instance_info_by_query(None, None)
        .map(|info| info.endpoint)
        .ok_or_else(|| anyhow::anyhow!("No providers available in providers.toml"))
}
