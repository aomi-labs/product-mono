//! DeFi client using DeFiLlama APIs (free, no API key required)

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

const DEFILLAMA_API: &str = "https://api.llama.fi";
const DEFILLAMA_COINS_API: &str = "https://coins.llama.fi";
const DEFILLAMA_YIELDS_API: &str = "https://yields.llama.fi";

#[derive(Clone)]
pub struct DefiClient {
    http: Client,
}

impl DefiClient {
    pub fn new() -> eyre::Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { http })
    }

    // =========================================================================
    // Token Prices (via DeFiLlama Coins API)
    // =========================================================================

    pub async fn get_token_price(&self, token: &str) -> eyre::Result<TokenPrice> {
        // DeFiLlama uses format: chain:address or coingecko:symbol
        let coin_id = normalize_token_id(token);
        let url = format!("{}/prices/current/{}", DEFILLAMA_COINS_API, coin_id);
        
        let response: CoinsResponse = self.http.get(&url).send().await?.json().await?;
        
        let coin = response.coins.values().next()
            .ok_or_else(|| eyre::eyre!("Token not found: {}", token))?;
        
        Ok(TokenPrice {
            symbol: coin.symbol.clone(),
            price: coin.price,
            confidence: coin.confidence,
            timestamp: coin.timestamp,
        })
    }

    pub async fn get_multiple_prices(&self, tokens: &[&str]) -> eyre::Result<Vec<TokenPrice>> {
        let coin_ids: Vec<String> = tokens.iter().map(|t| normalize_token_id(t)).collect();
        let url = format!("{}/prices/current/{}", DEFILLAMA_COINS_API, coin_ids.join(","));
        
        let response: CoinsResponse = self.http.get(&url).send().await?.json().await?;
        
        Ok(response.coins.values().map(|coin| TokenPrice {
            symbol: coin.symbol.clone(),
            price: coin.price,
            confidence: coin.confidence,
            timestamp: coin.timestamp,
        }).collect())
    }

    // =========================================================================
    // Yield Opportunities (via DeFiLlama Yields API)
    // =========================================================================

    pub async fn get_yield_pools(&self, chain: Option<&str>, project: Option<&str>) -> eyre::Result<Vec<YieldPool>> {
        let url = format!("{}/pools", DEFILLAMA_YIELDS_API);
        let response: YieldsResponse = self.http.get(&url).send().await?.json().await?;
        
        let mut pools: Vec<YieldPool> = response.data.into_iter()
            .filter(|p| {
                let chain_match = chain.map(|c| p.chain.to_lowercase() == c.to_lowercase()).unwrap_or(true);
                let project_match = project.map(|pr| p.project.to_lowercase().contains(&pr.to_lowercase())).unwrap_or(true);
                chain_match && project_match && p.apy.unwrap_or(0.0) > 0.0
            })
            .map(|p| YieldPool {
                pool: p.pool,
                chain: p.chain,
                project: p.project,
                symbol: p.symbol,
                tvl_usd: p.tvl_usd,
                apy: p.apy,
                apy_base: p.apy_base,
                apy_reward: p.apy_reward,
                stablecoin: p.stablecoin,
                il_risk: p.il_risk,
            })
            .collect();
        
        // Sort by APY descending
        pools.sort_by(|a, b| b.apy.partial_cmp(&a.apy).unwrap_or(std::cmp::Ordering::Equal));
        
        Ok(pools)
    }

    // =========================================================================
    // Gas Prices (via public RPC estimation)
    // =========================================================================

    pub async fn get_gas_prices(&self) -> eyre::Result<Vec<GasPrice>> {
        // Using blocknative or etherscan-style estimates
        // For now, return estimates based on typical ranges
        Ok(vec![
            GasPrice { chain: "ethereum".to_string(), gas_gwei: 25.0, usd_for_swap: 8.0 },
            GasPrice { chain: "arbitrum".to_string(), gas_gwei: 0.1, usd_for_swap: 0.30 },
            GasPrice { chain: "optimism".to_string(), gas_gwei: 0.001, usd_for_swap: 0.20 },
            GasPrice { chain: "polygon".to_string(), gas_gwei: 50.0, usd_for_swap: 0.05 },
            GasPrice { chain: "base".to_string(), gas_gwei: 0.001, usd_for_swap: 0.15 },
            GasPrice { chain: "bsc".to_string(), gas_gwei: 3.0, usd_for_swap: 0.10 },
        ])
    }

    // =========================================================================
    // Protocols TVL
    // =========================================================================

    pub async fn get_protocols(&self, category: Option<&str>) -> eyre::Result<Vec<Protocol>> {
        let url = format!("{}/protocols", DEFILLAMA_API);
        let protocols: Vec<ProtocolResponse> = self.http.get(&url).send().await?.json().await?;
        
        let mut filtered: Vec<Protocol> = protocols.into_iter()
            .filter(|p| {
                category.map(|c| p.category.as_ref().map(|pc| pc.to_lowercase().contains(&c.to_lowercase())).unwrap_or(false)).unwrap_or(true)
            })
            .take(50)
            .map(|p| Protocol {
                name: p.name,
                slug: p.slug,
                tvl: p.tvl,
                chain: p.chain,
                chains: p.chains,
                category: p.category,
                change_1d: p.change_1d,
                change_7d: p.change_7d,
            })
            .collect();
        
        filtered.sort_by(|a, b| b.tvl.partial_cmp(&a.tvl).unwrap_or(std::cmp::Ordering::Equal));
        Ok(filtered)
    }

    // =========================================================================
    // Chain TVL
    // =========================================================================

    pub async fn get_chains_tvl(&self) -> eyre::Result<Vec<ChainTvl>> {
        let url = format!("{}/v2/chains", DEFILLAMA_API);
        let chains: Vec<ChainTvlResponse> = self.http.get(&url).send().await?.json().await?;
        
        let mut result: Vec<ChainTvl> = chains.into_iter()
            .map(|c| ChainTvl {
                name: c.name,
                tvl: c.tvl,
                token_symbol: c.token_symbol,
            })
            .collect();
        
        result.sort_by(|a, b| b.tvl.partial_cmp(&a.tvl).unwrap_or(std::cmp::Ordering::Equal));
        Ok(result)
    }

    // =========================================================================
    // Bridges
    // =========================================================================

    pub async fn get_bridges(&self) -> eyre::Result<Vec<Bridge>> {
        let url = format!("{}/bridges", DEFILLAMA_API);
        let response: BridgesResponse = self.http.get(&url).send().await?.json().await?;
        
        let mut bridges: Vec<Bridge> = response.bridges.into_iter()
            .map(|b| Bridge {
                name: b.display_name.unwrap_or(b.name),
                volume_24h: b.last_daily_volume,
                volume_7d: b.weekly_volume,
                chains: b.chains,
            })
            .collect();
        
        bridges.sort_by(|a, b| b.volume_24h.partial_cmp(&a.volume_24h).unwrap_or(std::cmp::Ordering::Equal));
        Ok(bridges)
    }

    // =========================================================================
    // Swap Quote (placeholder - would need 0x or 1inch API key)
    // =========================================================================

    pub async fn get_swap_quote(
        &self,
        _chain: &str,
        _from_token: &str,
        _to_token: &str,
        _amount: f64,
    ) -> eyre::Result<SwapQuote> {
        // This is a placeholder - real implementation would use 0x or 1inch API
        Ok(SwapQuote {
            from_token: "ETH".to_string(),
            to_token: "USDC".to_string(),
            from_amount: 1.0,
            to_amount: 2500.0,
            price_impact: 0.05,
            gas_estimate_usd: 5.0,
            sources: vec!["Uniswap V3".to_string(), "Curve".to_string()],
        })
    }
}

// Helper to normalize token identifiers for DeFiLlama
fn normalize_token_id(token: &str) -> String {
    let token_lower = token.to_lowercase();
    match token_lower.as_str() {
        "eth" | "ethereum" => "coingecko:ethereum".to_string(),
        "btc" | "bitcoin" => "coingecko:bitcoin".to_string(),
        "usdc" => "coingecko:usd-coin".to_string(),
        "usdt" | "tether" => "coingecko:tether".to_string(),
        "dai" => "coingecko:dai".to_string(),
        "sol" | "solana" => "coingecko:solana".to_string(),
        "bnb" => "coingecko:binancecoin".to_string(),
        "avax" | "avalanche" => "coingecko:avalanche-2".to_string(),
        "matic" | "polygon" => "coingecko:matic-network".to_string(),
        "arb" | "arbitrum" => "coingecko:arbitrum".to_string(),
        "op" | "optimism" => "coingecko:optimism".to_string(),
        "uni" | "uniswap" => "coingecko:uniswap".to_string(),
        "aave" => "coingecko:aave".to_string(),
        "link" | "chainlink" => "coingecko:chainlink".to_string(),
        "mkr" | "maker" => "coingecko:maker".to_string(),
        "crv" | "curve" => "coingecko:curve-dao-token".to_string(),
        "ldo" | "lido" => "coingecko:lido-dao".to_string(),
        _ => format!("coingecko:{}", token_lower),
    }
}

// =============================================================================
// Response Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrice {
    pub symbol: String,
    pub price: f64,
    pub confidence: Option<f64>,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldPool {
    pub pool: String,
    pub chain: String,
    pub project: String,
    pub symbol: String,
    pub tvl_usd: Option<f64>,
    pub apy: Option<f64>,
    pub apy_base: Option<f64>,
    pub apy_reward: Option<f64>,
    pub stablecoin: Option<bool>,
    pub il_risk: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasPrice {
    pub chain: String,
    pub gas_gwei: f64,
    pub usd_for_swap: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapQuote {
    pub from_token: String,
    pub to_token: String,
    pub from_amount: f64,
    pub to_amount: f64,
    pub price_impact: f64,
    pub gas_estimate_usd: f64,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Protocol {
    pub name: String,
    pub slug: String,
    pub tvl: f64,
    pub chain: Option<String>,
    pub chains: Option<Vec<String>>,
    pub category: Option<String>,
    pub change_1d: Option<f64>,
    pub change_7d: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainTvl {
    pub name: String,
    pub tvl: f64,
    pub token_symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bridge {
    pub name: String,
    pub volume_24h: Option<f64>,
    pub volume_7d: Option<f64>,
    pub chains: Option<Vec<String>>,
}

// =============================================================================
// API Response Types
// =============================================================================

#[derive(Debug, Deserialize)]
struct CoinsResponse {
    coins: HashMap<String, CoinData>,
}

#[derive(Debug, Deserialize)]
struct CoinData {
    symbol: String,
    price: f64,
    confidence: Option<f64>,
    timestamp: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct YieldsResponse {
    data: Vec<YieldPoolResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YieldPoolResponse {
    pool: String,
    chain: String,
    project: String,
    symbol: String,
    tvl_usd: Option<f64>,
    apy: Option<f64>,
    apy_base: Option<f64>,
    apy_reward: Option<f64>,
    stablecoin: Option<bool>,
    il_risk: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProtocolResponse {
    name: String,
    slug: String,
    tvl: f64,
    chain: Option<String>,
    chains: Option<Vec<String>>,
    category: Option<String>,
    change_1d: Option<f64>,
    change_7d: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChainTvlResponse {
    name: String,
    tvl: f64,
    token_symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BridgesResponse {
    bridges: Vec<BridgeResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeResponse {
    name: String,
    display_name: Option<String>,
    last_daily_volume: Option<f64>,
    weekly_volume: Option<f64>,
    chains: Option<Vec<String>>,
}
