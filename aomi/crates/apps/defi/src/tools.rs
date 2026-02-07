//! DeFi Master tools using DeFiLlama APIs

#![allow(clippy::manual_async_fn)]

use crate::client::DefiClient;
use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, WithTopic};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::LazyLock;
use tokio::sync::Mutex;

static DEFI_CLIENT: LazyLock<Mutex<Option<DefiClient>>> = LazyLock::new(|| Mutex::new(None));

async fn get_client() -> eyre::Result<DefiClient> {
    let mut guard = DEFI_CLIENT.lock().await;
    if let Some(client) = guard.clone() {
        return Ok(client);
    }
    let client = DefiClient::new()?;
    *guard = Some(client.clone());
    Ok(client)
}

// ============================================================================
// Tool 1: Get Token Price
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetTokenPriceArgs {
    /// Token symbol or name (e.g., "ETH", "bitcoin", "USDC")
    pub token: String,
}

impl AomiToolArgs for GetTokenPriceArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "token": {
                    "type": "string",
                    "description": "Token symbol or name (ETH, BTC, USDC, etc.)"
                }
            },
            "required": ["token"]
        })
    }
}

pub type GetTokenPriceParameters = WithTopic<GetTokenPriceArgs>;

#[derive(Debug, Clone)]
pub struct GetTokenPrice;

impl AomiTool for GetTokenPrice {
    const NAME: &'static str = "get_token_price";
    const NAMESPACE: &'static str = "defi";

    type Args = GetTokenPriceParameters;
    type Output = serde_json::Value;
    type Error = DefiToolError;

    fn support_async(&self) -> bool { false }

    fn description(&self) -> &'static str {
        "Get the current price of any cryptocurrency token."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let price = client.get_token_price(&args.inner.token).await?;
            Ok(json!({
                "symbol": price.symbol,
                "price_usd": format!("${:.2}", price.price),
                "confidence": price.confidence,
            }))
        }
    }
}

// ============================================================================
// Tool 2: Get Yield Opportunities
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetYieldOpportunitiesArgs {
    /// Filter by chain (optional): ethereum, arbitrum, optimism, polygon, base, bsc, solana
    pub chain: Option<String>,
    /// Filter by project name (optional): aave, compound, lido, etc.
    pub project: Option<String>,
    /// Only show stablecoin pools
    pub stablecoin_only: Option<bool>,
    /// Maximum results (default: 20)
    pub limit: Option<u32>,
}

impl AomiToolArgs for GetYieldOpportunitiesArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "chain": { "type": "string", "description": "Filter by chain: ethereum, arbitrum, polygon, etc." },
                "project": { "type": "string", "description": "Filter by project: aave, lido, compound, etc." },
                "stablecoin_only": { "type": "boolean", "description": "Only show stablecoin pools" },
                "limit": { "type": "integer", "description": "Max results (default: 20)" }
            },
            "required": []
        })
    }
}

pub type GetYieldOpportunitiesParameters = WithTopic<GetYieldOpportunitiesArgs>;

#[derive(Debug, Clone)]
pub struct GetYieldOpportunities;

impl AomiTool for GetYieldOpportunities {
    const NAME: &'static str = "get_yield_opportunities";
    const NAMESPACE: &'static str = "defi";

    type Args = GetYieldOpportunitiesParameters;
    type Output = serde_json::Value;
    type Error = DefiToolError;

    fn support_async(&self) -> bool { false }

    fn description(&self) -> &'static str {
        "Find yield farming and staking opportunities across DeFi protocols. Returns pools sorted by APY."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let mut pools = client.get_yield_pools(
                args.inner.chain.as_deref(),
                args.inner.project.as_deref(),
            ).await?;

            if args.inner.stablecoin_only.unwrap_or(false) {
                pools.retain(|p| p.stablecoin.unwrap_or(false));
            }

            let limit = args.inner.limit.unwrap_or(20) as usize;
            pools.truncate(limit);

            let formatted: Vec<_> = pools.iter().map(|p| json!({
                "pool": p.symbol,
                "project": p.project,
                "chain": p.chain,
                "apy": format!("{:.2}%", p.apy.unwrap_or(0.0)),
                "tvl": p.tvl_usd.map(|t| format!("${:.0}M", t / 1_000_000.0)),
                "stablecoin": p.stablecoin,
                "il_risk": p.il_risk,
            })).collect();

            Ok(json!({
                "pools_found": formatted.len(),
                "pools": formatted,
            }))
        }
    }
}

// ============================================================================
// Tool 3: Get Gas Prices
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetGasPricesArgs {
    /// Specific chain or "all" for comparison (default: all)
    pub chain: Option<String>,
}

impl AomiToolArgs for GetGasPricesArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "chain": { "type": "string", "description": "Chain name or 'all' for comparison" }
            },
            "required": []
        })
    }
}

pub type GetGasPricesParameters = WithTopic<GetGasPricesArgs>;

#[derive(Debug, Clone)]
pub struct GetGasPrices;

impl AomiTool for GetGasPrices {
    const NAME: &'static str = "get_gas_prices";
    const NAMESPACE: &'static str = "defi";

    type Args = GetGasPricesParameters;
    type Output = serde_json::Value;
    type Error = DefiToolError;

    fn support_async(&self) -> bool { false }

    fn description(&self) -> &'static str {
        "Get current gas prices across different blockchain networks."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let mut prices = client.get_gas_prices().await?;

            if let Some(chain) = &args.inner.chain {
                if chain.to_lowercase() != "all" {
                    prices.retain(|p| p.chain.to_lowercase() == chain.to_lowercase());
                }
            }

            let formatted: Vec<_> = prices.iter().map(|p| json!({
                "chain": p.chain,
                "gas_gwei": p.gas_gwei,
                "swap_cost_usd": format!("${:.2}", p.usd_for_swap),
            })).collect();

            Ok(json!({
                "gas_prices": formatted,
                "note": "Costs estimated for a typical swap transaction"
            }))
        }
    }
}

// ============================================================================
// Tool 4: Get Swap Quote
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetSwapQuoteArgs {
    /// Chain to swap on
    pub chain: String,
    /// Token to swap from
    pub from_token: String,
    /// Token to swap to
    pub to_token: String,
    /// Amount to swap
    pub amount: f64,
}

impl AomiToolArgs for GetSwapQuoteArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "chain": { "type": "string", "description": "Chain: ethereum, arbitrum, polygon, etc." },
                "from_token": { "type": "string", "description": "Token to swap from (ETH, USDC, etc.)" },
                "to_token": { "type": "string", "description": "Token to swap to" },
                "amount": { "type": "number", "description": "Amount to swap" }
            },
            "required": ["chain", "from_token", "to_token", "amount"]
        })
    }
}

pub type GetSwapQuoteParameters = WithTopic<GetSwapQuoteArgs>;

#[derive(Debug, Clone)]
pub struct GetSwapQuote;

impl AomiTool for GetSwapQuote {
    const NAME: &'static str = "get_swap_quote";
    const NAMESPACE: &'static str = "defi";

    type Args = GetSwapQuoteParameters;
    type Output = serde_json::Value;
    type Error = DefiToolError;

    fn support_async(&self) -> bool { false }

    fn description(&self) -> &'static str {
        "Get a swap quote from DEX aggregators. Shows best rate across decentralized exchanges."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let quote = client.get_swap_quote(
                &args.inner.chain,
                &args.inner.from_token,
                &args.inner.to_token,
                args.inner.amount,
            ).await?;

            Ok(json!({
                "from": format!("{} {}", args.inner.amount, quote.from_token),
                "to": format!("{:.4} {}", quote.to_amount, quote.to_token),
                "rate": format!("1 {} = {:.4} {}", quote.from_token, quote.to_amount / quote.from_amount, quote.to_token),
                "price_impact": format!("{:.2}%", quote.price_impact),
                "gas_estimate": format!("${:.2}", quote.gas_estimate_usd),
                "sources": quote.sources,
                "note": "Swap quote is indicative. Actual rates may vary."
            }))
        }
    }
}

// ============================================================================
// Tool 5: Get Protocols
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetProtocolsArgs {
    /// Filter by category: dexes, lending, yield, liquid-staking, bridge, derivatives
    pub category: Option<String>,
    /// Maximum results (default: 20)
    pub limit: Option<u32>,
}

impl AomiToolArgs for GetProtocolsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "category": { "type": "string", "description": "Category: dexes, lending, yield, liquid-staking, bridge" },
                "limit": { "type": "integer", "description": "Max results (default: 20)" }
            },
            "required": []
        })
    }
}

pub type GetProtocolsParameters = WithTopic<GetProtocolsArgs>;

#[derive(Debug, Clone)]
pub struct GetProtocols;

impl AomiTool for GetProtocols {
    const NAME: &'static str = "get_defi_protocols";
    const NAMESPACE: &'static str = "defi";

    type Args = GetProtocolsParameters;
    type Output = serde_json::Value;
    type Error = DefiToolError;

    fn support_async(&self) -> bool { false }

    fn description(&self) -> &'static str {
        "Get top DeFi protocols by TVL (Total Value Locked). Filter by category."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let mut protocols = client.get_protocols(args.inner.category.as_deref()).await?;
            
            let limit = args.inner.limit.unwrap_or(20) as usize;
            protocols.truncate(limit);

            let formatted: Vec<_> = protocols.iter().map(|p| json!({
                "name": p.name,
                "tvl": format!("${:.2}B", p.tvl / 1_000_000_000.0),
                "category": p.category,
                "chains": p.chains,
                "change_1d": p.change_1d.map(|c| format!("{:+.1}%", c)),
            })).collect();

            Ok(json!({
                "protocols_count": formatted.len(),
                "protocols": formatted,
            }))
        }
    }
}

// ============================================================================
// Tool 6: Get Chain TVL
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetChainTvlArgs {
    /// Maximum results (default: 15)
    pub limit: Option<u32>,
}

impl AomiToolArgs for GetChainTvlArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "description": "Max results (default: 15)" }
            },
            "required": []
        })
    }
}

pub type GetChainTvlParameters = WithTopic<GetChainTvlArgs>;

#[derive(Debug, Clone)]
pub struct GetChainTvl;

impl AomiTool for GetChainTvl {
    const NAME: &'static str = "get_chain_tvl";
    const NAMESPACE: &'static str = "defi";

    type Args = GetChainTvlParameters;
    type Output = serde_json::Value;
    type Error = DefiToolError;

    fn support_async(&self) -> bool { false }

    fn description(&self) -> &'static str {
        "Get TVL (Total Value Locked) rankings for blockchain networks."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let mut chains = client.get_chains_tvl().await?;
            
            let limit = args.inner.limit.unwrap_or(15) as usize;
            chains.truncate(limit);

            let formatted: Vec<_> = chains.iter().enumerate().map(|(i, c)| json!({
                "rank": i + 1,
                "chain": c.name,
                "tvl": format!("${:.2}B", c.tvl / 1_000_000_000.0),
                "native_token": c.token_symbol,
            })).collect();

            Ok(json!({
                "chains": formatted,
            }))
        }
    }
}

// ============================================================================
// Tool 7: Get Bridges
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetBridgesArgs {
    /// Maximum results (default: 10)
    pub limit: Option<u32>,
}

impl AomiToolArgs for GetBridgesArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "description": "Max results (default: 10)" }
            },
            "required": []
        })
    }
}

pub type GetBridgesParameters = WithTopic<GetBridgesArgs>;

#[derive(Debug, Clone)]
pub struct GetBridges;

impl AomiTool for GetBridges {
    const NAME: &'static str = "get_bridges";
    const NAMESPACE: &'static str = "defi";

    type Args = GetBridgesParameters;
    type Output = serde_json::Value;
    type Error = DefiToolError;

    fn support_async(&self) -> bool { false }

    fn description(&self) -> &'static str {
        "Get popular cross-chain bridges by volume. Useful for moving assets between chains."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let mut bridges = client.get_bridges().await?;
            
            let limit = args.inner.limit.unwrap_or(10) as usize;
            bridges.truncate(limit);

            let formatted: Vec<_> = bridges.iter().map(|b| json!({
                "name": b.name,
                "volume_24h": b.volume_24h.map(|v| format!("${:.1}M", v / 1_000_000.0)),
                "volume_7d": b.volume_7d.map(|v| format!("${:.1}M", v / 1_000_000.0)),
                "chains": b.chains,
            })).collect();

            Ok(json!({
                "bridges": formatted,
                "warning": "Always verify bridge security before transferring large amounts"
            }))
        }
    }
}

// ============================================================================
// Error Type
// ============================================================================

#[derive(Debug)]
pub struct DefiToolError(String);

impl std::fmt::Display for DefiToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DefiToolError: {}", self.0)
    }
}

impl std::error::Error for DefiToolError {}
