use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub coingecko_api_key: Option<String>,
    pub etherscan_api_key: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set"))?;

        let coingecko_api_key = std::env::var("COINGECKO_API_KEY").ok();

        // Single API key for all Etherscan v2 compatible explorers
        let etherscan_api_key = std::env::var("ETHERSCAN_API_KEY").ok();

        Ok(Self {
            database_url,
            coingecko_api_key,
            etherscan_api_key,
        })
    }

    /// Check if we have an Etherscan API key configured
    pub fn has_etherscan_key(&self) -> bool {
        self.etherscan_api_key.is_some()
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Check database URL format
        if !self.database_url.starts_with("postgres://")
            && !self.database_url.starts_with("postgresql://")
        {
            anyhow::bail!("DATABASE_URL must be a PostgreSQL connection string");
        }

        // Warn if no Etherscan API key is configured
        if self.etherscan_api_key.is_none() {
            tracing::warn!(
                "No Etherscan API key configured. Contract source code fetching will not work."
            );
            tracing::warn!("Set ETHERSCAN_API_KEY to enable scraping.");
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
    fn test_has_etherscan_key() {
        let config_with_key = Config {
            database_url: "postgresql://localhost/test".to_string(),
            coingecko_api_key: None,
            etherscan_api_key: Some("test_key".to_string()),
        };

        let config_without_key = Config {
            database_url: "postgresql://localhost/test".to_string(),
            coingecko_api_key: None,
            etherscan_api_key: None,
        };

        assert!(config_with_key.has_etherscan_key());
        assert!(!config_without_key.has_etherscan_key());
    }

    #[test]
    fn test_validate_database_url() {
        let config = Config {
            database_url: "invalid://localhost/test".to_string(),
            coingecko_api_key: None,
            etherscan_api_key: Some("key".to_string()),
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_postgres_url() {
        let config = Config {
            database_url: "postgresql://localhost/test".to_string(),
            coingecko_api_key: None,
            etherscan_api_key: Some("key".to_string()),
        };

        assert!(config.validate().is_ok());
    }
}
