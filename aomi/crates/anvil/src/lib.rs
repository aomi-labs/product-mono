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
//! - **Multi-fork support**: Create Backends with multiple chain forks (requires `backend` feature)
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
//! let eth_provider = manager.get_provider(Some(1), None).await
//!     .ok_or_else(|| anyhow!("Ethereum provider not found"))?;
//!
//! // Or by name
//! let eth_provider = manager.get_provider_by_name("ethereum").await.unwrap();
//!
//! // Get single-fork backend (requires `backend` feature)
//! #[cfg(feature = "backend")]
//! let backend = manager.get_backend(vec![
//!     ForkQuery { chain_id: Some(1), block_number: None }
//! ]).await?;
//!
//! // Get multi-fork backend
//! #[cfg(feature = "backend")]
//! let multi_backend = manager.get_backend(vec![
//!     ForkQuery { chain_id: Some(1), block_number: None },    // Ethereum
//!     ForkQuery { chain_id: Some(10), block_number: None },   // Optimism
//!     ForkQuery { chain_id: Some(42161), block_number: None } // Arbitrum
//! ]).await?;
//! ```

mod config;
mod instance;
mod manager;

// Re-export config types
pub use config::{AnvilInstanceConfig, AnvilParams, ExternalConfig, ProvidersConfig};

// Re-export instance types
pub use instance::AnvilInstance;

// Re-export manager types
pub use manager::{
    ForkQuery, InstanceInfo, InstanceMetricsSnapshot, InstanceSource, ManagedInstance,
    ProviderManager,
};
