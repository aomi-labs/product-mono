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
//! use aomi_anvil::{default_manager, set_providers_path, ForkQuery};
//!
//! // Optionally configure custom path before first use (must be called before default_manager)
//! set_providers_path("path/to/providers.test.toml");
//!
//! // Get the static manager (resolves path via set_providers_path > env var > directory walk)
//! let manager = default_manager().await?;
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

use alloy::primitives::{Address, B256};
use alloy::signers::local::PrivateKeySigner;
use anyhow::Result;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tokio::process::Command;
use tokio::sync::OnceCell;

// Re-export config types
pub use config::{AnvilInstanceConfig, ExternalConfig, ProvidersConfig};

// Re-export instance types
pub use instance::{InstanceInfo, InstanceMetricsSnapshot, InstanceSource, ManagedInstance};

// Re-export manager types
pub use manager::{ForkQuery, ProviderManager};

/// Configured providers path (set via `set_providers_path` before `default_manager` is called)
static PROVIDERS_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Load a ProviderManager from the default providers.toml path.
static PROVIDER_MANAGER: Lazy<OnceCell<Arc<ProviderManager>>> = Lazy::new(OnceCell::new);

/// Set a custom providers.toml path before the static manager is initialized.
///
/// Priority order for path resolution:
/// 1. Path set via `set_providers_path()` (this function)
/// 2. `PROVIDERS_TOML` environment variable
/// 3. Directory walk from current directory
///
/// Must be called before the first call to `default_manager()`.
/// Returns `true` if the path was set, `false` if already configured.
pub fn set_providers_path(path: impl Into<PathBuf>) -> bool {
    PROVIDERS_PATH.set(path.into()).is_ok()
}

/// Get the configured providers path, if set.
pub fn get_providers_path() -> Option<&'static PathBuf> {
    PROVIDERS_PATH.get()
}

pub async fn provider_manager() -> Result<Arc<ProviderManager>> {
    PROVIDER_MANAGER
        .get_or_try_init(|| async {
            // Load config first to identify protected ports (external endpoints)
            let protected_ports = get_external_localhost_ports();

            // Clean slate: kill orphaned anvil processes, but preserve external endpoints
            let killed = cleanup_anvil_processes(&protected_ports).await;
            if killed > 0 {
                tracing::info!(
                    count = killed,
                    protected = ?protected_ports,
                    "Cleaned up orphaned anvil processes before startup"
                );
            }

            ProviderManager::from_default_config().await.map(Arc::new)
        })
        .await
        .map(Arc::clone)
}

/// Extract localhost ports from external endpoints in the config.
/// These ports should be preserved during cleanup (not killed).
fn get_external_localhost_ports() -> Vec<u16> {
    let config = match resolve_providers_path() {
        Ok(path) => ProvidersConfig::from_file(&path).ok(),
        Err(_) => None,
    };

    let Some(config) = config else {
        return Vec::new();
    };

    config
        .external
        .values()
        .filter_map(|ext| extract_localhost_port(&ext.rpc_url))
        .collect()
}

/// Extract port from a localhost URL (e.g., "http://127.0.0.1:8545" -> Some(8545))
fn extract_localhost_port(url_str: &str) -> Option<u16> {
    let url = reqwest::Url::parse(url_str).ok()?;
    let host = url.host_str()?;

    // Check if it's localhost
    if host == "127.0.0.1" || host == "localhost" || host == "0.0.0.0" {
        url.port()
    } else {
        None
    }
}

/// Kill orphaned anvil processes, preserving those on protected ports.
///
/// This is called automatically by `provider_manager()` to ensure a clean slate
/// before spawning new instances. Anvils on protected ports (external endpoints)
/// are preserved.
///
/// Returns the number of processes killed.
pub async fn cleanup_anvil_processes(protected_ports: &[u16]) -> usize {
    if protected_ports.is_empty() {
        // No protected ports - kill all anvils
        return cleanup_all_anvils().await;
    }

    // Get list of anvil PIDs and their ports, then selectively kill
    let pids_to_kill = get_anvil_pids_excluding_ports(protected_ports).await;

    if pids_to_kill.is_empty() {
        tracing::debug!("No orphaned anvil processes to clean up");
        return 0;
    }

    let mut killed = 0;
    for pid in &pids_to_kill {
        let result = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output()
            .await;

        if result.is_ok_and(|r| r.status.success()) {
            killed += 1;
        }
    }

    killed
}

/// Kill all anvil processes (no protection)
async fn cleanup_all_anvils() -> usize {
    let output = Command::new("pkill")
        .args(["-9", "-f", "anvil"])
        .output()
        .await;

    match output {
        Ok(result) => {
            if result.status.success() {
                tracing::debug!("pkill anvil succeeded");
                1
            } else {
                tracing::debug!("No existing anvil processes found");
                0
            }
        }
        Err(e) => {
            tracing::warn!("Failed to run pkill: {}. Continuing anyway.", e);
            0
        }
    }
}

/// Get PIDs of anvil processes NOT running on protected ports
async fn get_anvil_pids_excluding_ports(protected_ports: &[u16]) -> Vec<u32> {
    // Get all anvil processes with their command lines
    let output = Command::new("pgrep").args(["-f", "anvil"]).output().await;

    let Ok(result) = output else {
        return Vec::new();
    };

    if !result.status.success() {
        return Vec::new();
    }

    let pids: Vec<u32> = String::from_utf8_lossy(&result.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();

    // For each PID, check if it's running on a protected port
    let mut pids_to_kill = Vec::new();
    for pid in pids {
        if !is_anvil_on_protected_port(pid, protected_ports).await {
            pids_to_kill.push(pid);
        }
    }

    pids_to_kill
}

/// Check if an anvil process (by PID) is running on one of the protected ports
async fn is_anvil_on_protected_port(pid: u32, protected_ports: &[u16]) -> bool {
    // Get the command line for this PID
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "args="])
        .output()
        .await;

    let Ok(result) = output else {
        return false;
    };

    if !result.status.success() {
        return false;
    }

    let cmdline = String::from_utf8_lossy(&result.stdout);

    // Check if any protected port appears in the command line
    for port in protected_ports {
        // Look for patterns like "--port 8545" or "--port=8545"
        let patterns = [
            format!("--port {}", port),
            format!("--port={}", port),
            format!("-p {}", port),
            format!("-p={}", port),
        ];

        for pattern in &patterns {
            if cmdline.contains(pattern) {
                tracing::debug!(pid = pid, port = port, "Preserving anvil on protected port");
                return true;
            }
        }
    }

    false
}

/// Count the number of running anvil processes on the system.
pub async fn count_anvil_processes() -> usize {
    let output = Command::new("pgrep").args(["-f", "anvil"]).output().await;

    match output {
        Ok(result) => {
            if result.status.success() {
                String::from_utf8_lossy(&result.stdout).lines().count()
            } else {
                0
            }
        }
        Err(_) => 0,
    }
}

/// Resolve the providers.toml path (re-exported from lifecycle for internal use)
fn resolve_providers_path() -> Result<PathBuf> {
    lifecycle::resolve_providers_path()
}

/// Standard account names for eval/test mode.
/// Index 0 is Alice (the primary test wallet), index 1 is Bob (the counterparty).
pub const EVAL_ACCOUNT_NAMES: &[&str] = &["Alice", "Bob"];

/// An autosign wallet with private key, derived address, and optional name.
#[derive(Debug, Clone)]
pub struct AutosignWallet {
    /// The private key (32 bytes)
    pub private_key: B256,
    /// The derived Ethereum address
    pub address: Address,
    /// Optional name for eval/test mode (e.g., "Alice", "Bob")
    pub name: Option<&'static str>,
}

impl AutosignWallet {
    /// Create a new AutosignWallet from a hex private key string.
    pub fn from_hex(hex_key: &str) -> Result<Self> {
        let signer: PrivateKeySigner = hex_key
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid private key '{}': {}", hex_key, e))?;

        let address = signer.address();
        let private_key = B256::from_slice(signer.credential().to_bytes().as_slice());

        Ok(Self {
            private_key,
            address,
            name: None,
        })
    }

    /// Create a new AutosignWallet with a name.
    pub fn from_hex_with_name(hex_key: &str, name: &'static str) -> Result<Self> {
        let mut wallet = Self::from_hex(hex_key)?;
        wallet.name = Some(name);
        Ok(wallet)
    }
}

/// Get the Alice wallet (autosign[0]) - the primary test wallet.
pub fn alice_wallet() -> Result<AutosignWallet> {
    load_autosign_wallets()?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No Alice account configured in autosign_keys"))
}

/// Get the Bob wallet (autosign[1]) - the counterparty wallet.
pub fn bob_wallet() -> Result<AutosignWallet> {
    load_autosign_wallets()?
        .into_iter()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("No Bob account configured in autosign_keys"))
}

/// Load autosign wallets from providers.toml.
///
/// These wallets should automatically sign transactions in eval-test mode.
/// Private keys are stored in the config, and addresses are derived at runtime.
/// Names are assigned in order from `EVAL_ACCOUNT_NAMES` (Alice, Bob, etc.).
///
/// Returns an empty vector if the config doesn't specify any autosign keys.
pub fn load_autosign_wallets() -> Result<Vec<AutosignWallet>> {
    let path = resolve_providers_path()?;
    let config = ProvidersConfig::from_file(&path)?;

    config
        .autosign_keys
        .iter()
        .enumerate()
        .map(|(i, hex_key)| {
            let name = EVAL_ACCOUNT_NAMES.get(i).copied();
            let mut wallet = AutosignWallet::from_hex(hex_key)?;
            wallet.name = name;
            Ok(wallet)
        })
        .collect()
}
