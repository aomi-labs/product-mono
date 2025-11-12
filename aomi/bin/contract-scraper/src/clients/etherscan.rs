use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EtherscanClient {
    client: reqwest::Client,
    api_key: Option<String>,
    base_urls: HashMap<i32, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtherscanResponse<T> {
    pub status: String,
    pub message: String,
    pub result: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSource {
    #[serde(rename = "SourceCode")]
    pub source_code: String,
    #[serde(rename = "ABI")]
    pub abi: String,
    #[serde(rename = "ContractName")]
    pub contract_name: String,
    #[serde(rename = "CompilerVersion")]
    pub compiler_version: String,
    #[serde(rename = "OptimizationUsed")]
    pub optimization_used: String,
    #[serde(rename = "Proxy", default)]
    pub proxy: String,
    #[serde(rename = "Implementation", default)]
    pub implementation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    #[serde(rename = "blockNumber")]
    pub block_number: String,
    #[serde(rename = "timeStamp")]
    pub timestamp: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: String,
    #[serde(rename = "gasPrice")]
    pub gas_price: String,
    #[serde(rename = "gasUsed")]
    pub gas_used: String,
    #[serde(rename = "isError")]
    pub is_error: String,
}

impl EtherscanClient {
    pub fn new(api_key: Option<String>) -> Self {
        let base_urls = Self::default_base_urls();

        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
            api_key,
            base_urls,
        }
    }

    /// Default base URLs for various chains (all use Etherscan v2 API)
    fn default_base_urls() -> HashMap<i32, String> {
        let mut urls = HashMap::new();
        urls.insert(1, "https://api.etherscan.io/v2/api".to_string()); // Ethereum Mainnet
        urls.insert(137, "https://api.polygonscan.com/v2/api".to_string()); // Polygon
        urls.insert(42161, "https://api.arbiscan.io/v2/api".to_string()); // Arbitrum
        urls.insert(8453, "https://api.basescan.org/v2/api".to_string()); // Base
        urls.insert(10, "https://api-optimistic.etherscan.io/v2/api".to_string()); // Optimism
        urls
    }

    /// Get contract source code and ABI
    pub async fn get_contract_source(
        &self,
        chain_id: i32,
        address: &str,
    ) -> Result<ContractSource> {
        let base_url = self
            .base_urls
            .get(&chain_id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported chain_id: {}", chain_id))?;

        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Etherscan API key configured"))?;

        let url = format!(
            "{}?chainid={}&module=contract&action=getsourcecode&address={}&apikey={}",
            base_url, chain_id, address, api_key
        );

        tracing::debug!(
            "Fetching contract source for {} on chain {}",
            address,
            chain_id
        );

        // Respect rate limits (5 calls/sec for free tier)
        tokio::time::sleep(Duration::from_millis(200)).await;

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Etherscan API request failed with status: {}",
                response.status()
            );
        }

        let api_response: EtherscanResponse<Vec<ContractSource>> = response.json().await?;

        if api_response.status != "1" {
            anyhow::bail!(
                "Etherscan API error: {} - {}",
                api_response.status,
                api_response.message
            );
        }

        api_response
            .result
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No contract source found for address: {}", address))
    }

    /// Get transaction count for a contract
    pub async fn get_transaction_count(&self, chain_id: i32, address: &str) -> Result<u64> {
        let base_url = self
            .base_urls
            .get(&chain_id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported chain_id: {}", chain_id))?;

        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Etherscan API key configured"))?;

        let url = format!(
            "{}?chainid={}&module=account&action=txlist&address={}&startblock=0&endblock=99999999&page=1&offset=1&sort=desc&apikey={}",
            base_url, chain_id, address, api_key
        );

        tracing::debug!(
            "Fetching transaction count for {} on chain {}",
            address,
            chain_id
        );

        // Respect rate limits
        tokio::time::sleep(Duration::from_millis(200)).await;

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Etherscan API request failed with status: {}",
                response.status()
            );
        }

        let api_response: EtherscanResponse<Vec<Transaction>> = response.json().await?;

        if api_response.status != "1" {
            // If no transactions found, return 0
            if api_response.message.contains("No transactions found") {
                return Ok(0);
            }
            anyhow::bail!(
                "Etherscan API error: {} - {}",
                api_response.status,
                api_response.message
            );
        }

        Ok(api_response.result.len() as u64)
    }

    /// Detect if a contract is a proxy and get implementation address
    pub async fn detect_proxy(
        &self,
        chain_id: i32,
        address: &str,
    ) -> Result<(bool, Option<String>)> {
        let source = self.get_contract_source(chain_id, address).await?;

        let is_proxy = source.proxy == "1" || !source.implementation.is_empty();
        let implementation = if !source.implementation.is_empty() {
            Some(source.implementation.clone())
        } else {
            None
        };

        Ok((is_proxy, implementation))
    }

    /// Get the last activity timestamp for a contract
    pub async fn get_last_activity(&self, chain_id: i32, address: &str) -> Result<Option<i64>> {
        let base_url = self
            .base_urls
            .get(&chain_id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported chain_id: {}", chain_id))?;

        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Etherscan API key configured"))?;

        let url = format!(
            "{}?chainid={}&module=account&action=txlist&address={}&startblock=0&endblock=99999999&page=1&offset=1&sort=desc&apikey={}",
            base_url, chain_id, address, api_key
        );

        // Respect rate limits
        tokio::time::sleep(Duration::from_millis(200)).await;

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Etherscan API request failed with status: {}",
                response.status()
            );
        }

        let api_response: EtherscanResponse<Vec<Transaction>> = response.json().await?;

        if api_response.status != "1" {
            return Ok(None);
        }

        if let Some(tx) = api_response.result.first() {
            let timestamp = tx.timestamp.parse::<i64>()?;
            Ok(Some(timestamp))
        } else {
            Ok(None)
        }
    }
}

/// Helper function to convert chain name to chain ID
pub fn chain_to_chain_id(chain: &str) -> Result<i32> {
    match chain.to_lowercase().as_str() {
        "ethereum" | "eth" => Ok(1),
        "polygon" | "matic" => Ok(137),
        "arbitrum" | "arb" => Ok(42161),
        "base" => Ok(8453),
        "optimism" | "op" => Ok(10),
        _ => Err(anyhow::anyhow!("Unknown chain: {}", chain)),
    }
}

/// Helper function to convert chain ID to chain name
pub fn chain_id_to_chain_name(chain_id: i32) -> Result<String> {
    match chain_id {
        1 => Ok("ethereum".to_string()),
        137 => Ok("polygon".to_string()),
        42161 => Ok("arbitrum".to_string()),
        8453 => Ok("base".to_string()),
        10 => Ok("optimism".to_string()),
        _ => Err(anyhow::anyhow!("Unknown chain_id: {}", chain_id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_conversion() {
        assert_eq!(chain_to_chain_id("ethereum").unwrap(), 1);
        assert_eq!(chain_to_chain_id("polygon").unwrap(), 137);
        assert_eq!(chain_to_chain_id("arbitrum").unwrap(), 42161);

        assert_eq!(chain_id_to_chain_name(1).unwrap(), "ethereum");
        assert_eq!(chain_id_to_chain_name(137).unwrap(), "polygon");
        assert_eq!(chain_id_to_chain_name(42161).unwrap(), "arbitrum");
    }

    #[test]
    fn test_default_base_urls() {
        let urls = EtherscanClient::default_base_urls();
        assert!(urls.contains_key(&1));
        assert!(urls.contains_key(&137));
        assert!(urls.contains_key(&42161));
        assert_eq!(urls.get(&1).unwrap(), "https://api.etherscan.io/v2/api");
    }
}
