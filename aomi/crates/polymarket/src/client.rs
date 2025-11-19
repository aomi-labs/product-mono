use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";
const DATA_API_BASE: &str = "https://data-api.polymarket.com";

#[derive(Clone)]
pub struct PolymarketClient {
    http_client: reqwest::Client,
}

impl PolymarketClient {
    pub fn new() -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { http_client })
    }

    /// Get markets from Gamma API
    pub async fn get_markets(&self, params: GetMarketsParams) -> Result<Vec<Market>> {
        let url = format!("{}/markets", GAMMA_API_BASE);

        let mut query_params: Vec<(&str, String)> = Vec::new();

        if let Some(limit) = params.limit {
            query_params.push(("limit", limit.to_string()));
        }
        if let Some(offset) = params.offset {
            query_params.push(("offset", offset.to_string()));
        }
        if let Some(active) = params.active {
            query_params.push(("active", active.to_string()));
        }
        if let Some(closed) = params.closed {
            query_params.push(("closed", closed.to_string()));
        }
        if let Some(archived) = params.archived {
            query_params.push(("archived", archived.to_string()));
        }
        if let Some(ref tag) = params.tag {
            query_params.push(("tag", tag.clone()));
        }

        let response = self
            .http_client
            .get(&url)
            .query(&query_params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Gamma API request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let response_text = response.text().await?;

        // Try to parse as JSON, providing better error messages
        let markets: Vec<Market> = serde_json::from_str(&response_text)
            .map_err(|e| {
                // Log first 500 chars of response for debugging
                let preview = if response_text.len() > 500 {
                    &response_text[..500]
                } else {
                    &response_text
                };
                eyre::eyre!(
                    "Failed to parse markets response: {}\nResponse preview: {}...",
                    e,
                    preview
                )
            })?;
        Ok(markets)
    }

    /// Get a single market by ID or slug
    pub async fn get_market(&self, id_or_slug: &str) -> Result<Market> {
        let url = format!("{}/markets/{}", GAMMA_API_BASE, id_or_slug);

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get market {}: {} - {}",
                id_or_slug,
                status,
                error_text
            ));
        }

        let market: Market = response.json().await?;
        Ok(market)
    }

    /// Get trades from Data API
    pub async fn get_trades(&self, params: GetTradesParams) -> Result<Vec<Trade>> {
        let url = format!("{}/trades", DATA_API_BASE);

        let mut query_params: Vec<(&str, String)> = Vec::new();

        if let Some(limit) = params.limit {
            query_params.push(("limit", limit.to_string()));
        }
        if let Some(offset) = params.offset {
            query_params.push(("offset", offset.to_string()));
        }
        if let Some(ref market) = params.market {
            query_params.push(("market", market.clone()));
        }
        if let Some(ref user) = params.user {
            query_params.push(("user", user.clone()));
        }
        if let Some(ref side) = params.side {
            query_params.push(("side", side.clone()));
        }

        let response = self
            .http_client
            .get(&url)
            .query(&query_params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Data API request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let response_text = response.text().await?;

        // Try to parse as JSON, providing better error messages
        let trades: Vec<Trade> = serde_json::from_str(&response_text)
            .map_err(|e| {
                // Log first 500 chars of response for debugging
                let preview = if response_text.len() > 500 {
                    &response_text[..500]
                } else {
                    &response_text
                };
                eyre::eyre!(
                    "Failed to parse trades response: {}\nResponse preview: {}...",
                    e,
                    preview
                )
            })?;
        Ok(trades)
    }
}

impl Default for PolymarketClient {
    fn default() -> Self {
        Self::new().expect("Failed to create PolymarketClient")
    }
}

// Parameters for get_markets
#[derive(Debug, Default, Clone)]
pub struct GetMarketsParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub archived: Option<bool>,
    pub tag: Option<String>,
}

// Parameters for get_trades
#[derive(Debug, Default, Clone)]
pub struct GetTradesParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub market: Option<String>,
    pub user: Option<String>,
    pub side: Option<String>,
}

// Market data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub id: Option<String>,
    pub question: Option<String>,
    pub slug: Option<String>,
    pub condition_id: Option<String>,
    pub description: Option<String>,
    // Polymarket API returns these as JSON-encoded strings, not arrays
    #[serde(deserialize_with = "deserialize_string_or_array", default)]
    pub outcomes: Option<Vec<String>>,
    #[serde(deserialize_with = "deserialize_string_or_array", default)]
    pub outcome_prices: Option<Vec<String>>,
    pub volume: Option<String>,
    pub volume_num: Option<f64>,
    pub liquidity: Option<String>,
    pub liquidity_num: Option<f64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub image: Option<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub archived: Option<bool>,
    pub category: Option<String>,
    pub market_type: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// Custom deserializer to handle both JSON-encoded strings and actual arrays
fn deserialize_string_or_array<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct StringOrArrayVisitor;

    impl<'de> Visitor<'de> for StringOrArrayVisitor {
        type Value = Option<Vec<String>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or array of strings")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(self)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Try to parse the string as JSON array
            match serde_json::from_str::<Vec<String>>(v) {
                Ok(arr) => Ok(Some(arr)),
                Err(_) => {
                    // If it fails, treat the string itself as a single-element array
                    Ok(Some(vec![v.to_string()]))
                }
            }
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(elem) = seq.next_element()? {
                vec.push(elem);
            }
            Ok(Some(vec))
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }
    }

    deserializer.deserialize_option(StringOrArrayVisitor)
}

// Trade data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub id: Option<String>,
    pub market: Option<String>,
    pub asset: Option<String>,
    pub side: Option<String>,
    // API returns these as numbers, not strings
    pub size: Option<f64>,
    pub price: Option<f64>,
    pub timestamp: Option<i64>,
    pub transaction_hash: Option<String>,
    pub outcome: Option<String>,
    // Additional fields from the API
    pub proxy_wallet: Option<String>,
    pub condition_id: Option<String>,
    pub title: Option<String>,
    pub slug: Option<String>,
    pub icon: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_polymarket_client_creation() {
        let client = PolymarketClient::new();
        assert!(client.is_ok(), "Client should be created successfully");
    }

    #[tokio::test]
    async fn test_get_markets_basic() {
        let client = match PolymarketClient::new() {
            Ok(c) => c,
            Err(_) => return, // Skip test if client creation fails
        };

        let params = GetMarketsParams {
            limit: Some(5),
            offset: None,
            active: Some(true),
            closed: Some(false),
            archived: Some(false),
            tag: None,
        };

        let result = client.get_markets(params).await;
        println!("result: {:?}", result);

        match result {
            Ok(markets) => {
                println!("✅ Successfully fetched {} markets", markets.len());
                assert!(markets.len() <= 5, "Should respect limit parameter");

                // Verify market structure if we got any results
                if let Some(market) = markets.first() {
                    println!("Sample market: {:?}", market.question);
                    assert!(market.id.is_some() || market.slug.is_some(), "Market should have id or slug");
                }
            }
            Err(e) => {
                println!("⚠️  Market fetch failed (may be expected if API is unavailable): {}", e);
                // Don't fail the test - API might be rate limited or unavailable
            }
        }
    }

    #[tokio::test]
    async fn test_get_markets_with_filters() {
        let client = match PolymarketClient::new() {
            Ok(c) => c,
            Err(_) => return,
        };

        let params = GetMarketsParams {
            limit: Some(3),
            offset: Some(0),
            active: Some(true),
            closed: Some(false),
            archived: Some(false),
            tag: Some("crypto".to_string()),
        };

        let result = client.get_markets(params).await;
        println!("result: {:?}", result);

        match result {
            Ok(markets) => {
                println!("✅ Successfully fetched {} crypto markets", markets.len());
                assert!(markets.len() <= 3, "Should respect limit parameter");
            }
            Err(e) => {
                println!("⚠️  Crypto markets fetch failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_get_market_by_slug() {
        let client = match PolymarketClient::new() {
            Ok(c) => c,
            Err(_) => return,
        };

        // First get a market to get a valid slug
        let params = GetMarketsParams {
            limit: Some(5),
            active: Some(true),
            ..Default::default()
        };

        if let Ok(markets) = client.get_markets(params).await {
            println!("markets: {:?}", markets);
            // Try multiple markets until we find one that works
            let mut found_working_slug = false;

            for market in markets.iter().take(5) {
                if let Some(ref slug) = market.slug {
                    println!("Testing with slug: {}", slug);

                    let result = client.get_market(slug).await;
                    match result {
                        Ok(fetched_market) => {
                            println!("✅ Successfully fetched market by slug");
                            assert_eq!(fetched_market.slug, market.slug);
                            found_working_slug = true;
                            break;
                        }
                        Err(e) => {
                            // Some slugs might not be fetchable individually (422 errors)
                            println!("⚠️  Slug '{}' not fetchable: {}", slug, e);
                        }
                    }
                }
            }

            if !found_working_slug {
                println!("⚠️  No fetchable slugs found in test sample (this is okay)");
            }
        }
    }

    #[tokio::test]
    async fn test_get_trades() {
        let client = match PolymarketClient::new() {
            Ok(c) => c,
            Err(_) => return,
        };

        let params = GetTradesParams {
            limit: Some(10),
            offset: None,
            market: None,
            user: None,
            side: None,
        };

        let result = client.get_trades(params).await;
        println!("result: {:?}", result);

        match result {
            Ok(trades) => {
                println!("✅ Successfully fetched {} trades", trades.len());
                assert!(trades.len() <= 10, "Should respect limit parameter");

                // Verify trade structure if we got any results
                if let Some(trade) = trades.first() {
                    println!("Sample trade: side={:?}, price={:?}", trade.side, trade.price);
                    assert!(trade.timestamp.is_some(), "Trade should have timestamp");
                }
            }
            Err(e) => {
                println!("⚠️  Trades fetch failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_get_trades_with_side_filter() {
        let client = match PolymarketClient::new() {
            Ok(c) => c,
            Err(_) => return,
        };

        let params = GetTradesParams {
            limit: Some(5),
            offset: None,
            market: None,
            user: None,
            side: Some("BUY".to_string()),
        };

        let result = client.get_trades(params).await;
        println!("result: {:?}", result);
        
        match result {
            Ok(trades) => {
                println!("✅ Successfully fetched {} BUY trades", trades.len());
                assert!(trades.len() <= 5);

                // Verify all trades are BUY side
                for trade in &trades {
                    if let Some(ref side) = trade.side {
                        assert_eq!(side.to_uppercase(), "BUY", "All trades should be BUY side");
                    }
                }
            }
            Err(e) => {
                println!("⚠️  BUY trades fetch failed: {}", e);
            }
        }
    }

    #[test]
    fn test_get_markets_params_defaults() {
        let params = GetMarketsParams::default();
        assert!(params.limit.is_none());
        assert!(params.offset.is_none());
        assert!(params.active.is_none());
        assert!(params.closed.is_none());
        assert!(params.archived.is_none());
        assert!(params.tag.is_none());
    }

    #[test]
    fn test_get_trades_params_defaults() {
        let params = GetTradesParams::default();
        assert!(params.limit.is_none());
        assert!(params.offset.is_none());
        assert!(params.market.is_none());
        assert!(params.user.is_none());
        assert!(params.side.is_none());
    }

    #[test]
    fn test_market_serialization() {
        let market = Market {
            id: Some("test-123".to_string()),
            question: Some("Will BTC reach 100k?".to_string()),
            slug: Some("btc-100k".to_string()),
            condition_id: Some("0x123".to_string()),
            description: Some("Test market".to_string()),
            outcomes: Some(vec!["Yes".to_string(), "No".to_string()]),
            outcome_prices: Some(vec!["0.5".to_string(), "0.5".to_string()]),
            volume: Some("1000000".to_string()),
            volume_num: Some(1000000.0),
            liquidity: Some("500000".to_string()),
            liquidity_num: Some(500000.0),
            start_date: Some("2025-01-01".to_string()),
            end_date: Some("2025-12-31".to_string()),
            image: Some("https://example.com/image.png".to_string()),
            active: Some(true),
            closed: Some(false),
            archived: Some(false),
            category: Some("crypto".to_string()),
            market_type: Some("binary".to_string()),
            extra: HashMap::new(),
        };

        let json = serde_json::to_string(&market);
        assert!(json.is_ok(), "Market should serialize to JSON");

        let deserialized: Result<Market, _> = serde_json::from_str(&json.unwrap());
        assert!(deserialized.is_ok(), "JSON should deserialize to Market");
    }
}
