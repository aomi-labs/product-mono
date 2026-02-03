//! Delta RFQ HTTP client.
//!
//! Connects to real Delta RFQ API for testnet/mainnet.
//!
//! Set `DELTA_RFQ_API_URL` to override the default base URL.

use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

    pub async fn create_quote(&self, request: CreateQuoteRequest) -> Result<Quote> {
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

    pub async fn fill_quote(&self, quote_id: &str, request: FillQuoteRequest) -> Result<FillResponse> {
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

    pub async fn get_receipts(&self, quote_id: &str) -> Result<Vec<QuoteReceipt>> {
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

