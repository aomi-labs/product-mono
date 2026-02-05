//! Delta RFQ HTTP client.
//!
//! Connects to real Delta RFQ API for testnet/mainnet.
//!
//! Set `DELTA_RFQ_API_URL` to override the default base URL.

use eyre::Result;
use serde::{Deserialize, Serialize};
use std::env;

const DEFAULT_API_URL: &str = "http://localhost:3335";

fn get_api_url() -> String {
    env::var("DELTA_RFQ_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

// ============================================================================
// HTTP Client (real API)
// ============================================================================

/// HTTP client for connecting to real Delta RFQ API.
#[derive(Clone)]
pub struct DeltaRfqClient {
    http_client: reqwest::Client,
    base_url: String,
}

impl DeltaRfqClient {
    pub async fn new() -> Result<Self> {
        let url = get_api_url();
        tracing::info!("Delta RFQ: Using HTTP mode ({})", url);
        Self::with_url(url)
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

    pub async fn health(&self) -> Result<HealthResponse> {
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

    pub async fn list_quotes(&self) -> Result<Vec<Quote>> {
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

    pub async fn create_quote(&self, request: CreateQuoteRequest) -> Result<CreateQuoteResponse> {
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

        let quote_response: CreateQuoteResponse = response.json().await?;
        Ok(quote_response)
    }

    pub async fn get_quote(&self, quote_id: &str) -> Result<Quote> {
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

    pub async fn fill_quote(
        &self,
        quote_id: &str,
        request: FillQuoteRequest,
    ) -> Result<FillResponse> {
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

    pub async fn get_receipts(&self, quote_id: &str) -> Result<Vec<Receipt>> {
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

        let receipts: Vec<Receipt> = response.json().await?;
        Ok(receipts)
    }
}

// ============================================================================
// Request/Response Types (matching domain server's flattened API)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub shard: Option<u64>,
    pub mock_mode: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateQuoteRequest {
    pub text: String,
    pub maker_owner_id: String,
    pub maker_shard: u64,
}

/// Flattened quote from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    /// Unique quote ID
    pub id: String,
    /// Original English text
    pub text: String,
    /// Current status: "active", "filled", "expired", "cancelled"
    pub status: String,
    /// Asset being traded (e.g., "dETH")
    pub asset: String,
    /// Trade direction: "buy" or "sell"
    pub direction: String,
    /// Size of the trade
    pub size: f64,
    /// Price limit (max for buys, min for sells)
    pub price_limit: Option<f64>,
    /// Settlement currency (e.g., "USDD")
    pub currency: String,
    /// Expiry as unix timestamp (seconds)
    pub expires_at: i64,
    /// Creation time as unix timestamp (seconds)
    pub created_at: i64,
    /// Maker's owner ID
    pub maker_owner_id: String,
    /// Maker's shard number
    pub maker_shard: u64,
    /// The compiled constraints (Local Law)
    pub local_law: LocalLaw,
}

/// Local Law (compiled constraints)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLaw {
    /// Maximum amount that can be debited (in plancks)
    pub max_debit: u64,
    /// Expiry timestamp
    pub expiry_timestamp: u64,
    /// Allowed price feed sources
    pub allowed_sources: Vec<String>,
    /// Maximum staleness for price feeds (seconds)
    pub max_staleness_secs: u64,
    /// Minimum number of sources required
    pub quorum_count: u32,
    /// Maximum price spread tolerance (percentage)
    pub quorum_tolerance_percent: f64,
    /// Require atomic delivery vs payment
    pub require_atomic_dvp: bool,
    /// Disallow extra transfers
    pub no_side_payments: bool,
}

/// Response after creating a quote (quote fields flattened + extra fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateQuoteResponse {
    /// Unique quote ID
    pub id: String,
    /// Original English text
    pub text: String,
    /// Current status
    pub status: String,
    /// Asset being traded
    pub asset: String,
    /// Trade direction
    pub direction: String,
    /// Size of the trade
    pub size: f64,
    /// Price limit
    pub price_limit: Option<f64>,
    /// Settlement currency
    pub currency: String,
    /// Expiry timestamp
    pub expires_at: i64,
    /// Creation timestamp
    pub created_at: i64,
    /// Maker's owner ID
    pub maker_owner_id: String,
    /// Maker's shard
    pub maker_shard: u64,
    /// The compiled Local Law
    pub local_law: LocalLaw,
    /// Human-readable summary of constraints
    pub constraints_summary: String,
    /// Success message
    pub message: String,
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

/// Response after attempting to fill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillResponse {
    /// Whether the fill succeeded
    pub success: bool,
    /// Fill attempt ID
    pub fill_id: String,
    /// Quote ID that was filled
    pub quote_id: String,
    /// Human-readable message
    pub message: String,
    /// Error details if rejected
    pub error: Option<FillError>,
    /// Receipt details if accepted
    pub receipt: Option<FillReceipt>,
    /// Proof info if accepted
    pub proof: Option<Proof>,
}

/// Error details for rejected fills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillError {
    /// Error code (e.g., "STALE_FEED", "QUORUM_NOT_MET")
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Additional details
    pub details: Option<serde_json::Value>,
}

/// Receipt for successful fills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillReceipt {
    pub id: String,
    pub quote_id: String,
    pub taker_owner_id: String,
    pub taker_shard: u64,
    pub size: f64,
    pub price: f64,
    pub filled_at: i64,
    pub settlement: Option<Settlement>,
}

/// Settlement details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub maker_debit: u64,
    pub maker_credit: u64,
    pub taker_debit: u64,
    pub taker_credit: u64,
    pub asset: String,
    pub currency: String,
}

/// Proof information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    pub sdl_hash: String,
    pub status: String,
}

/// Receipt summary (from get_receipts endpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// Receipt ID
    pub id: String,
    /// Quote ID
    pub quote_id: String,
    /// Whether fill was accepted
    pub success: bool,
    /// Status: "accepted" or "rejected"
    pub status: String,
    /// Taker's owner ID
    pub taker_owner_id: String,
    /// Taker's shard
    pub taker_shard: u64,
    /// Fill size
    pub size: f64,
    /// Fill price
    pub price: f64,
    /// When attempted (unix timestamp)
    pub attempted_at: i64,
    /// Error code if rejected
    pub error_code: Option<String>,
    /// Error message if rejected
    pub error_message: Option<String>,
    /// SDL hash if accepted
    pub sdl_hash: Option<String>,
}
