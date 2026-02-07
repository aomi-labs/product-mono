//! Unified prediction market client supporting multiple platforms.
//!
//! Supported platforms:
//! - Polymarket (crypto-native, Polygon)
//! - Kalshi (CFTC-regulated, US)
//! - Manifold (community, play money + prizes)
//! - Metaculus (forecasting, long-term predictions)

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const POLYMARKET_GAMMA_URL: &str = "https://gamma-api.polymarket.com";
const POLYMARKET_CLOB_URL: &str = "https://clob.polymarket.com";
const KALSHI_API_URL: &str = "https://api.elections.kalshi.com/trade-api/v2";
const MANIFOLD_API_URL: &str = "https://api.manifold.markets/v0";
const METACULUS_API_URL: &str = "https://www.metaculus.com/api2";

/// Unified prediction market client
#[derive(Clone)]
pub struct PredictionClient {
    http: Client,
}

impl PredictionClient {
    pub fn new() -> eyre::Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { http })
    }

    // =========================================================================
    // POLYMARKET
    // =========================================================================

    pub async fn polymarket_search(&self, query: &str, limit: u32) -> eyre::Result<Vec<UnifiedMarket>> {
        let url = format!(
            "{}/events?title_like={}&limit={}&active=true",
            POLYMARKET_GAMMA_URL,
            urlencoding::encode(query),
            limit
        );

        let response: Vec<PolymarketEvent> = self.http.get(&url).send().await?.json().await?;

        Ok(response
            .into_iter()
            .flat_map(|event| {
                event.markets.into_iter().map(move |market| {
                    let outcome_prices = parse_json_string_or_array(&market.outcome_prices);
                    let outcomes = parse_json_string_or_array(&market.outcomes);
                    UnifiedMarket {
                        id: market.condition_id.clone(),
                        platform: Platform::Polymarket,
                        question: market.question.clone(),
                        description: market.description.clone(),
                        probability: outcome_prices.first().and_then(|p| p.parse::<f64>().ok()),
                        volume: market.volume_num,
                        liquidity: market.liquidity_num,
                        close_time: market.end_date_iso.clone(),
                        url: format!("https://polymarket.com/event/{}", event.slug),
                        outcomes,
                        outcome_prices,
                        category: event.category.clone(),
                        resolved: market.closed,
                        resolution: None,
                    }
                })
            })
            .collect())
    }

    pub async fn polymarket_get_market(&self, condition_id: &str) -> eyre::Result<UnifiedMarket> {
        let url = format!("{}/markets/{}", POLYMARKET_GAMMA_URL, condition_id);
        let market: PolymarketMarket = self.http.get(&url).send().await?.json().await?;

        let outcome_prices = parse_json_string_or_array(&market.outcome_prices);
        let outcomes = parse_json_string_or_array(&market.outcomes);

        Ok(UnifiedMarket {
            id: market.condition_id.clone(),
            platform: Platform::Polymarket,
            question: market.question.clone(),
            description: market.description.clone(),
            probability: outcome_prices.first().and_then(|p| p.parse::<f64>().ok()),
            volume: market.volume_num,
            liquidity: market.liquidity_num,
            close_time: market.end_date_iso.clone(),
            url: format!("https://polymarket.com/market/{}", market.condition_id),
            outcomes,
            outcome_prices,
            category: None,
            resolved: market.closed,
            resolution: None,
        })
    }

    // =========================================================================
    // KALSHI
    // =========================================================================

    pub async fn kalshi_search(&self, query: &str, limit: u32) -> eyre::Result<Vec<UnifiedMarket>> {
        // Kalshi doesn't have a search endpoint, so we fetch markets and filter
        let url = format!(
            "{}/markets?limit={}&status=active",
            KALSHI_API_URL, limit
        );

        let response: KalshiMarketsResponse = self.http.get(&url).send().await?.json().await?;

        let query_lower = query.to_lowercase();
        Ok(response
            .markets
            .into_iter()
            .filter(|m| m.title.to_lowercase().contains(&query_lower))
            .take(limit as usize)
            .map(|market| UnifiedMarket {
                id: market.ticker.clone(),
                platform: Platform::Kalshi,
                question: market.title.clone(),
                description: market.subtitle,
                probability: market.last_price.map(|p| p / 100.0),
                volume: market.volume.map(|v| v as f64),
                liquidity: None,
                close_time: market.close_time,
                url: format!("https://kalshi.com/markets/{}", market.ticker),
                outcomes: vec!["Yes".to_string(), "No".to_string()],
                outcome_prices: market.last_price.map(|p| vec![
                    format!("{:.2}", p / 100.0),
                    format!("{:.2}", 1.0 - p / 100.0),
                ]).unwrap_or_default(),
                category: market.category,
                resolved: market.status == Some("closed".to_string()),
                resolution: market.result,
            })
            .collect())
    }

    pub async fn kalshi_get_market(&self, ticker: &str) -> eyre::Result<UnifiedMarket> {
        let url = format!("{}/markets/{}", KALSHI_API_URL, ticker);
        let response: KalshiMarketResponse = self.http.get(&url).send().await?.json().await?;
        let market = response.market;

        Ok(UnifiedMarket {
            id: market.ticker.clone(),
            platform: Platform::Kalshi,
            question: market.title.clone(),
            description: market.subtitle,
            probability: market.last_price.map(|p| p / 100.0),
            volume: market.volume.map(|v| v as f64),
            liquidity: None,
            close_time: market.close_time,
            url: format!("https://kalshi.com/markets/{}", market.ticker),
            outcomes: vec!["Yes".to_string(), "No".to_string()],
            outcome_prices: market.last_price.map(|p| vec![
                format!("{:.2}", p / 100.0),
                format!("{:.2}", 1.0 - p / 100.0),
            ]).unwrap_or_default(),
            category: market.category,
            resolved: market.status == Some("closed".to_string()),
            resolution: market.result,
        })
    }

    // =========================================================================
    // MANIFOLD
    // =========================================================================

    pub async fn manifold_search(&self, query: &str, limit: u32) -> eyre::Result<Vec<UnifiedMarket>> {
        let url = format!(
            "{}/search-markets?term={}&limit={}&filter=open",
            MANIFOLD_API_URL,
            urlencoding::encode(query),
            limit
        );

        let markets: Vec<ManifoldMarket> = self.http.get(&url).send().await?.json().await?;

        Ok(markets
            .into_iter()
            .filter_map(|market| {
                // Only include binary markets with probability
                let prob = market.probability?;
                Some(UnifiedMarket {
                    id: market.id.clone(),
                    platform: Platform::Manifold,
                    question: market.question.clone(),
                    description: market.text_description,
                    probability: Some(prob),
                    volume: market.volume,
                    liquidity: market.total_liquidity,
                    close_time: market.close_time.map(|t| format!("{}", t)),
                    url: market.url.clone(),
                    outcomes: vec!["Yes".to_string(), "No".to_string()],
                    outcome_prices: vec![
                        format!("{:.2}", prob),
                        format!("{:.2}", 1.0 - prob),
                    ],
                    category: market.group_slugs.and_then(|s| s.first().cloned()),
                    resolved: market.is_resolved,
                    resolution: market.resolution,
                })
            })
            .collect())
    }

    pub async fn manifold_get_market(&self, market_id: &str) -> eyre::Result<UnifiedMarket> {
        let url = format!("{}/market/{}", MANIFOLD_API_URL, market_id);
        let market: ManifoldMarket = self.http.get(&url).send().await?.json().await?;

        let prob = market.probability.unwrap_or(0.5);
        Ok(UnifiedMarket {
            id: market.id.clone(),
            platform: Platform::Manifold,
            question: market.question.clone(),
            description: market.text_description,
            probability: market.probability,
            volume: market.volume,
            liquidity: market.total_liquidity,
            close_time: market.close_time.map(|t| format!("{}", t)),
            url: market.url.clone(),
            outcomes: vec!["Yes".to_string(), "No".to_string()],
            outcome_prices: vec![
                format!("{:.2}", prob),
                format!("{:.2}", 1.0 - prob),
            ],
            category: market.group_slugs.and_then(|s| s.first().cloned()),
            resolved: market.is_resolved,
            resolution: market.resolution,
        })
    }

    // =========================================================================
    // METACULUS
    // =========================================================================

    pub async fn metaculus_search(&self, query: &str, limit: u32) -> eyre::Result<Vec<UnifiedMarket>> {
        let url = format!(
            "{}/questions/?search={}&limit={}&status=open&type=forecast",
            METACULUS_API_URL,
            urlencoding::encode(query),
            limit
        );

        let response: MetaculusQuestionsResponse = self.http.get(&url).send().await?.json().await?;

        Ok(response
            .results
            .into_iter()
            .filter_map(|q| {
                // Only include binary questions with community predictions
                let prob = q.community_prediction.as_ref()?.full.as_ref()?.q2;
                Some(UnifiedMarket {
                    id: q.id.to_string(),
                    platform: Platform::Metaculus,
                    question: q.title.clone(),
                    description: q.description,
                    probability: Some(prob),
                    volume: None,
                    liquidity: None,
                    close_time: q.scheduled_close_time,
                    url: format!("https://www.metaculus.com/questions/{}/", q.id),
                    outcomes: vec!["Yes".to_string(), "No".to_string()],
                    outcome_prices: vec![
                        format!("{:.2}", prob),
                        format!("{:.2}", 1.0 - prob),
                    ],
                    category: q.categories.and_then(|c| c.first().cloned()),
                    resolved: q.resolution.is_some(),
                    resolution: q.resolution.map(|r| format!("{}", r)),
                })
            })
            .collect())
    }

    // =========================================================================
    // AGGREGATED METHODS
    // =========================================================================

    /// Search across all platforms and return unified results
    pub async fn search_all(
        &self,
        query: &str,
        platforms: Option<Vec<Platform>>,
        limit_per_platform: u32,
    ) -> eyre::Result<Vec<UnifiedMarket>> {
        let platforms = platforms.unwrap_or_else(|| vec![
            Platform::Polymarket,
            Platform::Kalshi,
            Platform::Manifold,
            Platform::Metaculus,
        ]);

        let mut all_markets = Vec::new();

        for platform in platforms {
            let result = match platform {
                Platform::Polymarket => self.polymarket_search(query, limit_per_platform).await,
                Platform::Kalshi => self.kalshi_search(query, limit_per_platform).await,
                Platform::Manifold => self.manifold_search(query, limit_per_platform).await,
                Platform::Metaculus => self.metaculus_search(query, limit_per_platform).await,
            };

            match result {
                Ok(markets) => all_markets.extend(markets),
                Err(e) => {
                    tracing::warn!("Failed to search {}: {}", platform.as_str(), e);
                }
            }
        }

        Ok(all_markets)
    }

    /// Find the same question across platforms and compare probabilities
    pub async fn get_aggregated_odds(&self, query: &str) -> eyre::Result<AggregatedOdds> {
        let markets = self.search_all(query, None, 5).await?;

        let mut odds = AggregatedOdds {
            query: query.to_string(),
            polymarket: None,
            kalshi: None,
            manifold: None,
            metaculus: None,
            consensus: None,
            spread: None,
            markets: Vec::new(),
        };

        let mut probs = Vec::new();

        for market in markets {
            if let Some(prob) = market.probability {
                match market.platform {
                    Platform::Polymarket => odds.polymarket = Some(prob),
                    Platform::Kalshi => odds.kalshi = Some(prob),
                    Platform::Manifold => odds.manifold = Some(prob),
                    Platform::Metaculus => odds.metaculus = Some(prob),
                }
                probs.push(prob);
                odds.markets.push(MarketSummary {
                    platform: market.platform.as_str().to_string(),
                    question: market.question,
                    probability: prob,
                    url: market.url,
                });
            }
        }

        if !probs.is_empty() {
            let sum: f64 = probs.iter().sum();
            odds.consensus = Some(sum / probs.len() as f64);

            if probs.len() > 1 {
                let min = probs.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = probs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                odds.spread = Some(max - min);
            }
        }

        Ok(odds)
    }
}

// =============================================================================
// UNIFIED TYPES
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    Polymarket,
    Kalshi,
    Manifold,
    Metaculus,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Polymarket => "polymarket",
            Platform::Kalshi => "kalshi",
            Platform::Manifold => "manifold",
            Platform::Metaculus => "metaculus",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "polymarket" => Some(Platform::Polymarket),
            "kalshi" => Some(Platform::Kalshi),
            "manifold" => Some(Platform::Manifold),
            "metaculus" => Some(Platform::Metaculus),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMarket {
    pub id: String,
    pub platform: Platform,
    pub question: String,
    pub description: Option<String>,
    pub probability: Option<f64>,
    pub volume: Option<f64>,
    pub liquidity: Option<f64>,
    pub close_time: Option<String>,
    pub url: String,
    pub outcomes: Vec<String>,
    pub outcome_prices: Vec<String>,
    pub category: Option<String>,
    pub resolved: bool,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedOdds {
    pub query: String,
    pub polymarket: Option<f64>,
    pub kalshi: Option<f64>,
    pub manifold: Option<f64>,
    pub metaculus: Option<f64>,
    pub consensus: Option<f64>,
    pub spread: Option<f64>,
    pub markets: Vec<MarketSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSummary {
    pub platform: String,
    pub question: String,
    pub probability: f64,
    pub url: String,
}

// =============================================================================
// PLATFORM-SPECIFIC RESPONSE TYPES
// =============================================================================

// Polymarket types
#[derive(Debug, Deserialize)]
pub struct PolymarketEvent {
    pub slug: String,
    pub title: String,
    pub category: Option<String>,
    pub markets: Vec<PolymarketMarket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolymarketMarket {
    pub condition_id: String,
    pub question: String,
    pub description: Option<String>,
    #[serde(default)]
    pub outcomes: serde_json::Value, // Can be string or array
    #[serde(default)]
    pub outcome_prices: serde_json::Value, // Can be string or array
    pub volume_num: Option<f64>,
    pub liquidity_num: Option<f64>,
    pub end_date_iso: Option<String>,
    pub closed: bool,
}

// Kalshi types
#[derive(Debug, Deserialize)]
pub struct KalshiMarketsResponse {
    pub markets: Vec<KalshiMarket>,
}

#[derive(Debug, Deserialize)]
pub struct KalshiMarketResponse {
    pub market: KalshiMarket,
}

#[derive(Debug, Deserialize)]
pub struct KalshiMarket {
    pub ticker: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub last_price: Option<f64>,
    pub volume: Option<i64>,
    pub close_time: Option<String>,
    pub result: Option<String>,
}

// Manifold types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifoldMarket {
    pub id: String,
    pub question: String,
    pub text_description: Option<String>,
    pub probability: Option<f64>,
    pub volume: Option<f64>,
    pub total_liquidity: Option<f64>,
    pub close_time: Option<i64>,
    pub url: String,
    pub group_slugs: Option<Vec<String>>,
    pub is_resolved: bool,
    pub resolution: Option<String>,
}

// Metaculus types
#[derive(Debug, Deserialize)]
pub struct MetaculusQuestionsResponse {
    pub results: Vec<MetaculusQuestion>,
}

#[derive(Debug, Deserialize)]
pub struct MetaculusQuestion {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub categories: Option<Vec<String>>,
    pub scheduled_close_time: Option<String>,
    pub resolution: Option<f64>,
    pub community_prediction: Option<MetaculusPrediction>,
}

#[derive(Debug, Deserialize)]
pub struct MetaculusPrediction {
    pub full: Option<MetaculusPredictionFull>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetaculusPredictionFull {
    pub q2: f64, // median prediction
}

// Helper to parse Polymarket's outcomes/prices which can be JSON strings or arrays
fn parse_json_string_or_array(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        serde_json::Value::String(s) => {
            // Try to parse as JSON array
            serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
        }
        _ => vec![],
    }
}
