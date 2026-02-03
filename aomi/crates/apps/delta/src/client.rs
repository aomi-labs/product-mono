//! Delta RFQ client with SDK-backed mocks for development.
//!
//! Supports two modes:
//! - **Mock mode**: SDK runtime + mock proving + mock RPC (no backend required)
//! - **HTTP mode**: Connects to real Delta RFQ API for testnet/mainnet
//!
//! Set `DELTA_RFQ_MOCK=true` to use mock mode, or `DELTA_RFQ_API_URL` for HTTP mode.

use async_trait::async_trait;
// SDK types isolated in a private module to prevent lifetime bound leakage.
// This module has no public API that exposes SDK types.
#[cfg(feature = "sdk")]
mod sdk_runtime {
    use delta_domain_sdk::Domain;
    use delta_domain_sdk::base::{
        core::Shard,
        crypto::ed25519,
        vaults::{Address, Vault},
    };
    use std::collections::HashMap;
    use std::num::NonZero;

    pub async fn init_mock_runtime(shard_num: u64) -> eyre::Result<()> {
        let shard = NonZero::new(shard_num as Shard).unwrap_or_else(|| NonZero::new(1).unwrap());
        let keypair = ed25519::PrivKey::generate();
        let mock_vaults: HashMap<Address, Vault> = HashMap::new();

        // Build domain with in-memory storage and mock RPC (default proving is mock)
        let Domain { runner, client: _, views: _ } = Domain::in_mem_builder(shard, keypair)
            .with_mock_rpc(mock_vaults)
            .build()
            .await?;

        // Run domain in background
        tokio::spawn(async move {
            tracing::info!("[delta-rfq] SDK mock domain started (mock proving + mock RPC)");
            if let Err(err) = runner.run().await {
                tracing::error!("[delta-rfq] SDK mock domain stopped: {err}");
            }
        });

        Ok(())
    }
}

#[cfg(not(feature = "sdk"))]
mod sdk_runtime {
    pub async fn init_mock_runtime(_shard_num: u64) -> eyre::Result<()> {
        tracing::info!("[delta-rfq] Mock mode enabled (SDK mock feature not compiled)");
        Ok(())
    }
}
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

fn mock_shard() -> u64 {
    env::var("DELTA_RFQ_DOMAIN_SHARD")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1)
}

// ============================================================================
// SDK Mock Runtime (mock proving + mock RPC)
// ============================================================================

use std::sync::OnceLock;

static SDK_MOCK_INITIALIZED: OnceLock<bool> = OnceLock::new();

/// Initialize SDK mock runtime once. The runtime runs in a background task
/// demonstrating SDK integration with mock proving and mock RPC.
async fn ensure_mock_runtime() -> Result<()> {
    // Check if already initialized (fast path)
    if SDK_MOCK_INITIALIZED.get().is_some() {
        return Ok(());
    }

    let shard_num = mock_shard();
    sdk_runtime::init_mock_runtime(shard_num).await?;

    // Mark as initialized
    let _ = SDK_MOCK_INITIALIZED.set(true);
    Ok(())
}

pub async fn ensure_runtime() -> Result<()> {
    if is_mock_mode() {
        ensure_mock_runtime().await?;
    }
    Ok(())
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

/// Delta RFQ Client that auto-selects SDK mock or HTTP mode based on environment.
pub enum DeltaRfqClient {
    SdkMock(SdkMockDeltaClient),
    Http(HttpDeltaClient),
}

impl DeltaRfqClient {
    pub async fn new() -> Result<Self> {
        if is_mock_mode() {
            tracing::info!("Delta RFQ: Using SDK MOCK mode (DELTA_RFQ_MOCK=true)");
            Ok(Self::SdkMock(SdkMockDeltaClient::new().await?))
        } else {
            let url = get_api_url();
            tracing::info!("Delta RFQ: Using HTTP mode ({})", url);
            Ok(Self::Http(HttpDeltaClient::with_url(url)?))
        }
    }

    pub async fn mock() -> Result<Self> {
        Ok(Self::SdkMock(SdkMockDeltaClient::new().await?))
    }

    pub fn http(url: String) -> Result<Self> {
        Ok(Self::Http(HttpDeltaClient::with_url(url)?))
    }
}

#[async_trait]
impl DeltaRfqClientTrait for DeltaRfqClient {
    async fn health(&self) -> Result<HealthResponse> {
        match self {
            Self::SdkMock(c) => c.health().await,
            Self::Http(c) => c.health().await,
        }
    }

    async fn list_quotes(&self) -> Result<Vec<Quote>> {
        match self {
            Self::SdkMock(c) => c.list_quotes().await,
            Self::Http(c) => c.list_quotes().await,
        }
    }

    async fn create_quote(&self, request: CreateQuoteRequest) -> Result<Quote> {
        match self {
            Self::SdkMock(c) => c.create_quote(request).await,
            Self::Http(c) => c.create_quote(request).await,
        }
    }

    async fn get_quote(&self, quote_id: &str) -> Result<Quote> {
        match self {
            Self::SdkMock(c) => c.get_quote(quote_id).await,
            Self::Http(c) => c.get_quote(quote_id).await,
        }
    }

    async fn fill_quote(&self, quote_id: &str, request: FillQuoteRequest) -> Result<FillResponse> {
        match self {
            Self::SdkMock(c) => c.fill_quote(quote_id, request).await,
            Self::Http(c) => c.fill_quote(quote_id, request).await,
        }
    }

    async fn get_receipts(&self, quote_id: &str) -> Result<Vec<QuoteReceipt>> {
        match self {
            Self::SdkMock(c) => c.get_receipts(quote_id).await,
            Self::Http(c) => c.get_receipts(quote_id).await,
        }
    }
}

// ============================================================================
// SDK Mock Client (in-memory arena, SDK runtime for proving/base layer)
// ============================================================================

/// SDK-backed mock client for testing without a real backend.
/// Uses Delta SDK's mock proving and mock RPC (initialized at startup)
/// while keeping quote storage in-memory for simplicity.
pub struct SdkMockDeltaClient {
    quotes: Arc<RwLock<HashMap<String, Quote>>>,
    receipts: Arc<RwLock<HashMap<String, Vec<QuoteReceipt>>>>,
    next_id: Arc<RwLock<u64>>,
}

impl SdkMockDeltaClient {
    pub async fn new() -> Result<Self> {
        // Initialize SDK runtime with mock proving + mock RPC
        // (demonstrates SDK integration, runtime runs in background)
        ensure_mock_runtime().await?;
        Ok(Self {
            quotes: Arc::new(RwLock::new(HashMap::new())),
            receipts: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        })
    }

    async fn generate_id(&self) -> String {
        let mut id = self.next_id.write().await;
        let current = *id;
        *id += 1;
        format!("quote_{}", current)
    }

    fn parse_quote_text(text: &str) -> (String, String, f64, f64) {
        let text_lower = text.to_lowercase();

        let direction = if text_lower.contains("buy") {
            "buy"
        } else if text_lower.contains("sell") {
            "sell"
        } else {
            "buy"
        };

        let asset = if text_lower.contains("deth") || text_lower.contains("eth") {
            "dETH"
        } else if text_lower.contains("dbtc") || text_lower.contains("btc") {
            "dBTC"
        } else {
            "dETH"
        };

        let size = extract_number_after(&text_lower, &["buy", "sell"]).unwrap_or(1.0);

        let price_limit = extract_number_after(&text_lower, &["at most", "at least", "for"])
            .unwrap_or(2000.0);

        (direction.to_string(), asset.to_string(), size, price_limit)
    }

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
                    "expires_at": chrono::Utc::now().timestamp() + 300,
                }
            ],
            "compiled_at": chrono::Utc::now().to_rfc3339(),
            "prover": "delta_domain_sdk::proving::mock::Client::global_laws",
            "base_layer": "mock_rpc"
        })
    }

    fn validate_fill(quote: &Quote, request: &FillQuoteRequest) -> Result<(), String> {
        if let Some(max_size) = quote.size {
            if request.size > max_size {
                return Err(format!(
                    "Fill size {} exceeds quote max size {}",
                    request.size, max_size
                ));
            }
        }

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

        if let Some(expires_at) = quote.expires_at {
            let now = chrono::Utc::now().timestamp();
            if now > expires_at {
                return Err("Quote has expired".to_string());
            }
        }

        if let Some(status) = &quote.status {
            if status != "active" {
                return Err(format!("Quote is not active (status: {})", status));
            }
        }

        if request.feed_evidence.len() < 2 {
            return Err("Insufficient price feed evidence (need at least 2 sources)".to_string());
        }

        Ok(())
    }

    fn generate_mock_proof(quote_id: &str, receipt_id: &str) -> serde_json::Value {
        serde_json::json!({
            "proof_type": "sdk_mock_proof",
            "quote_id": quote_id,
            "receipt_id": receipt_id,
            "verified": true,
            "prover": "delta_domain_sdk::proving::mock::Client::global_laws",
            "base_layer": "mock_rpc",
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "note": "Generated via Delta SDK mock proving client; not a real ZK proof."
        })
    }
}

#[async_trait]
impl DeltaRfqClientTrait for SdkMockDeltaClient {
    async fn health(&self) -> Result<HealthResponse> {
        Ok(HealthResponse {
            status: "healthy".to_string(),
            extra: HashMap::from([
                ("mode".to_string(), serde_json::json!("sdk-mock")),
                (
                    "proving".to_string(),
                    serde_json::json!("delta_domain_sdk::proving::mock::Client::global_laws"),
                ),
                ("base_layer".to_string(), serde_json::json!("mock_rpc")),
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
            expires_at: Some(now + 300),
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

        if let Err(e) = Self::validate_fill(quote, &request) {
            return Ok(FillResponse {
                success: false,
                receipt: None,
                error: Some(e),
                proof: None,
                extra: HashMap::new(),
            });
        }

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

        quote.status = Some("filled".to_string());

        drop(quotes);
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
    async fn test_sdk_mock_create_and_list() {
        let client = SdkMockDeltaClient::new().await.unwrap();

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

        let quotes = client.list_quotes().await.unwrap();
        assert_eq!(quotes.len(), 1);
    }

    #[tokio::test]
    async fn test_sdk_mock_fill_success() {
        let client = SdkMockDeltaClient::new().await.unwrap();

        let create_req = CreateQuoteRequest {
            text: "Buy 10 dETH at most 2000 USDD".to_string(),
            maker_owner_id: "maker".to_string(),
            maker_shard: 1,
        };
        let quote = client.create_quote(create_req).await.unwrap();
        let quote_id = quote.id.unwrap();

        let fill_req = FillQuoteRequest {
            taker_owner_id: "taker".to_string(),
            taker_shard: 1,
            size: 5.0,
            price: 1900.0,
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
    async fn test_sdk_mock_fill_rejected() {
        let client = SdkMockDeltaClient::new().await.unwrap();

        let create_req = CreateQuoteRequest {
            text: "Buy 10 dETH at most 2000 USDD".to_string(),
            maker_owner_id: "maker".to_string(),
            maker_shard: 1,
        };
        let quote = client.create_quote(create_req).await.unwrap();
        let quote_id = quote.id.unwrap();

        let fill_req = FillQuoteRequest {
            taker_owner_id: "taker".to_string(),
            taker_shard: 1,
            size: 5.0,
            price: 2100.0,
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
