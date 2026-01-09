use anyhow::{Context, Result};
use aomi_anvil::ProviderManager;
use std::path::PathBuf;
use std::{env, io};

fn resolve_providers_path() -> Result<PathBuf> {
    if let Ok(path) = env::var("PROVIDERS_TOML") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        anyhow::bail!("PROVIDERS_TOML was set but not found: {}", path.display());
    }

    let mut dir = env::current_dir().map_err(|e| {
        anyhow::anyhow!(io::Error::new(
            e.kind(),
            format!("Failed to read current dir: {}", e),
        ))
    })?;

    loop {
        let candidate = dir.join("providers.toml");
        if candidate.exists() {
            return Ok(candidate);
        }

        if !dir.pop() {
            break;
        }
    }

    anyhow::bail!("providers.toml not found in current directory or ancestors");
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let providers_path = resolve_providers_path().context("Failed to resolve providers.toml")?;
    let manager = ProviderManager::from_config_file(&providers_path).await?;

    if let Some(info) = manager.get_instance_info_by_name("ethereum") {
        tracing::info!(
            name = %info.name,
            chain_id = info.chain_id,
            endpoint = %info.endpoint,
            "Anvil instance ready"
        );
    } else {
        tracing::warn!("No 'ethereum' instance found in providers.toml");
    }

    tracing::info!("ProviderManager running. Press Ctrl+C to shut down.");
    tokio::signal::ctrl_c().await?;
    manager.shutdown_all().await?;
    tracing::info!("ProviderManager shutdown complete.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_provider::Provider;
    use aomi_anvil::{AnvilInstanceConfig, ForkQuery, ProvidersConfig};
    use std::collections::HashMap;

    fn alchemy_key() -> String {
        std::env::var("ALCHEMY_API_KEY")
            .unwrap_or_else(|_| "4UjEl1ULr2lQYsGR5n7gGKd3pzgAzxKs".to_string())
    }

    fn anvil_available() -> bool {
        std::process::Command::new("anvil")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn config_for_instances(instances: Vec<(&str, u64, String)>) -> ProvidersConfig {
        let mut anvil_instances = HashMap::new();
        for (name, chain_id, fork_url) in instances {
            anvil_instances.insert(
                name.to_string(),
                AnvilInstanceConfig::new(chain_id, fork_url),
            );
        }

        ProvidersConfig {
            anvil_instances,
            external: HashMap::new(),
        }
    }

    fn disable_proxy_for_localhost() {
        std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
        std::env::set_var("no_proxy", "127.0.0.1,localhost");
        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("http_proxy");
        std::env::remove_var("HTTPS_PROXY");
        std::env::remove_var("https_proxy");
        std::env::remove_var("ALL_PROXY");
        std::env::remove_var("all_proxy");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_spawn_and_query_block_number() -> Result<()> {
        if !anvil_available() {
            eprintln!("Skipping test: anvil not installed");
            return Ok(());
        }

        disable_proxy_for_localhost();

        let key = alchemy_key();
        let config = config_for_instances(vec![(
            "ethereum",
            1,
            format!("https://eth-mainnet.g.alchemy.com/v2/{}", key),
        )]);

        let manager = ProviderManager::from_config(config).await?;
        let provider = manager.get_provider_by_name("ethereum").await?;
        let block_number = provider.get_block_number().await?;
        assert!(block_number > 0);
        manager.shutdown_all().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_spawn_multiple_forks_across_chains() -> Result<()> {
        if !anvil_available() {
            eprintln!("Skipping test: anvil not installed");
            return Ok(());
        }

        disable_proxy_for_localhost();

        let key = alchemy_key();
        let config = config_for_instances(vec![
            (
                "ethereum",
                1,
                format!("https://eth-mainnet.g.alchemy.com/v2/{}", key),
            ),
            (
                "optimism",
                10,
                format!("https://opt-mainnet.g.alchemy.com/v2/{}", key),
            ),
            (
                "arbitrum",
                42161,
                format!("https://arb-mainnet.g.alchemy.com/v2/{}", key),
            ),
            (
                "base",
                8453,
                format!("https://base-mainnet.g.alchemy.com/v2/{}", key),
            ),
        ]);

        let manager = ProviderManager::from_config(config).await?;
        assert_eq!(manager.instance_count(), 4);
        assert!(manager.get_instance_info_by_name("ethereum").is_some());
        assert!(manager.get_instance_info_by_name("optimism").is_some());
        assert!(manager.get_instance_info_by_name("arbitrum").is_some());
        assert!(manager.get_instance_info_by_name("base").is_some());
        manager.shutdown_all().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_op_provider_and_multi_backend() -> Result<()> {
        if !anvil_available() {
            eprintln!("Skipping test: anvil not installed");
            return Ok(());
        }

        disable_proxy_for_localhost();

        let key = alchemy_key();
        let config = config_for_instances(vec![
            (
                "optimism",
                10,
                format!("https://opt-mainnet.g.alchemy.com/v2/{}", key),
            ),
            (
                "arbitrum",
                42161,
                format!("https://arb-mainnet.g.alchemy.com/v2/{}", key),
            ),
            (
                "base",
                8453,
                format!("https://base-mainnet.g.alchemy.com/v2/{}", key),
            ),
        ]);

        let manager = ProviderManager::from_config(config).await?;

        let provider = manager.get_provider_by_name("optimism").await?;
        let block_number = provider.get_block_number().await?;
        assert!(block_number > 0);

        let backend = manager
            .get_backend(vec![
                ForkQuery {
                    chain_id: Some(10),
                    block_number: None,
                },
                ForkQuery {
                    chain_id: Some(42161),
                    block_number: None,
                },
                ForkQuery {
                    chain_id: Some(8453),
                    block_number: None,
                },
            ])
            .await?;

        drop(backend);
        manager.shutdown_all().await?;
        Ok(())
    }
}
