// The trait requires `impl Future` return type, not `async fn`
#![allow(clippy::manual_async_fn)]

use crate::client::{CreateQuoteRequest, DeltaRfqClient, DeltaRfqClientTrait, FeedEvidence, FillQuoteRequest};
use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, WithTopic};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::OnceCell;

// Global client instance - auto-selects mock or HTTP based on DELTA_RFQ_MOCK env var
static DELTA_RFQ_CLIENT: OnceCell<DeltaRfqClient> = OnceCell::const_new();

async fn delta_rfq_client() -> eyre::Result<&'static DeltaRfqClient> {
    DELTA_RFQ_CLIENT
        .get_or_try_init(|| async { DeltaRfqClient::new().await })
        .await
}

// ============================================================================
// Tool 1: Create Quote (Maker)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateQuoteArgs {
    /// Natural language description of the quote (e.g., "Buy 10 dETH at most 2000 USDD each, expires in 5 minutes")
    pub text: String,
    /// Maker's owner ID
    pub maker_owner_id: String,
    /// Maker's shard number
    pub maker_shard: u64,
}

impl AomiToolArgs for CreateQuoteArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Natural language description of the quote (e.g., 'Buy 10 dETH at most 2000 USDD each, expires in 5 minutes')"
                },
                "maker_owner_id": {
                    "type": "string",
                    "description": "Maker's owner ID"
                },
                "maker_shard": {
                    "type": "integer",
                    "description": "Maker's shard number"
                }
            },
            "required": ["text", "maker_owner_id", "maker_shard"]
        })
    }
}

pub type CreateQuoteParameters = WithTopic<CreateQuoteArgs>;

#[derive(Debug, Clone)]
pub struct CreateQuote;

impl AomiTool for CreateQuote {
    const NAME: &'static str = "delta_create_quote";

    type Args = CreateQuoteParameters;
    type Output = serde_json::Value;
    type Error = DeltaToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Create a new RFQ quote from natural language. The backend compiles the text into machine-checkable 'Local Laws' that protect against invalid fills. Examples: 'Buy 10 dETH at most 2000 USDD each, expires in 5 minutes' or 'Sell 5 dBTC for at least 40000 USDD, valid for 1 hour'"
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let request = CreateQuoteRequest {
                text: args.inner.text,
                maker_owner_id: args.inner.maker_owner_id,
                maker_shard: args.inner.maker_shard,
            };

            let client = delta_rfq_client().await?;
            let quote = client.create_quote(request).await?;

            Ok(json!({
                "quote_id": quote.id,
                "text": quote.text,
                "status": quote.status,
                "asset": quote.asset,
                "direction": quote.direction,
                "size": quote.size,
                "price_limit": quote.price_limit,
                "expires_at": quote.expires_at,
                "local_law": quote.local_law,
                "message": "Quote created successfully. The Local Law has been compiled and will enforce your constraints cryptographically."
            }))
        }
    }
}

// ============================================================================
// Tool 2: List Quotes (Maker & Taker)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListQuotesArgs {
    // Currently no filter args, but could add status, asset, etc.
}

impl AomiToolArgs for ListQuotesArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
}

pub type ListQuotesParameters = WithTopic<ListQuotesArgs>;

#[derive(Debug, Clone)]
pub struct ListQuotes;

impl AomiTool for ListQuotes {
    const NAME: &'static str = "delta_list_quotes";

    type Args = ListQuotesParameters;
    type Output = serde_json::Value;
    type Error = DeltaToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "List all active quotes in the Delta RFQ Arena. Returns quotes with their compiled Local Laws, status, and fill parameters."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = delta_rfq_client().await?;
            let quotes = client.list_quotes().await?;

            let formatted_quotes: Vec<serde_json::Value> = quotes
                .iter()
                .map(|q| {
                    json!({
                        "id": q.id,
                        "text": q.text,
                        "status": q.status,
                        "asset": q.asset,
                        "direction": q.direction,
                        "size": q.size,
                        "price_limit": q.price_limit,
                        "expires_at": q.expires_at,
                        "maker_owner_id": q.maker_owner_id,
                    })
                })
                .collect();

            Ok(json!({
                "quotes_count": formatted_quotes.len(),
                "quotes": formatted_quotes,
            }))
        }
    }
}

// ============================================================================
// Tool 3: Get Quote (Maker & Taker)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetQuoteArgs {
    /// Quote ID to retrieve
    pub quote_id: String,
}

impl AomiToolArgs for GetQuoteArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "quote_id": {
                    "type": "string",
                    "description": "Quote ID to retrieve"
                }
            },
            "required": ["quote_id"]
        })
    }
}

pub type GetQuoteParameters = WithTopic<GetQuoteArgs>;

#[derive(Debug, Clone)]
pub struct GetQuote;

impl AomiTool for GetQuote {
    const NAME: &'static str = "delta_get_quote";

    type Args = GetQuoteParameters;
    type Output = serde_json::Value;
    type Error = DeltaToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get detailed information about a specific quote, including its compiled Local Law and current status."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = delta_rfq_client().await?;
            let quote = client.get_quote(&args.inner.quote_id).await?;

            Ok(json!({
                "id": quote.id,
                "text": quote.text,
                "status": quote.status,
                "asset": quote.asset,
                "direction": quote.direction,
                "size": quote.size,
                "price_limit": quote.price_limit,
                "expires_at": quote.expires_at,
                "created_at": quote.created_at,
                "maker_owner_id": quote.maker_owner_id,
                "maker_shard": quote.maker_shard,
                "local_law": quote.local_law,
            }))
        }
    }
}

// ============================================================================
// Tool 4: Fill Quote (Taker)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FillQuoteArgs {
    /// Quote ID to fill
    pub quote_id: String,
    /// Taker's owner ID
    pub taker_owner_id: String,
    /// Taker's shard number
    pub taker_shard: u64,
    /// Size to fill
    pub size: f64,
    /// Price at which to fill
    pub price: f64,
    /// Price feed evidence to prove the fill is valid (array of feed evidence objects)
    pub feed_evidence: Vec<FeedEvidenceArg>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeedEvidenceArg {
    /// Price feed source name (e.g., "FeedA", "Chainlink", "Pyth")
    pub source: String,
    /// Asset the price is for (e.g., "dETH", "dBTC")
    pub asset: String,
    /// Price from this feed
    pub price: f64,
    /// Unix timestamp of the price
    pub timestamp: i64,
    /// Cryptographic signature proving authenticity
    pub signature: String,
}

impl AomiToolArgs for FillQuoteArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "quote_id": {
                    "type": "string",
                    "description": "Quote ID to fill"
                },
                "taker_owner_id": {
                    "type": "string",
                    "description": "Taker's owner ID"
                },
                "taker_shard": {
                    "type": "integer",
                    "description": "Taker's shard number"
                },
                "size": {
                    "type": "number",
                    "description": "Size to fill"
                },
                "price": {
                    "type": "number",
                    "description": "Price at which to fill"
                },
                "feed_evidence": {
                    "type": "array",
                    "description": "Price feed evidence to prove the fill is valid",
                    "items": {
                        "type": "object",
                        "properties": {
                            "source": {
                                "type": "string",
                                "description": "Price feed source name (e.g., 'FeedA', 'Chainlink', 'Pyth')"
                            },
                            "asset": {
                                "type": "string",
                                "description": "Asset the price is for (e.g., 'dETH', 'dBTC')"
                            },
                            "price": {
                                "type": "number",
                                "description": "Price from this feed"
                            },
                            "timestamp": {
                                "type": "integer",
                                "description": "Unix timestamp of the price"
                            },
                            "signature": {
                                "type": "string",
                                "description": "Cryptographic signature proving authenticity"
                            }
                        },
                        "required": ["source", "asset", "price", "timestamp", "signature"]
                    }
                }
            },
            "required": ["quote_id", "taker_owner_id", "taker_shard", "size", "price", "feed_evidence"]
        })
    }
}

pub type FillQuoteParameters = WithTopic<FillQuoteArgs>;

#[derive(Debug, Clone)]
pub struct FillQuote;

impl AomiTool for FillQuote {
    const NAME: &'static str = "delta_fill_quote";

    type Args = FillQuoteParameters;
    type Output = serde_json::Value;
    type Error = DeltaToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Attempt to fill a quote with price feed evidence. The fill will only succeed if it satisfies the quote's Local Law constraints. Requires multiple price feed sources as evidence that the fill price is valid."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let feed_evidence: Vec<FeedEvidence> = args
                .inner
                .feed_evidence
                .into_iter()
                .map(|e| FeedEvidence {
                    source: e.source,
                    asset: e.asset,
                    price: e.price,
                    timestamp: e.timestamp,
                    signature: e.signature,
                })
                .collect();

            let request = FillQuoteRequest {
                taker_owner_id: args.inner.taker_owner_id,
                taker_shard: args.inner.taker_shard,
                size: args.inner.size,
                price: args.inner.price,
                feed_evidence,
            };

            let client = delta_rfq_client().await?;
            let response = client.fill_quote(&args.inner.quote_id, request).await?;

            if response.success {
                Ok(json!({
                    "success": true,
                    "message": "Quote filled successfully! The fill satisfied all Local Law constraints.",
                    "receipt": response.receipt,
                    "proof": response.proof,
                }))
            } else {
                Ok(json!({
                    "success": false,
                    "message": "Fill rejected by Local Law constraints.",
                    "error": response.error,
                }))
            }
        }
    }
}

// ============================================================================
// Tool 5: Get Receipts (Maker & Taker)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetReceiptsArgs {
    /// Quote ID to get receipts for
    pub quote_id: String,
}

impl AomiToolArgs for GetReceiptsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "quote_id": {
                    "type": "string",
                    "description": "Quote ID to get receipts for"
                }
            },
            "required": ["quote_id"]
        })
    }
}

pub type GetReceiptsParameters = WithTopic<GetReceiptsArgs>;

#[derive(Debug, Clone)]
pub struct GetReceipts;

impl AomiTool for GetReceipts {
    const NAME: &'static str = "delta_get_receipts";

    type Args = GetReceiptsParameters;
    type Output = serde_json::Value;
    type Error = DeltaToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get all fill receipts for a quote. Each receipt contains the fill details and ZK proof that the fill was valid according to the Local Law."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = delta_rfq_client().await?;
            let receipts = client.get_receipts(&args.inner.quote_id).await?;

            let formatted_receipts: Vec<serde_json::Value> = receipts
                .iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "quote_id": r.quote_id,
                        "taker_owner_id": r.taker_owner_id,
                        "taker_shard": r.taker_shard,
                        "size": r.size,
                        "price": r.price,
                        "filled_at": r.filled_at,
                        "tx_hash": r.tx_hash,
                        "proof": r.proof,
                    })
                })
                .collect();

            Ok(json!({
                "receipts_count": formatted_receipts.len(),
                "receipts": formatted_receipts,
            }))
        }
    }
}

// ============================================================================
// Error type
// ============================================================================

#[derive(Debug)]
pub struct DeltaToolError(String);

impl std::fmt::Display for DeltaToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DeltaToolError: {}", self.0)
    }
}

impl std::error::Error for DeltaToolError {}
