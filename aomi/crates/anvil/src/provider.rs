use crate::config::{AnvilParams, ForksConfig};
use crate::instance::AnvilInstance;
use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};

static FORK_PROVIDERS: Lazy<Arc<RwLock<Option<Vec<ForkProvider>>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));
static LAST_CONFIG: Lazy<Arc<RwLock<Option<ForksConfig>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

pub enum ForkProvider {
    Anvil(AnvilInstance),
    External(String),
}

#[derive(Clone, Debug)]
pub struct ForkProviderSnapshot {
    endpoint: String,
    chain_id: Option<u64>,
    is_spawned: bool,
}

impl ForkProviderSnapshot {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn chain_id(&self) -> Option<u64> {
        self.chain_id
    }

    pub fn is_spawned(&self) -> bool {
        self.is_spawned
    }
}

impl From<&ForkProvider> for ForkProviderSnapshot {
    fn from(provider: &ForkProvider) -> Self {
        match provider {
            ForkProvider::Anvil(instance) => Self {
                endpoint: instance.endpoint().to_string(),
                chain_id: Some(instance.chain_id()),
                is_spawned: true,
            },
            ForkProvider::External(url) => Self {
                endpoint: url.clone(),
                chain_id: None,
                is_spawned: false,
            },
        }
    }
}

impl ForkProvider {
    pub fn endpoint(&self) -> &str {
        match self {
            Self::Anvil(instance) => instance.endpoint(),
            Self::External(url) => url,
        }
    }

    pub fn is_managed(&self) -> bool {
        matches!(self, Self::Anvil(_))
    }

    pub fn chain_id(&self) -> Option<u64> {
        match self {
            Self::Anvil(instance) => Some(instance.chain_id()),
            Self::External(_) => None,
        }
    }

    pub fn external(url: impl Into<String>) -> Self {
        Self::External(url.into())
    }

    pub async fn spawn(config: AnvilParams) -> Result<Self> {
        let instance = AnvilInstance::spawn(config).await?;
        Ok(Self::Anvil(instance))
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        if let ForkProvider::Anvil(instance) = self {
            instance.kill().await?;
        }
        Ok(())
    }

    fn redact_url(url: &str) -> String {
        if url.contains("alchemy.com") || url.contains("infura.io") {
            if let Some(idx) = url.rfind('/') {
                let (base, key) = url.split_at(idx + 1);
                if key.len() > 8 {
                    return format!("{}...{}", base, &key[key.len() - 4..]);
                }
            }
        }
        url.to_string()
    }
}

async fn build_providers(config: ForksConfig) -> Result<Vec<ForkProvider>> {
    let mut providers = Vec::new();

    // Check for external RPC URL first
    if let Ok(url) = std::env::var(&config.env_var) {
        tracing::info!(
            "Using external RPC from {}: {}",
            config.env_var,
            ForkProvider::redact_url(&url)
        );
        providers.push(ForkProvider::External(url));
        return Ok(providers);
    }

    // Auto-spawn if enabled
    if config.auto_spawn && !config.forks.is_empty() {
        tracing::info!(
            "{} not set, auto-spawning {} Anvil instance(s)",
            config.env_var,
            config.forks.len()
        );

        for fork_config in config.forks {
            let provider = ForkProvider::spawn(fork_config).await?;
            providers.push(provider);
        }
        return Ok(providers);
    }

    bail!(
        "{} not set and auto_spawn disabled or no fork configs provided",
        config.env_var
    );
}

fn set_last_config(config: ForksConfig) {
    let mut guard = LAST_CONFIG.write().expect("poisoned last config lock");
    *guard = Some(config);
}

/// Initialize providers using the supplied config unless one is already active.
/// Returns a snapshot of the active providers.
pub async fn init_fork_providers(config: ForksConfig) -> Result<Vec<ForkProviderSnapshot>> {
    set_last_config(config.clone());

    if is_fork_provider_initialized() {
        return Ok(fork_providers());
    }

    let providers = build_providers(config).await?;
    {
        let mut guard = FORK_PROVIDERS
            .write()
            .expect("poisoned fork providers lock");
        if guard.is_none() {
            *guard = Some(providers);
        }
    }

    Ok(fork_providers())
}

pub async fn init_fork_provider(config: ForksConfig) -> Result<ForkProviderSnapshot> {
    let providers = init_fork_providers(config).await?;
    providers
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No fork providers initialized"))
}

pub async fn init_fork_provider_external(url: impl Into<String>) -> ForkProviderSnapshot {
    let url = url.into();
    set_last_config(ForksConfig::external_only());
    {
        let mut guard = FORK_PROVIDERS
            .write()
            .expect("poisoned fork providers lock");
        *guard = Some(vec![ForkProvider::external(url)]);
    }
    fork_provider()
}

pub async fn init_fork_provider_anvil(config: AnvilParams) -> Result<ForkProviderSnapshot> {
    let providers = init_fork_providers(ForksConfig::single(config)).await?;
    providers
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No fork providers initialized"))
}

pub async fn reset_fork_providers() -> Result<()> {
    let existing = {
        let mut guard = FORK_PROVIDERS
            .write()
            .expect("poisoned fork providers lock");
        guard.take()
    };

    if let Some(mut providers) = existing {
        for provider in providers.iter_mut() {
            provider.shutdown().await?;
        }
    }

    Ok(())
}

/// Reset and reinitialize using the last stored config. Useful for tests.
pub async fn reset_fork_providers_and_reinit() -> Result<Vec<ForkProviderSnapshot>> {
    let config = {
        let guard = LAST_CONFIG.read().expect("poisoned last config lock");
        guard
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No last ForksConfig stored for reinit"))?
    };

    reset_fork_providers().await?;
    init_fork_providers(config).await
}

pub fn fork_providers() -> Vec<ForkProviderSnapshot> {
    try_fork_providers().unwrap_or_default()
}

pub fn fork_provider() -> ForkProviderSnapshot {
    try_fork_provider()
        .expect("ForkProviders not initialized - call init_fork_providers() at startup")
}

pub fn fork_provider_at(index: usize) -> Option<ForkProviderSnapshot> {
    try_fork_providers().and_then(|p| p.get(index).cloned())
}

pub fn try_fork_providers() -> Option<Vec<ForkProviderSnapshot>> {
    let guard = FORK_PROVIDERS.read().expect("poisoned fork providers lock");
    guard
        .as_ref()
        .map(|providers| providers.iter().map(ForkProviderSnapshot::from).collect())
}

pub fn try_fork_provider() -> Option<ForkProviderSnapshot> {
    try_fork_providers().and_then(|mut p| p.drain(..).next())
}

pub fn is_fork_provider_initialized() -> bool {
    FORK_PROVIDERS
        .read()
        .expect("poisoned fork providers lock")
        .is_some()
}

pub fn fork_endpoint() -> String {
    fork_provider().endpoint().to_string()
}

pub fn try_fork_endpoint() -> Option<String> {
    try_fork_provider().map(|p| p.endpoint().to_string())
}

pub fn fork_endpoint_at(index: usize) -> Option<String> {
    fork_provider_at(index).map(|p| p.endpoint().to_string())
}

pub fn num_fork_providers() -> usize {
    FORK_PROVIDERS
        .read()
        .expect("poisoned fork providers lock")
        .as_ref()
        .map(|p| p.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_url() {
        assert_eq!(
            ForkProvider::redact_url("https://eth-mainnet.g.alchemy.com/v2/abc123def456"),
            "https://eth-mainnet.g.alchemy.com/v2/...f456"
        );

        assert_eq!(
            ForkProvider::redact_url("http://localhost:8545"),
            "http://localhost:8545"
        );
    }

    #[tokio::test]
    async fn test_external_provider_snapshot() {
        let snapshot = init_fork_provider_external("http://localhost:8545").await;
        assert_eq!(snapshot.endpoint(), "http://localhost:8545");
        assert!(!snapshot.is_spawned());
        assert!(snapshot.chain_id().is_none());
    }
}
