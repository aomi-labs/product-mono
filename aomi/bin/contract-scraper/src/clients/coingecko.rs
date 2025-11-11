use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Simple token bucket rate limiter
#[derive(Debug, Clone)]
pub struct RateLimiter {
    tokens: Arc<Mutex<f64>>,
    rate: f64,         // tokens per second
    capacity: f64,     // max tokens
    last_update: Arc<Mutex<Instant>>,
}

impl RateLimiter {
    pub fn new(rate: f64, capacity: f64) -> Self {
        Self {
            tokens: Arc::new(Mutex::new(capacity)),
            rate,
            capacity,
            last_update: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub async fn acquire(&self) -> Result<()> {
        loop {
            let mut tokens = self.tokens.lock().await;
            let mut last_update = self.last_update.lock().await;

            let now = Instant::now();
            let elapsed = now.duration_since(*last_update).as_secs_f64();

            // Refill tokens based on elapsed time
            *tokens = (*tokens + elapsed * self.rate).min(self.capacity);
            *last_update = now;

            if *tokens >= 1.0 {
                *tokens -= 1.0;
                return Ok(());
            }

            // Calculate wait time
            let wait_time = ((1.0 - *tokens) / self.rate).max(0.0);
            drop(tokens);
            drop(last_update);

            tokio::time::sleep(Duration::from_secs_f64(wait_time)).await;
        }
    }
}

#[derive(Debug, Clone)]
pub struct CoinGeckoClient {
    client: reqwest::Client,
    api_key: Option<String>,
    rate_limiter: RateLimiter,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinData {
    pub id: String,
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub platforms: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinDetail {
    pub id: String,
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub description: HashMap<String, String>,
    #[serde(default)]
    pub platforms: HashMap<String, String>,
}

impl CoinGeckoClient {
    pub fn new(api_key: Option<String>) -> Self {
        // Free tier: 10-50 calls/min, use conservative rate
        let rate_limiter = RateLimiter::new(0.2, 10.0); // ~12 calls per minute

        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
            api_key,
            rate_limiter,
            base_url: "https://api.coingecko.com/api/v3".to_string(),
        }
    }

    /// Get list of all coins with platform information
    pub async fn get_coins_list(&self) -> Result<Vec<CoinData>> {
        self.rate_limiter.acquire().await?;

        let mut url = format!("{}/coins/list?include_platform=true", self.base_url);
        if let Some(key) = &self.api_key {
            url.push_str(&format!("&x_cg_api_key={}", key));
        }

        tracing::debug!("Fetching coins list from CoinGecko");

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "CoinGecko API request failed with status: {}",
                response.status()
            );
        }

        let coins: Vec<CoinData> = response.json().await?;
        tracing::info!("Fetched {} coins from CoinGecko", coins.len());

        Ok(coins)
    }

    /// Get detailed information about a specific coin
    pub async fn get_coin_by_id(&self, id: &str) -> Result<CoinDetail> {
        self.rate_limiter.acquire().await?;

        let mut url = format!("{}/coins/{}", self.base_url, id);
        if let Some(key) = &self.api_key {
            url.push_str(&format!("?x_cg_api_key={}", key));
        }

        tracing::debug!("Fetching coin details for: {}", id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "CoinGecko API request failed with status: {} for coin: {}",
                response.status(),
                id
            );
        }

        let coin: CoinDetail = response.json().await?;
        tracing::debug!("Fetched details for coin: {}", id);

        Ok(coin)
    }

    /// Find contract address for a specific chain
    pub fn get_contract_address(coin: &CoinData, chain: &str) -> Option<String> {
        coin.platforms.get(chain).cloned()
    }

    /// Convert CoinGecko chain name to our internal chain name
    pub fn normalize_chain_name(coingecko_chain: &str) -> Option<String> {
        match coingecko_chain {
            "ethereum" => Some("ethereum".to_string()),
            "polygon-pos" => Some("polygon".to_string()),
            "arbitrum-one" => Some("arbitrum".to_string()),
            "optimistic-ethereum" => Some("optimism".to_string()),
            "base" => Some("base".to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_chain_name() {
        assert_eq!(
            CoinGeckoClient::normalize_chain_name("ethereum"),
            Some("ethereum".to_string())
        );
        assert_eq!(
            CoinGeckoClient::normalize_chain_name("polygon-pos"),
            Some("polygon".to_string())
        );
        assert_eq!(
            CoinGeckoClient::normalize_chain_name("arbitrum-one"),
            Some("arbitrum".to_string())
        );
        assert_eq!(CoinGeckoClient::normalize_chain_name("unknown"), None);
    }

    #[test]
    fn test_get_contract_address() {
        let mut platforms = HashMap::new();
        platforms.insert("ethereum".to_string(), "0x123".to_string());

        let coin = CoinData {
            id: "test".to_string(),
            symbol: "TEST".to_string(),
            name: "Test Token".to_string(),
            platforms,
        };

        assert_eq!(
            CoinGeckoClient::get_contract_address(&coin, "ethereum"),
            Some("0x123".to_string())
        );
        assert_eq!(
            CoinGeckoClient::get_contract_address(&coin, "polygon"),
            None
        );
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(10.0, 10.0); // 10 tokens per second

        let start = Instant::now();

        // Should be able to acquire immediately
        limiter.acquire().await.unwrap();

        // Check that it took minimal time
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 100);
    }
}
