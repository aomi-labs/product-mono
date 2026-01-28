use eyre::Result;
use serde::{Deserialize, Serialize};
use std::env;

const DEFAULT_API_URL: &str = "http://localhost:3335";

fn get_api_url() -> String {
    env::var("DELTA_RFQ_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

#[derive(Clone)]
pub struct DeltaRfqClient {
    http_client: reqwest::Client,
    base_url: String,
}

impl DeltaRfqClient {
    pub fn new() -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http_client,
            base_url: get_api_url(),
        })
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

    /// Health check
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

    /// List all active quotes
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

    /// Create a new quote from natural language text
    pub async fn create_quote(&self, request: CreateQuoteRequest) -> Result<Quote> {
        let url = format!("{}/quotes", self.base_url);
        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await?;

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

    /// Get a specific quote by ID
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

    /// Attempt to fill a quote
    pub async fn fill_quote(&self, quote_id: &str, request: FillQuoteRequest) -> Result<FillResponse> {
        let url = format!("{}/quotes/{}/fill", self.base_url, quote_id);
        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await?;

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

    /// Get all fill receipts for a quote
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

impl Default for DeltaRfqClient {
    fn default() -> Self {
        Self::new().expect("Failed to create DeltaRfqClient")
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateQuoteRequest {
    /// Natural language description of the quote (e.g., "Buy 10 dETH at most 2000 USDD each, expires in 5 minutes")
    pub text: String,
    /// Maker's owner ID
    pub maker_owner_id: String,
    /// Maker's shard number
    pub maker_shard: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    /// Unique quote identifier
    pub id: Option<String>,
    /// Original text of the quote
    pub text: Option<String>,
    /// Maker's owner ID
    pub maker_owner_id: Option<String>,
    /// Maker's shard
    pub maker_shard: Option<u64>,
    /// Compiled local law (machine-checkable guardrails)
    pub local_law: Option<serde_json::Value>,
    /// Quote status (e.g., "active", "filled", "expired", "cancelled")
    pub status: Option<String>,
    /// Asset being traded
    pub asset: Option<String>,
    /// Direction: "buy" or "sell"
    pub direction: Option<String>,
    /// Quantity
    pub size: Option<f64>,
    /// Max/min price constraint
    pub price_limit: Option<f64>,
    /// Expiration timestamp
    pub expires_at: Option<i64>,
    /// Creation timestamp
    pub created_at: Option<i64>,
    /// Additional fields
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillQuoteRequest {
    /// Taker's owner ID
    pub taker_owner_id: String,
    /// Taker's shard number
    pub taker_shard: u64,
    /// Size to fill
    pub size: f64,
    /// Price at which to fill
    pub price: f64,
    /// Price feed evidence to prove the fill is valid
    pub feed_evidence: Vec<FeedEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedEvidence {
    /// Price feed source name
    pub source: String,
    /// Asset the price is for
    pub asset: String,
    /// Price from this feed
    pub price: f64,
    /// Timestamp of the price
    pub timestamp: i64,
    /// Cryptographic signature proving authenticity
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillResponse {
    /// Whether the fill was successful
    pub success: bool,
    /// Fill receipt if successful
    pub receipt: Option<QuoteReceipt>,
    /// Error message if failed
    pub error: Option<String>,
    /// ZK proof of valid fill (if applicable)
    pub proof: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteReceipt {
    /// Receipt ID
    pub id: Option<String>,
    /// Quote ID this receipt is for
    pub quote_id: Option<String>,
    /// Taker's owner ID
    pub taker_owner_id: Option<String>,
    /// Taker's shard
    pub taker_shard: Option<u64>,
    /// Filled size
    pub size: Option<f64>,
    /// Fill price
    pub price: Option<f64>,
    /// Timestamp of the fill
    pub filled_at: Option<i64>,
    /// Transaction hash (if on-chain)
    pub tx_hash: Option<String>,
    /// ZK proof
    pub proof: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = DeltaRfqClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_with_custom_url() {
        let client = DeltaRfqClient::with_url("http://custom:8080".to_string());
        assert!(client.is_ok());
    }

    #[test]
    fn test_create_quote_request_serialization() {
        let request = CreateQuoteRequest {
            text: "Buy 10 dETH at most 2000 USDD each".to_string(),
            maker_owner_id: "maker123".to_string(),
            maker_shard: 9,
        };

        let json = serde_json::to_string(&request);
        assert!(json.is_ok());
    }

    #[test]
    fn test_fill_quote_request_serialization() {
        let request = FillQuoteRequest {
            taker_owner_id: "taker456".to_string(),
            taker_shard: 9,
            size: 10.0,
            price: 1950.0,
            feed_evidence: vec![
                FeedEvidence {
                    source: "FeedA".to_string(),
                    asset: "dETH".to_string(),
                    price: 1950.0,
                    timestamp: 1769250388,
                    signature: "sig1".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&request);
        assert!(json.is_ok());
    }
}
