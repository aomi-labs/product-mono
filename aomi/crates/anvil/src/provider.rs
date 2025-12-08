use crate::config::{ForkConfig, ForksProviderConfig};
use crate::instance::AnvilInstance;
use anyhow::Result;
use tokio::sync::OnceCell;

static FORK_PROVIDERS: OnceCell<Vec<ForkProvider>> = OnceCell::const_new();

pub enum ForkProvider {
    Anvil(AnvilInstance),
    External(String),
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

    pub async fn spawn(config: ForkConfig) -> Result<Self> {
        let instance = AnvilInstance::spawn(config).await?;
        Ok(Self::Anvil(instance))
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

pub async fn init_fork_providers(config: ForksProviderConfig) -> Result<&'static Vec<ForkProvider>> {
    FORK_PROVIDERS
        .get_or_try_init(|| async {
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

            if providers.is_empty() {
                anyhow::bail!(
                    "{} not set and auto_spawn disabled or no fork configs provided",
                    config.env_var
                );
            }

            Ok(providers)
        })
        .await
}

pub async fn init_fork_provider(config: ForksProviderConfig) -> Result<&'static ForkProvider> {
    let providers = init_fork_providers(config).await?;
    providers.first().ok_or_else(|| anyhow::anyhow!("No fork providers initialized"))
}

pub async fn init_fork_provider_external(url: impl Into<String>) -> &'static ForkProvider {
    let providers = FORK_PROVIDERS
        .get_or_init(|| async { vec![ForkProvider::External(url.into())] })
        .await;
    &providers[0]
}

pub async fn init_fork_provider_anvil(config: ForkConfig) -> Result<&'static ForkProvider> {
    let providers = FORK_PROVIDERS
        .get_or_try_init(|| async {
            let provider = ForkProvider::spawn(config).await?;
            Ok::<Vec<ForkProvider>, anyhow::Error>(vec![provider])
        })
        .await?;
    Ok(&providers[0])
}

pub fn fork_providers() -> &'static Vec<ForkProvider> {
    FORK_PROVIDERS
        .get()
        .expect("ForkProviders not initialized - call init_fork_providers() at startup")
}

pub fn fork_provider() -> &'static ForkProvider {
    &fork_providers()[0]
}

pub fn fork_provider_at(index: usize) -> Option<&'static ForkProvider> {
    fork_providers().get(index)
}

pub fn try_fork_providers() -> Option<&'static Vec<ForkProvider>> {
    FORK_PROVIDERS.get()
}

pub fn try_fork_provider() -> Option<&'static ForkProvider> {
    try_fork_providers().and_then(|p| p.first())
}

pub fn is_fork_provider_initialized() -> bool {
    FORK_PROVIDERS.get().is_some()
}

pub fn fork_endpoint() -> &'static str {
    fork_provider().endpoint()
}

pub fn fork_endpoint_at(index: usize) -> Option<&'static str> {
    fork_provider_at(index).map(|p| p.endpoint())
}

pub fn num_fork_providers() -> usize {
    try_fork_providers().map(|p| p.len()).unwrap_or(0)
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

    #[test]
    fn test_external_provider() {
        let provider = ForkProvider::external("http://localhost:8545");
        assert_eq!(provider.endpoint(), "http://localhost:8545");
        assert!(!provider.is_managed());
        assert!(provider.chain_id().is_none());
    }
}
