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
    pub slug: String,
    pub symbol: Option<String>,
    #[serde(default)]
    pub chains: Vec<String>,
    pub tvl: Option<f64>,
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(rename = "chainTvls", default)]
    pub chain_tvls: HashMap<String, Option<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolDetail {
    pub name: String,
    pub symbol: Option<String>,
    pub chain: String,
    #[serde(default)]
    pub chains: Vec<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    // Note: tvl in protocol detail is a time-series array, not a single value
    // We use the TVL from the protocol list instead
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
        // Normalize chain names to lowercase for comparison
        let chains_lower: Vec<String> = chains.iter().map(|c| c.to_lowercase()).collect();

        protocols
            .into_iter()
            .filter(|p| {
                // Filter out CEX (centralized exchanges) - they don't have smart contracts
                if let Some(ref category) = p.category {
                    if category.to_uppercase() == "CEX" {
                        return false;
                    }
                }
                true
            })
            .filter(|p| {
                // Filter by TVL - keep protocols with TVL >= min_tvl
                match p.tvl {
                    Some(tvl) => tvl >= min_tvl,
                    None => false, // Skip protocols without TVL data
                }
            })
            .filter(|p| {
                if chains.is_empty() {
                    true
                } else {
                    // Case-insensitive chain comparison
                    p.chains.iter().any(|c| chains_lower.contains(&c.to_lowercase()))
                }
            })
            .collect()
    }

    /// Sort protocols by TVL in descending order (None values go to the end)
    pub fn sort_by_tvl(mut protocols: Vec<Protocol>) -> Vec<Protocol> {
        protocols.sort_by(|a, b| {
            match (a.tvl, b.tvl) {
                (Some(a_tvl), Some(b_tvl)) => b_tvl.partial_cmp(&a_tvl).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,    // a has value, b doesn't - a comes first
                (None, Some(_)) => std::cmp::Ordering::Greater, // b has value, a doesn't - b comes first
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
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
                slug: "high-tvl".to_string(),
                symbol: None,
                chains: vec!["ethereum".to_string()],
                tvl: Some(1000000.0),
                description: None,
                category: Some("Liquid Staking".to_string()),
                chain_tvls: HashMap::new(),
            },
            Protocol {
                id: "2".to_string(),
                name: "Low TVL".to_string(),
                slug: "low-tvl".to_string(),
                symbol: None,
                chains: vec!["ethereum".to_string()],
                tvl: Some(100.0),
                description: None,
                category: Some("Lending".to_string()),
                chain_tvls: HashMap::new(),
            },
            Protocol {
                id: "3".to_string(),
                name: "No TVL".to_string(),
                slug: "no-tvl".to_string(),
                symbol: None,
                chains: vec!["ethereum".to_string()],
                tvl: None,
                description: None,
                category: Some("DEX".to_string()),
                chain_tvls: HashMap::new(),
            },
            Protocol {
                id: "4".to_string(),
                name: "CEX High TVL".to_string(),
                slug: "cex-high-tvl".to_string(),
                symbol: None,
                chains: vec!["ethereum".to_string()],
                tvl: Some(5000000.0),
                description: None,
                category: Some("CEX".to_string()),
                chain_tvls: HashMap::new(),
            },
        ];

        let filtered = DefiLlamaClient::filter_by_tvl_and_chains(protocols, 1000.0, &[]);
        assert_eq!(filtered.len(), 1); // Only High TVL, CEX is filtered out
        assert_eq!(filtered[0].name, "High TVL");
    }

    #[test]
    fn test_sort_by_tvl() {
        let protocols = vec![
            Protocol {
                id: "1".to_string(),
                name: "Low".to_string(),
                slug: "low".to_string(),
                symbol: None,
                chains: vec![],
                tvl: Some(100.0),
                description: None,
                category: None,
                chain_tvls: HashMap::new(),
            },
            Protocol {
                id: "2".to_string(),
                name: "High".to_string(),
                slug: "high".to_string(),
                symbol: None,
                chains: vec![],
                tvl: Some(1000.0),
                description: None,
                category: None,
                chain_tvls: HashMap::new(),
            },
            Protocol {
                id: "3".to_string(),
                name: "None".to_string(),
                slug: "none".to_string(),
                symbol: None,
                chains: vec![],
                tvl: None,
                description: None,
                category: None,
                chain_tvls: HashMap::new(),
            },
        ];

        let sorted = DefiLlamaClient::sort_by_tvl(protocols);
        assert_eq!(sorted[0].name, "High");
        assert_eq!(sorted[1].name, "Low");
        assert_eq!(sorted[2].name, "None"); // None values should be at the end
    }
}
