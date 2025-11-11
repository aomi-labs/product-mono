use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DefiLlamaClient {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Protocol {
    pub id: String,
    pub name: String,
    pub symbol: Option<String>,
    pub chains: Vec<String>,
    pub tvl: f64,
    pub description: Option<String>,
    #[serde(rename = "chainTvls", default)]
    pub chain_tvls: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolDetail {
    pub name: String,
    pub symbol: Option<String>,
    pub chains: Vec<String>,
    pub tvl: f64,
    pub description: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub contracts: HashMap<String, Vec<String>>,
}

impl DefiLlamaClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
            base_url: "https://api.llama.fi".to_string(),
        }
    }

    /// Get all protocols from DeFi Llama
    pub async fn get_protocols(&self) -> Result<Vec<Protocol>> {
        let url = format!("{}/protocols", self.base_url);
        tracing::debug!("Fetching protocols from: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "DeFi Llama API request failed with status: {}",
                response.status()
            );
        }

        let protocols: Vec<Protocol> = response.json().await?;
        tracing::info!("Fetched {} protocols from DeFi Llama", protocols.len());

        Ok(protocols)
    }

    /// Get detailed information about a specific protocol
    pub async fn get_protocol(&self, name: &str) -> Result<ProtocolDetail> {
        let url = format!("{}/protocol/{}", self.base_url, name);
        tracing::debug!("Fetching protocol details from: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "DeFi Llama API request failed with status: {} for protocol: {}",
                response.status(),
                name
            );
        }

        let protocol: ProtocolDetail = response.json().await?;
        tracing::debug!("Fetched details for protocol: {}", name);

        Ok(protocol)
    }

    /// Filter protocols by TVL threshold and specific chains
    pub fn filter_by_tvl_and_chains(
        protocols: Vec<Protocol>,
        min_tvl: f64,
        chains: &[String],
    ) -> Vec<Protocol> {
        protocols
            .into_iter()
            .filter(|p| p.tvl >= min_tvl)
            .filter(|p| {
                if chains.is_empty() {
                    true
                } else {
                    p.chains.iter().any(|c| chains.contains(c))
                }
            })
            .collect()
    }

    /// Sort protocols by TVL in descending order
    pub fn sort_by_tvl(mut protocols: Vec<Protocol>) -> Vec<Protocol> {
        protocols.sort_by(|a, b| b.tvl.partial_cmp(&a.tvl).unwrap_or(std::cmp::Ordering::Equal));
        protocols
    }
}

impl Default for DefiLlamaClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_by_tvl() {
        let protocols = vec![
            Protocol {
                id: "1".to_string(),
                name: "High TVL".to_string(),
                symbol: None,
                chains: vec!["ethereum".to_string()],
                tvl: 1000000.0,
                description: None,
                chain_tvls: HashMap::new(),
            },
            Protocol {
                id: "2".to_string(),
                name: "Low TVL".to_string(),
                symbol: None,
                chains: vec!["ethereum".to_string()],
                tvl: 100.0,
                description: None,
                chain_tvls: HashMap::new(),
            },
        ];

        let filtered = DefiLlamaClient::filter_by_tvl_and_chains(protocols, 1000.0, &[]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "High TVL");
    }

    #[test]
    fn test_sort_by_tvl() {
        let protocols = vec![
            Protocol {
                id: "1".to_string(),
                name: "Low".to_string(),
                symbol: None,
                chains: vec![],
                tvl: 100.0,
                description: None,
                chain_tvls: HashMap::new(),
            },
            Protocol {
                id: "2".to_string(),
                name: "High".to_string(),
                symbol: None,
                chains: vec![],
                tvl: 1000.0,
                description: None,
                chain_tvls: HashMap::new(),
            },
        ];

        let sorted = DefiLlamaClient::sort_by_tvl(protocols);
        assert_eq!(sorted[0].name, "High");
        assert_eq!(sorted[1].name, "Low");
    }
}
