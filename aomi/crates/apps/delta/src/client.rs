//! Delta RFQ Client with mock support for testing.
//!
//! Supports two modes:
//! - **Mock mode**: In-memory simulation for local testing (no backend required)
//! - **HTTP mode**: Connects to real Delta RFQ API for testnet/mainnet
//!
//! Set `DELTA_RFQ_MOCK=true` to use mock mode, or `DELTA_RFQ_API_URL` for HTTP mode.

use async_trait::async_trait;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;

const DEFAULT_API_URL: &str = "http://localhost:3335";

fn get_api_url() -> String {
    env::var("DELTA_RFQ_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

fn is_mock_mode() -> bool {
    env::var("DELTA_RFQ_MOCK")
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false)
}

// ============================================================================
// Client Trait
// ============================================================================

/// Trait for Delta RFQ client implementations.
#[async_trait]
pub trait DeltaRfqClientTrait: Send + Sync {
    async fn health(&self) -> Result<HealthResponse>;
    async fn list_quotes(&self) -> Result<Vec<Quote>>;
    async fn create_quote(&self, request: CreateQuoteRequest) -> Result<Quote>;
    async fn get_quote(&self, quote_id: &str) -> Result<Quote>;
    async fn fill_quote(&self, quote_id: &str, request: FillQuoteRequest) -> Result<FillResponse>;
    async fn get_receipts(&self, quote_id: &str) -> Result<Vec<QuoteReceipt>>;
}

// ============================================================================
// Unified Client (auto-selects mock or HTTP based on env)
// ============================================================================

/// Delta RFQ Client that auto-selects mock or HTTP mode based on environment.
pub enum DeltaRfqClient {
    Mock(MockDeltaClient),
    Http(HttpDeltaClient),
}

impl DeltaRfqClient {
    pub fn new() -> Result<Self> {
        if is_mock_mode() {
            tracing::info!("Delta RFQ: Using MOCK mode (DELTA_RFQ_MOCK=true)");
            Ok(Self::Mock(MockDeltaClient::new()))
        } else {
            let url = get_api_url();
            tracing::info!("Delta RFQ: Using HTTP mode ({})", url);
            Ok(Self::Http(HttpDeltaClient::with_url(url)?))
        }
    }

    pub fn mock() -> Self {
        Self::Mock(MockDeltaClient::new())
    }

    pub fn http(url: String) -> Result<Self> {
        Ok(Self::Http(HttpDeltaClient::with_url(url)?))
    }
}

impl Default for DeltaRfqClient {
    fn default() -> Self {
        Self::new().expect("Failed to create DeltaRfqClient")
    }
}

#[async_trait]
impl DeltaRfqClientTrait for DeltaRfqClient {
    async fn health(&self) -> Result<HealthResponse> {
        match self {
            Self::Mock(c) => c.health().await,
            Self::Http(c) => c.health().await,
        }
    }

    async fn list_quotes(&self) -> Result<Vec<Quote>> {
        match self {
            Self::Mock(c) => c.list_quotes().await,
            Self::Http(c) => c.list_quotes().await,
        }
    }

    async fn create_quote(&self, request: CreateQuoteRequest) -> Result<Quote> {
        match self {
            Self::Mock(c) => c.create_quote(request).await,
            Self::Http(c) => c.create_quote(request).await,
        }
    }

    async fn get_quote(&self, quote_id: &str) -> Result<Quote> {
        match self {
            Self::Mock(c) => c.get_quote(quote_id).await,
            Self::Http(c) => c.get_quote(quote_id).await,
        }
    }

    async fn fill_quote(&self, quote_id: &str, request: FillQuoteRequest) -> Result<FillResponse> {
        match self {
            Self::Mock(c) => c.fill_quote(quote_id, request).await,
            Self::Http(c) => c.fill_quote(quote_id, request).await,
        }
    }

    async fn get_receipts(&self, quote_id: &str) -> Result<Vec<QuoteReceipt>> {
        match self {
            Self::Mock(c) => c.get_receipts(quote_id).await,
            Self::Http(c) => c.get_receipts(quote_id).await,
        }
    }
}

// ============================================================================
// Mock Client (in-memory simulation)
// ============================================================================

/// In-memory mock client for testing without a real backend.
/// Simulates quote creation, filling, and proof generation.
pub struct MockDeltaClient {
    quotes: Arc<RwLock<HashMap<String, Quote>>>,
    receipts: Arc<RwLock<HashMap<String, Vec<QuoteReceipt>>>>,
    next_id: Arc<RwLock<u64>>,
}

impl MockDeltaClient {
    pub fn new() -> Self {
        Self {
            quotes: Arc::new(RwLock::new(HashMap::new())),
            receipts: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }

    async fn generate_id(&self) -> String {
        // Use a simple counter for predictable IDs in tests
        let mut id = self.next_id.write().await;
        let current = *id;
        *id += 1;
        format!("quote_{}", current)
    }

    /// Parse natural language quote into structured fields.
    /// This simulates the Local Law compilation process.
    fn parse_quote_text(text: &str) -> (String, String, f64, f64) {
        let text_lower = text.to_lowercase();

        // Detect direction
        let direction = if text_lower.contains("buy") {
            "buy"
        } else if text_lower.contains("sell") {
            "sell"
        } else {
            "buy" // default
        };

        // Detect asset
        let asset = if text_lower.contains("deth") || text_lower.contains("eth") {
            "dETH"
        } else if text_lower.contains("dbtc") || text_lower.contains("btc") {
            "dBTC"
        } else {
            "dETH" // default
        };

        // Extract size (simple regex-like parsing)
        let size = extract_number_after(&text_lower, &["buy", "sell"]).unwrap_or(1.0);

        // Extract price limit
        let price_limit = extract_number_after(&text_lower, &["at most", "at least", "for"])
            .unwrap_or(2000.0);

        (direction.to_string(), asset.to_string(), size, price_limit)
    }

    /// Generate a mock Local Law from parsed quote parameters.
    fn generate_local_law(
        direction: &str,
        asset: &str,
        size: f64,
        price_limit: f64,
    ) -> serde_json::Value {
        serde_json::json!({
            "version": "1.0",
            "constraints": [
                {
                    "type": "asset_check",
                    "asset": asset,
                },
                {
                    "type": "direction_check",
                    "direction": direction,
                },
                {
                    "type": "size_check",
                    "max_size": size,
                },
                {
                    "type": "price_check",
                    "operator": if direction == "buy" { "<=" } else { ">=" },
                    "limit": price_limit,
                },
                {
                    "type": "expiration_check",
                    "expires_at": chrono::Utc::now().timestamp() + 300, // 5 min default
                }
            ],
            "compiled_at": chrono::Utc::now().to_rfc3339(),
            "prover": "mock"
        })
    }

    /// Validate a fill against the quote's Local Law constraints.
    fn validate_fill(quote: &Quote, request: &FillQuoteRequest) -> Result<(), String> {
        // Check size
        if let Some(max_size) = quote.size {
            if request.size > max_size {
                return Err(format!(
                    "Fill size {} exceeds quote max size {}",
                    request.size, max_size
                ));
            }
        }

        // Check price based on direction
        if let (Some(direction), Some(price_limit)) = (&quote.direction, quote.price_limit) {
            match direction.as_str() {
                "buy" => {
                    if request.price > price_limit {
                        return Err(format!(
                            "Fill price {} exceeds buy limit {}",
                            request.price, price_limit
                        ));
                    }
                }
                "sell" => {
                    if request.price < price_limit {
                        return Err(format!(
                            "Fill price {} below sell limit {}",
                            request.price, price_limit
                        ));
                    }
                }
                _ => {}
            }
        }

        // Check expiration
        if let Some(expires_at) = quote.expires_at {
            let now = chrono::Utc::now().timestamp();
            if now > expires_at {
                return Err("Quote has expired".to_string());
            }
        }

        // Check quote status
        if let Some(status) = &quote.status {
            if status != "active" {
                return Err(format!("Quote is not active (status: {})", status));
            }
        }

        // Validate feed evidence (require at least 2 sources)
        if request.feed_evidence.len() < 2 {
            return Err("Insufficient price feed evidence (need at least 2 sources)".to_string());
        }

        Ok(())
    }

    /// Generate a mock ZK proof for a valid fill.
    fn generate_mock_proof(quote_id: &str, receipt_id: &str) -> serde_json::Value {
        serde_json::json!({
            "proof_type": "mock_zk_proof",
            "quote_id": quote_id,
            "receipt_id": receipt_id,
            "verified": true,
            "prover": "mock_proving_client",
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "note": "This is a mock proof for testing. In production, this would be a real ZK proof generated by SP1."
        })
    }
}

impl Default for MockDeltaClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DeltaRfqClientTrait for MockDeltaClient {
    async fn health(&self) -> Result<HealthResponse> {
        Ok(HealthResponse {
            status: "healthy".to_string(),
            extra: HashMap::from([
                ("mode".to_string(), serde_json::json!("mock")),
                ("version".to_string(), serde_json::json!("0.1.0")),
            ]),
        })
    }

    async fn list_quotes(&self) -> Result<Vec<Quote>> {
        let quotes = self.quotes.read().await;
        Ok(quotes.values().cloned().collect())
    }

    async fn create_quote(&self, request: CreateQuoteRequest) -> Result<Quote> {
        let id = self.generate_id().await;
        let (direction, asset, size, price_limit) = Self::parse_quote_text(&request.text);
        let local_law = Self::generate_local_law(&direction, &asset, size, price_limit);
        let now = chrono::Utc::now().timestamp();

        let quote = Quote {
            id: Some(id.clone()),
            text: Some(request.text),
            maker_owner_id: Some(request.maker_owner_id),
            maker_shard: Some(request.maker_shard),
            local_law: Some(local_law),
            status: Some("active".to_string()),
            asset: Some(asset),
            direction: Some(direction),
            size: Some(size),
            price_limit: Some(price_limit),
            expires_at: Some(now + 300), // 5 minutes from now
            created_at: Some(now),
            extra: HashMap::new(),
        };

        self.quotes.write().await.insert(id.clone(), quote.clone());
        self.receipts.write().await.insert(id, Vec::new());

        Ok(quote)
    }

    async fn get_quote(&self, quote_id: &str) -> Result<Quote> {
        let quotes = self.quotes.read().await;
        quotes
            .get(quote_id)
            .cloned()
            .ok_or_else(|| eyre::eyre!("Quote not found: {}", quote_id))
    }

    async fn fill_quote(&self, quote_id: &str, request: FillQuoteRequest) -> Result<FillResponse> {
        let mut quotes = self.quotes.write().await;
        let quote = quotes
            .get_mut(quote_id)
            .ok_or_else(|| eyre::eyre!("Quote not found: {}", quote_id))?;

        // Validate the fill against Local Law constraints
        if let Err(e) = Self::validate_fill(quote, &request) {
            return Ok(FillResponse {
                success: false,
                receipt: None,
                error: Some(e),
                proof: None,
                extra: HashMap::new(),
            });
        }

        // Create receipt
        let receipt_id = format!("{}_fill_{}", quote_id, chrono::Utc::now().timestamp_millis());
        let receipt = QuoteReceipt {
            id: Some(receipt_id.clone()),
            quote_id: Some(quote_id.to_string()),
            taker_owner_id: Some(request.taker_owner_id),
            taker_shard: Some(request.taker_shard),
            size: Some(request.size),
            price: Some(request.price),
            filled_at: Some(chrono::Utc::now().timestamp()),
            tx_hash: Some(format!("0xmock_{}", receipt_id)),
            proof: Some(Self::generate_mock_proof(quote_id, &receipt_id)),
            extra: HashMap::new(),
        };

        // Update quote status
        quote.status = Some("filled".to_string());

        // Store receipt
        drop(quotes); // Release write lock before acquiring another
        let mut receipts = self.receipts.write().await;
        receipts
            .entry(quote_id.to_string())
            .or_default()
            .push(receipt.clone());

        Ok(FillResponse {
            success: true,
            receipt: Some(receipt),
            error: None,
            proof: Some(Self::generate_mock_proof(quote_id, &receipt_id)),
            extra: HashMap::new(),
        })
    }

    async fn get_receipts(&self, quote_id: &str) -> Result<Vec<QuoteReceipt>> {
        let receipts = self.receipts.read().await;
        Ok(receipts.get(quote_id).cloned().unwrap_or_default())
    }
}

// ============================================================================
// HTTP Client (real API)
// ============================================================================

/// HTTP client for connecting to real Delta RFQ API.
#[derive(Clone)]
pub struct HttpDeltaClient {
    http_client: reqwest::Client,
    base_url: String,
}

impl HttpDeltaClient {
    pub fn new() -> Result<Self> {
        Self::with_url(get_api_url())
    }

    pub fn with_url(base_url: String) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http_client,
            base_url,
        })
    }
}

impl Default for HttpDeltaClient {
    fn default() -> Self {
        Self::new().expect("Failed to create HttpDeltaClient")
    }
}

#[async_trait]
impl DeltaRfqClientTrait for HttpDeltaClient {
    async fn health(&self) -> Result<HealthResponse> {
        let url = format!("{}/health", self.base_url);
        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Health check failed with status {}: {}",
                status,
                error_text
            ));
        }

        let health: HealthResponse = response.json().await?;
        Ok(health)
    }

    async fn list_quotes(&self) -> Result<Vec<Quote>> {
        let url = format!("{}/quotes", self.base_url);
        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "List quotes failed with status {}: {}",
                status,
                error_text
            ));
        }

        let quotes: Vec<Quote> = response.json().await?;
        Ok(quotes)
    }

    async fn create_quote(&self, request: CreateQuoteRequest) -> Result<Quote> {
        let url = format!("{}/quotes", self.base_url);
        let response = self.http_client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Create quote failed with status {}: {}",
                status,
                error_text
            ));
        }

        let quote: Quote = response.json().await?;
        Ok(quote)
    }

    async fn get_quote(&self, quote_id: &str) -> Result<Quote> {
        let url = format!("{}/quotes/{}", self.base_url, quote_id);
        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Get quote {} failed with status {}: {}",
                quote_id,
                status,
                error_text
            ));
        }

        let quote: Quote = response.json().await?;
        Ok(quote)
    }

    async fn fill_quote(&self, quote_id: &str, request: FillQuoteRequest) -> Result<FillResponse> {
        let url = format!("{}/quotes/{}/fill", self.base_url, quote_id);
        let response = self.http_client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Fill quote {} failed with status {}: {}",
                quote_id,
                status,
                error_text
            ));
        }

        let fill_response: FillResponse = response.json().await?;
        Ok(fill_response)
    }

    async fn get_receipts(&self, quote_id: &str) -> Result<Vec<QuoteReceipt>> {
        let url = format!("{}/quotes/{}/receipts", self.base_url, quote_id);
        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Get receipts for quote {} failed with status {}: {}",
                quote_id,
                status,
                error_text
            ));
        }

        let receipts: Vec<QuoteReceipt> = response.json().await?;
        Ok(receipts)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract a number from text after certain keywords.
fn extract_number_after(text: &str, keywords: &[&str]) -> Option<f64> {
    for keyword in keywords {
        if let Some(pos) = text.find(keyword) {
            let after = &text[pos + keyword.len()..];
            // Find the first number in the remaining text
            let num_str: String = after
                .chars()
                .skip_while(|c| !c.is_ascii_digit() && *c != '.')
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(num) = num_str.parse::<f64>() {
                return Some(num);
            }
        }
    }
    None
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateQuoteRequest {
    pub text: String,
    pub maker_owner_id: String,
    pub maker_shard: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub id: Option<String>,
    pub text: Option<String>,
    pub maker_owner_id: Option<String>,
    pub maker_shard: Option<u64>,
    pub local_law: Option<serde_json::Value>,
    pub status: Option<String>,
    pub asset: Option<String>,
    pub direction: Option<String>,
    pub size: Option<f64>,
    pub price_limit: Option<f64>,
    pub expires_at: Option<i64>,
    pub created_at: Option<i64>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillQuoteRequest {
    pub taker_owner_id: String,
    pub taker_shard: u64,
    pub size: f64,
    pub price: f64,
    pub feed_evidence: Vec<FeedEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedEvidence {
    pub source: String,
    pub asset: String,
    pub price: f64,
    pub timestamp: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillResponse {
    pub success: bool,
    pub receipt: Option<QuoteReceipt>,
    pub error: Option<String>,
    pub proof: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteReceipt {
    pub id: Option<String>,
    pub quote_id: Option<String>,
    pub taker_owner_id: Option<String>,
    pub taker_shard: Option<u64>,
    pub size: Option<f64>,
    pub price: Option<f64>,
    pub filled_at: Option<i64>,
    pub tx_hash: Option<String>,
    pub proof: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_client_create_and_list() {
        let client = MockDeltaClient::new();

        // Create a quote
        let request = CreateQuoteRequest {
            text: "Buy 10 dETH at most 2000 USDD each".to_string(),
            maker_owner_id: "maker123".to_string(),
            maker_shard: 1,
        };

        let quote = client.create_quote(request).await.unwrap();
        assert!(quote.id.is_some());
        assert_eq!(quote.direction, Some("buy".to_string()));
        assert_eq!(quote.asset, Some("dETH".to_string()));
        assert_eq!(quote.size, Some(10.0));
        assert_eq!(quote.price_limit, Some(2000.0));
        assert!(quote.local_law.is_some());

        // List quotes
        let quotes = client.list_quotes().await.unwrap();
        assert_eq!(quotes.len(), 1);
    }

    #[tokio::test]
    async fn test_mock_client_fill_success() {
        let client = MockDeltaClient::new();

        // Create a quote
        let create_req = CreateQuoteRequest {
            text: "Buy 10 dETH at most 2000 USDD".to_string(),
            maker_owner_id: "maker".to_string(),
            maker_shard: 1,
        };
        let quote = client.create_quote(create_req).await.unwrap();
        let quote_id = quote.id.unwrap();

        // Fill with valid price (below limit)
        let fill_req = FillQuoteRequest {
            taker_owner_id: "taker".to_string(),
            taker_shard: 1,
            size: 5.0,
            price: 1900.0, // Below 2000 limit
            feed_evidence: vec![
                FeedEvidence {
                    source: "FeedA".to_string(),
                    asset: "dETH".to_string(),
                    price: 1900.0,
                    timestamp: chrono::Utc::now().timestamp(),
                    signature: "sig1".to_string(),
                },
                FeedEvidence {
                    source: "FeedB".to_string(),
                    asset: "dETH".to_string(),
                    price: 1905.0,
                    timestamp: chrono::Utc::now().timestamp(),
                    signature: "sig2".to_string(),
                },
            ],
        };

        let result = client.fill_quote(&quote_id, fill_req).await.unwrap();
        assert!(result.success);
        assert!(result.receipt.is_some());
        assert!(result.proof.is_some());
    }

    #[tokio::test]
    async fn test_mock_client_fill_rejected() {
        let client = MockDeltaClient::new();

        // Create a quote
        let create_req = CreateQuoteRequest {
            text: "Buy 10 dETH at most 2000 USDD".to_string(),
            maker_owner_id: "maker".to_string(),
            maker_shard: 1,
        };
        let quote = client.create_quote(create_req).await.unwrap();
        let quote_id = quote.id.unwrap();

        // Fill with price above limit - should be rejected
        let fill_req = FillQuoteRequest {
            taker_owner_id: "taker".to_string(),
            taker_shard: 1,
            size: 5.0,
            price: 2100.0, // Above 2000 limit!
            feed_evidence: vec![
                FeedEvidence {
                    source: "FeedA".to_string(),
                    asset: "dETH".to_string(),
                    price: 2100.0,
                    timestamp: chrono::Utc::now().timestamp(),
                    signature: "sig1".to_string(),
                },
                FeedEvidence {
                    source: "FeedB".to_string(),
                    asset: "dETH".to_string(),
                    price: 2100.0,
                    timestamp: chrono::Utc::now().timestamp(),
                    signature: "sig2".to_string(),
                },
            ],
        };

        let result = client.fill_quote(&quote_id, fill_req).await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("exceeds buy limit"));
    }
}
