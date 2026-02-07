//! Prediction Wizard tools - Aggregated prediction market access
//!
//! Tools:
//! - search_prediction_markets: Search across all platforms
//! - get_market_details: Get details for a specific market
//! - get_aggregated_odds: Compare probabilities across platforms
//! - get_trending_predictions: Get hot/trending markets

#![allow(clippy::manual_async_fn)]

use crate::client::{Platform, PredictionClient};
use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, WithTopic};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::LazyLock;
use tokio::sync::Mutex;

// Global client instance
static PREDICTION_CLIENT: LazyLock<Mutex<Option<PredictionClient>>> =
    LazyLock::new(|| Mutex::new(None));

async fn get_client() -> eyre::Result<PredictionClient> {
    let mut guard = PREDICTION_CLIENT.lock().await;
    if let Some(client) = guard.clone() {
        return Ok(client);
    }

    let client = PredictionClient::new()?;
    *guard = Some(client.clone());
    Ok(client)
}

// ============================================================================
// Tool 1: Search Prediction Markets
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchPredictionMarketsArgs {
    /// Search query (e.g., "Trump 2028", "Bitcoin 100k", "AI AGI")
    pub query: String,
    /// Filter by category (optional): politics, crypto, sports, economics, ai, science, entertainment
    pub category: Option<String>,
    /// Platforms to search (optional, default: all). Options: polymarket, kalshi, manifold, metaculus
    pub platforms: Option<Vec<String>>,
    /// Maximum results per platform (default: 5, max: 20)
    pub limit: Option<u32>,
}

impl AomiToolArgs for SearchPredictionMarketsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query (e.g., 'Trump 2028', 'Bitcoin price', 'AI AGI')"
                },
                "category": {
                    "type": "string",
                    "enum": ["politics", "crypto", "sports", "economics", "ai", "science", "entertainment"],
                    "description": "Filter by category (optional)"
                },
                "platforms": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "enum": ["polymarket", "kalshi", "manifold", "metaculus"]
                    },
                    "description": "Platforms to search (default: all)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results per platform (default: 5, max: 20)"
                }
            },
            "required": ["query"]
        })
    }
}

pub type SearchPredictionMarketsParameters = WithTopic<SearchPredictionMarketsArgs>;

#[derive(Debug, Clone)]
pub struct SearchPredictionMarkets;

impl AomiTool for SearchPredictionMarkets {
    const NAME: &'static str = "search_prediction_markets";
    const NAMESPACE: &'static str = "prediction";

    type Args = SearchPredictionMarketsParameters;
    type Output = serde_json::Value;
    type Error = PredictionToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Search prediction markets across Polymarket, Kalshi, Manifold, and Metaculus. Returns unified results with current probabilities, volumes, and links."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;

            let platforms: Option<Vec<Platform>> = args.inner.platforms.map(|ps| {
                ps.iter()
                    .filter_map(|p| Platform::from_str(p))
                    .collect()
            });

            let limit = args.inner.limit.unwrap_or(5).min(20);
            let markets = client.search_all(&args.inner.query, platforms, limit).await?;

            // Group by platform for easier reading
            let mut by_platform: std::collections::HashMap<String, Vec<serde_json::Value>> =
                std::collections::HashMap::new();

            for market in &markets {
                let platform_name = market.platform.as_str().to_string();
                let entry = by_platform.entry(platform_name).or_default();
                entry.push(json!({
                    "id": market.id,
                    "question": market.question,
                    "probability": market.probability.map(|p| format!("{:.1}%", p * 100.0)),
                    "volume": market.volume.map(|v| format!("${:.0}", v)),
                    "close_time": market.close_time,
                    "url": market.url,
                    "resolved": market.resolved,
                }));
            }

            Ok(json!({
                "query": args.inner.query,
                "total_results": markets.len(),
                "results_by_platform": by_platform,
            }))
        }
    }
}

// ============================================================================
// Tool 2: Get Market Details
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetMarketDetailsArgs {
    /// Market ID or identifier
    pub market_id: String,
    /// Platform the market is on: polymarket, kalshi, manifold, metaculus
    pub platform: String,
}

impl AomiToolArgs for GetMarketDetailsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "market_id": {
                    "type": "string",
                    "description": "Market ID or identifier"
                },
                "platform": {
                    "type": "string",
                    "enum": ["polymarket", "kalshi", "manifold", "metaculus"],
                    "description": "Platform the market is on"
                }
            },
            "required": ["market_id", "platform"]
        })
    }
}

pub type GetMarketDetailsParameters = WithTopic<GetMarketDetailsArgs>;

#[derive(Debug, Clone)]
pub struct GetMarketDetails;

impl AomiTool for GetMarketDetails {
    const NAME: &'static str = "get_prediction_market_details";
    const NAMESPACE: &'static str = "prediction";

    type Args = GetMarketDetailsParameters;
    type Output = serde_json::Value;
    type Error = PredictionToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get detailed information about a specific prediction market including description, resolution criteria, current prices, and trading links."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let platform = Platform::from_str(&args.inner.platform)
                .ok_or_else(|| eyre::eyre!("Invalid platform: {}", args.inner.platform))?;

            let market = match platform {
                Platform::Polymarket => {
                    client.polymarket_get_market(&args.inner.market_id).await?
                }
                Platform::Kalshi => {
                    client.kalshi_get_market(&args.inner.market_id).await?
                }
                Platform::Manifold => {
                    client.manifold_get_market(&args.inner.market_id).await?
                }
                Platform::Metaculus => {
                    // Metaculus doesn't have individual market fetch in our client yet
                    return Err(eyre::eyre!("Metaculus individual market fetch not implemented"));
                }
            };

            Ok(json!({
                "id": market.id,
                "platform": market.platform.as_str(),
                "question": market.question,
                "description": market.description,
                "probability": market.probability.map(|p| format!("{:.1}%", p * 100.0)),
                "outcomes": market.outcomes,
                "outcome_prices": market.outcome_prices,
                "volume": market.volume.map(|v| format!("${:.0}", v)),
                "liquidity": market.liquidity.map(|l| format!("${:.0}", l)),
                "close_time": market.close_time,
                "url": market.url,
                "category": market.category,
                "resolved": market.resolved,
                "resolution": market.resolution,
            }))
        }
    }
}

// ============================================================================
// Tool 3: Get Aggregated Odds
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetAggregatedOddsArgs {
    /// The prediction question to search for (will fuzzy match across platforms)
    pub query: String,
}

impl AomiToolArgs for GetAggregatedOddsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The prediction question (e.g., 'Trump wins 2028', 'Bitcoin 100k by end of year')"
                }
            },
            "required": ["query"]
        })
    }
}

pub type GetAggregatedOddsParameters = WithTopic<GetAggregatedOddsArgs>;

#[derive(Debug, Clone)]
pub struct GetAggregatedOdds;

impl AomiTool for GetAggregatedOdds {
    const NAME: &'static str = "get_aggregated_odds";
    const NAMESPACE: &'static str = "prediction";

    type Args = GetAggregatedOddsParameters;
    type Output = serde_json::Value;
    type Error = PredictionToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get probability consensus across multiple prediction platforms for the same question. Shows spread between platforms and identifies arbitrage opportunities."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let odds = client.get_aggregated_odds(&args.inner.query).await?;

            let mut platform_odds = serde_json::Map::new();
            if let Some(p) = odds.polymarket {
                platform_odds.insert("polymarket".to_string(), json!(format!("{:.1}%", p * 100.0)));
            }
            if let Some(p) = odds.kalshi {
                platform_odds.insert("kalshi".to_string(), json!(format!("{:.1}%", p * 100.0)));
            }
            if let Some(p) = odds.manifold {
                platform_odds.insert("manifold".to_string(), json!(format!("{:.1}%", p * 100.0)));
            }
            if let Some(p) = odds.metaculus {
                platform_odds.insert("metaculus".to_string(), json!(format!("{:.1}%", p * 100.0)));
            }

            let arbitrage_opportunity = odds.spread.map(|s| s > 0.10).unwrap_or(false);

            Ok(json!({
                "query": odds.query,
                "platform_odds": platform_odds,
                "consensus": odds.consensus.map(|c| format!("{:.1}%", c * 100.0)),
                "spread": odds.spread.map(|s| format!("{:.1}%", s * 100.0)),
                "arbitrage_opportunity": arbitrage_opportunity,
                "markets_found": odds.markets.len(),
                "markets": odds.markets.iter().map(|m| json!({
                    "platform": m.platform,
                    "question": m.question,
                    "probability": format!("{:.1}%", m.probability * 100.0),
                    "url": m.url,
                })).collect::<Vec<_>>(),
            }))
        }
    }
}

// ============================================================================
// Tool 4: Get Trending Predictions
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetTrendingPredictionsArgs {
    /// Filter by category (optional)
    pub category: Option<String>,
    /// Sort by: volume_24h, new, closing_soon (default: volume_24h)
    pub sort_by: Option<String>,
    /// Maximum results (default: 10, max: 50)
    pub limit: Option<u32>,
}

impl AomiToolArgs for GetTrendingPredictionsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "description": "Filter by category (optional)"
                },
                "sort_by": {
                    "type": "string",
                    "enum": ["volume_24h", "new", "closing_soon"],
                    "description": "Sort order (default: volume_24h)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results (default: 10, max: 50)"
                }
            },
            "required": []
        })
    }
}

pub type GetTrendingPredictionsParameters = WithTopic<GetTrendingPredictionsArgs>;

#[derive(Debug, Clone)]
pub struct GetTrendingPredictions;

impl AomiTool for GetTrendingPredictions {
    const NAME: &'static str = "get_trending_predictions";
    const NAMESPACE: &'static str = "prediction";

    type Args = GetTrendingPredictionsParameters;
    type Output = serde_json::Value;
    type Error = PredictionToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get trending and hot prediction markets with highest recent activity. Great for discovering what people are betting on."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let limit = args.inner.limit.unwrap_or(10).min(50);

            // Search for general high-activity topics across platforms
            // We use common trending queries
            let queries = match args.inner.category.as_deref() {
                Some("politics") => vec!["election", "president", "congress"],
                Some("crypto") => vec!["bitcoin", "ethereum", "crypto"],
                Some("ai") => vec!["AI", "GPT", "AGI"],
                Some("sports") => vec!["super bowl", "world cup", "championship"],
                _ => vec!["2026", "president", "bitcoin", "AI"],
            };

            let mut all_markets = Vec::new();
            for query in queries {
                if let Ok(markets) = client.search_all(query, None, 5).await {
                    all_markets.extend(markets);
                }
            }

            // Sort by volume (descending) and dedupe
            all_markets.sort_by(|a, b| {
                b.volume
                    .unwrap_or(0.0)
                    .partial_cmp(&a.volume.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Dedupe by question (rough similarity)
            let mut seen_questions = std::collections::HashSet::new();
            let deduped: Vec<_> = all_markets
                .into_iter()
                .filter(|m| {
                    let key = m.question.to_lowercase().chars().take(50).collect::<String>();
                    seen_questions.insert(key)
                })
                .take(limit as usize)
                .collect();

            Ok(json!({
                "trending_count": deduped.len(),
                "markets": deduped.iter().map(|m| json!({
                    "platform": m.platform.as_str(),
                    "question": m.question,
                    "probability": m.probability.map(|p| format!("{:.1}%", p * 100.0)),
                    "volume": m.volume.map(|v| format!("${:.0}", v)),
                    "close_time": m.close_time,
                    "url": m.url,
                })).collect::<Vec<_>>(),
            }))
        }
    }
}

// ============================================================================
// Error Type
// ============================================================================

#[derive(Debug)]
pub struct PredictionToolError(String);

impl std::fmt::Display for PredictionToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PredictionToolError: {}", self.0)
    }
}

impl std::error::Error for PredictionToolError {}
