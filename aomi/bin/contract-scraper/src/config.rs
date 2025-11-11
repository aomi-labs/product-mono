use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub coingecko_api_key: Option<String>,
    pub etherscan_api_keys: HashMap<i32, String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set"))?;

        let coingecko_api_key = std::env::var("COINGECKO_API_KEY").ok();

        let etherscan_api_keys = Self::parse_etherscan_keys();

        Ok(Self {
            database_url,
            coingecko_api_key,
            etherscan_api_keys,
        })
    }

    fn parse_etherscan_keys() -> HashMap<i32, String> {
        let mut keys = HashMap::new();

        // Ethereum Mainnet (chain_id: 1)
        if let Ok(key) = std::env::var("ETHERSCAN_API_KEY") {
            keys.insert(1, key);
        }

        // Polygon (chain_id: 137)
        if let Ok(key) = std::env::var("POLYGONSCAN_API_KEY") {
            keys.insert(137, key);
        }

        // Arbitrum (chain_id: 42161)
        if let Ok(key) = std::env::var("ARBISCAN_API_KEY") {
            keys.insert(42161, key);
        }

        // Base (chain_id: 8453)
        if let Ok(key) = std::env::var("BASESCAN_API_KEY") {
            keys.insert(8453, key);
        }

        // Optimism (chain_id: 10)
        if let Ok(key) = std::env::var("OPTIMISM_API_KEY") {
            keys.insert(10, key);
        }

        keys
    }

    /// Check if we have an API key for a specific chain
    pub fn has_etherscan_key_for_chain(&self, chain_id: i32) -> bool {
        self.etherscan_api_keys.contains_key(&chain_id)
    }

    /// Get list of chains we can scrape (chains with API keys)
    pub fn available_chains(&self) -> Vec<i32> {
        self.etherscan_api_keys.keys().copied().collect()
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Check database URL format
        if !self.database_url.starts_with("postgres://")
            && !self.database_url.starts_with("postgresql://")
        {
            anyhow::bail!("DATABASE_URL must be a PostgreSQL connection string");
        }

        // Warn if no Etherscan API keys are configured
        if self.etherscan_api_keys.is_empty() {
            tracing::warn!(
                "No Etherscan API keys configured. Contract source code fetching will not work."
            );
            tracing::warn!("Set ETHERSCAN_API_KEY, POLYGONSCAN_API_KEY, etc. to enable scraping.");
        }

        // Warn if CoinGecko API key is missing
        if self.coingecko_api_key.is_none() {
            tracing::warn!("COINGECKO_API_KEY not set. Rate limits will be more restrictive.");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_etherscan_keys() {
        unsafe {
            // Save current env vars
            let original_eth = std::env::var("ETHERSCAN_API_KEY").ok();
            let original_poly = std::env::var("POLYGONSCAN_API_KEY").ok();

            // Set test env vars
            std::env::set_var("ETHERSCAN_API_KEY", "test_eth_key");
            std::env::set_var("POLYGONSCAN_API_KEY", "test_poly_key");

            let keys = Config::parse_etherscan_keys();

            assert_eq!(keys.get(&1), Some(&"test_eth_key".to_string()));
            assert_eq!(keys.get(&137), Some(&"test_poly_key".to_string()));

            // Restore original env vars
            if let Some(key) = original_eth {
                std::env::set_var("ETHERSCAN_API_KEY", key);
            } else {
                std::env::remove_var("ETHERSCAN_API_KEY");
            }
            if let Some(key) = original_poly {
                std::env::set_var("POLYGONSCAN_API_KEY", key);
            } else {
                std::env::remove_var("POLYGONSCAN_API_KEY");
            }
        }
    }

    #[test]
    fn test_available_chains() {
        let mut keys = HashMap::new();
        keys.insert(1, "key1".to_string());
        keys.insert(137, "key2".to_string());

        let config = Config {
            database_url: "postgresql://localhost/test".to_string(),
            coingecko_api_key: None,
            etherscan_api_keys: keys,
        };

        let chains = config.available_chains();
        assert_eq!(chains.len(), 2);
        assert!(chains.contains(&1));
        assert!(chains.contains(&137));
    }

    #[test]
    fn test_has_etherscan_key() {
        let mut keys = HashMap::new();
        keys.insert(1, "key".to_string());

        let config = Config {
            database_url: "postgresql://localhost/test".to_string(),
            coingecko_api_key: None,
            etherscan_api_keys: keys,
        };

        assert!(config.has_etherscan_key_for_chain(1));
        assert!(!config.has_etherscan_key_for_chain(137));
    }
}
