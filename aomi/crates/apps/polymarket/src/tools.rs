use crate::client::{GetMarketsParams, GetTradesParams, PolymarketClient, SubmitOrderRequest};
use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, WithTopic};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::LazyLock;
use tokio::sync::Mutex;

// Global client instance
static POLYMARKET_CLIENT: LazyLock<Mutex<PolymarketClient>> =
    LazyLock::new(|| Mutex::new(PolymarketClient::new().expect("Failed to create client")));

// ============================================================================
// Tool 1: Get Markets
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetMarketsArgs {
    /// Maximum number of markets to return (default: 100, max: 1000)
    pub limit: Option<u32>,
    /// Pagination offset (default: 0)
    pub offset: Option<u32>,
    /// Filter for active markets
    pub active: Option<bool>,
    /// Filter for closed markets
    pub closed: Option<bool>,
    /// Filter for archived markets
    pub archived: Option<bool>,
    /// Filter by tag/category (e.g., 'crypto', 'sports', 'politics')
    pub tag: Option<String>,
}

impl AomiToolArgs for GetMarketsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of markets to return (default: 100, max: 1000)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Pagination offset (default: 0)"
                },
                "active": {
                    "type": "boolean",
                    "description": "Filter for active markets"
                },
                "closed": {
                    "type": "boolean",
                    "description": "Filter for closed markets"
                },
                "archived": {
                    "type": "boolean",
                    "description": "Filter for archived markets"
                },
                "tag": {
                    "type": "string",
                    "description": "Filter by tag/category (e.g., 'crypto', 'sports', 'politics')"
                }
            },
            "required": []
        })
    }
}

pub type GetMarketsParameters = WithTopic<GetMarketsArgs>;

#[derive(Debug, Clone)]
pub struct GetMarkets;

impl AomiTool for GetMarkets {
    const NAME: &'static str = "get_polymarket_markets";

    type Args = GetMarketsParameters;
    type Output = serde_json::Value;
    type Error = PolymarketToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Query Polymarket prediction markets with filtering options. Returns a list of markets with their current prices, volumes, liquidity, and other metadata."
    }

    fn run_sync(
        &self,
        result_sender: tokio::sync::oneshot::Sender<eyre::Result<serde_json::Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let params = GetMarketsParams {
                limit: args.inner.limit,
                offset: args.inner.offset,
                active: args.inner.active,
                closed: args.inner.closed,
                archived: args.inner.archived,
                tag: args.inner.tag,
            };

            let result = async {
                let client = POLYMARKET_CLIENT.lock().await;
                let markets = client.get_markets(params).await?;

                let formatted_markets: Vec<serde_json::Value> = markets
                    .iter()
                    .map(|m| {
                        json!({
                            "id": m.id,
                            "question": m.question,
                            "slug": m.slug,
                            "outcomes": m.outcomes,
                            "outcome_prices": m.outcome_prices,
                            "volume": m.volume_num,
                            "liquidity": m.liquidity_num,
                            "active": m.active,
                            "closed": m.closed,
                            "category": m.category,
                            "start_date": m.start_date,
                            "end_date": m.end_date,
                        })
                    })
                    .collect();

                Ok(json!({
                    "markets_count": formatted_markets.len(),
                    "markets": formatted_markets,
                }))
            }
            .await;

            let _ = result_sender.send(result);
        }
    }
}

// ============================================================================
// Tool 2: Get Market Details
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetMarketDetailsArgs {
    /// Market ID or slug (e.g., 'will-bitcoin-reach-100k-by-2025' or market ID)
    pub market_id_or_slug: String,
}

impl AomiToolArgs for GetMarketDetailsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "market_id_or_slug": {
                    "type": "string",
                    "description": "Market ID or slug (e.g., 'will-bitcoin-reach-100k-by-2025' or market ID)"
                }
            },
            "required": ["market_id_or_slug"]
        })
    }
}

pub type GetMarketDetailsParameters = WithTopic<GetMarketDetailsArgs>;

#[derive(Debug, Clone)]
pub struct GetMarketDetails;

impl AomiTool for GetMarketDetails {
    const NAME: &'static str = "get_polymarket_market_details";

    type Args = GetMarketDetailsParameters;
    type Output = serde_json::Value;
    type Error = PolymarketToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get detailed information about a specific Polymarket prediction market by its ID or slug."
    }

    fn run_sync(
        &self,
        result_sender: tokio::sync::oneshot::Sender<eyre::Result<serde_json::Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let result = async {
                let client = POLYMARKET_CLIENT.lock().await;
                let market = client.get_market(&args.inner.market_id_or_slug).await?;

                Ok(json!({
                    "id": market.id,
                    "question": market.question,
                    "slug": market.slug,
                    "condition_id": market.condition_id,
                    "description": market.description,
                    "outcomes": market.outcomes,
                    "outcome_prices": market.outcome_prices,
                    "volume": market.volume,
                    "volume_num": market.volume_num,
                    "liquidity": market.liquidity,
                    "liquidity_num": market.liquidity_num,
                    "start_date": market.start_date,
                    "end_date": market.end_date,
                    "image": market.image,
                    "active": market.active,
                    "closed": market.closed,
                    "archived": market.archived,
                    "category": market.category,
                    "market_type": market.market_type,
                }))
            }
            .await;

            let _ = result_sender.send(result);
        }
    }
}

// ============================================================================
// Tool 3: Get Trades
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetTradesArgs {
    /// Maximum number of trades to return (default: 100, max: 10000)
    pub limit: Option<u32>,
    /// Pagination offset (default: 0)
    pub offset: Option<u32>,
    /// Filter by market condition ID (comma-separated for multiple)
    pub market: Option<String>,
    /// Filter by user wallet address (0x-prefixed)
    pub user: Option<String>,
    /// Filter by trade side ('BUY' or 'SELL')
    pub side: Option<String>,
}

impl AomiToolArgs for GetTradesArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of trades to return (default: 100, max: 10000)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Pagination offset (default: 0)"
                },
                "market": {
                    "type": "string",
                    "description": "Filter by market condition ID (comma-separated for multiple)"
                },
                "user": {
                    "type": "string",
                    "description": "Filter by user wallet address (0x-prefixed)"
                },
                "side": {
                    "type": "string",
                    "description": "Filter by trade side ('BUY' or 'SELL')"
                }
            },
            "required": []
        })
    }
}

pub type GetTradesParameters = WithTopic<GetTradesArgs>;

#[derive(Debug, Clone)]
pub struct GetTrades;

impl AomiTool for GetTrades {
    const NAME: &'static str = "get_polymarket_trades";

    type Args = GetTradesParameters;
    type Output = serde_json::Value;
    type Error = PolymarketToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Retrieve historical trades from Polymarket. Returns trade history with timestamps, prices, sizes, and user information."
    }

    fn run_sync(
        &self,
        result_sender: tokio::sync::oneshot::Sender<eyre::Result<serde_json::Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let params = GetTradesParams {
                limit: args.inner.limit,
                offset: args.inner.offset,
                market: args.inner.market,
                user: args.inner.user,
                side: args.inner.side,
            };

            let result = async {
                let client = POLYMARKET_CLIENT.lock().await;
                let trades = client.get_trades(params).await?;

                let formatted_trades: Vec<serde_json::Value> = trades
                    .iter()
                    .map(|t| {
                        json!({
                            "id": t.id,
                            "market": t.market,
                            "asset": t.asset,
                            "side": t.side,
                            "size": t.size,
                            "price": t.price,
                            "timestamp": t.timestamp,
                            "transaction_hash": t.transaction_hash,
                            "outcome": t.outcome,
                            "proxy_wallet": t.proxy_wallet,
                            "condition_id": t.condition_id,
                            "title": t.title,
                            "slug": t.slug,
                        })
                    })
                    .collect();

                Ok(json!({
                    "trades_count": formatted_trades.len(),
                    "trades": formatted_trades,
                }))
            }
            .await;

            let _ = result_sender.send(result);
        }
    }
}

// ============================================================================
// Tool 4: Place Order
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlacePolymarketOrderArgs {
    /// Wallet address (0x-prefixed) that signed the order
    pub owner: String,
    /// 0x signature returned from the wallet
    pub signature: String,
    /// JSON object describing the order payload per Polymarket docs
    pub order: serde_json::Value,
    /// Optional client order id for idempotency
    pub client_id: Option<String>,
    /// Optional override URL for the orders endpoint
    pub endpoint: Option<String>,
    /// Optional API key value inserted as X-API-KEY
    pub api_key: Option<String>,
    /// Optional JSON object with additional top-level fields to merge into the request
    pub extra_fields: Option<serde_json::Value>,
}

impl AomiToolArgs for PlacePolymarketOrderArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "owner": {
                    "type": "string",
                    "description": "Wallet address (0x-prefixed) that signed the order"
                },
                "signature": {
                    "type": "string",
                    "description": "0x signature returned from the wallet"
                },
                "order": {
                    "type": "object",
                    "description": "JSON object describing the order payload per Polymarket docs"
                },
                "client_id": {
                    "type": "string",
                    "description": "Optional client order id for idempotency"
                },
                "endpoint": {
                    "type": "string",
                    "description": "Optional override URL for the orders endpoint"
                },
                "api_key": {
                    "type": "string",
                    "description": "Optional API key value inserted as X-API-KEY"
                },
                "extra_fields": {
                    "type": "object",
                    "description": "Optional JSON object with additional top-level fields to merge into the request"
                }
            },
            "required": ["owner", "signature", "order"]
        })
    }
}

pub type PlacePolymarketOrderParameters = WithTopic<PlacePolymarketOrderArgs>;

#[derive(Debug, Clone)]
pub struct PlacePolymarketOrder;

impl AomiTool for PlacePolymarketOrder {
    const NAME: &'static str = "place_polymarket_order";

    type Args = PlacePolymarketOrderParameters;
    type Output = serde_json::Value;
    type Error = PolymarketToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Submit a signed Polymarket order to the CLOB API. Provide the wallet address that signed, the 0x signature string, and the order JSON."
    }

    fn run_sync(
        &self,
        result_sender: tokio::sync::oneshot::Sender<eyre::Result<serde_json::Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let request = SubmitOrderRequest {
                owner: args.inner.owner,
                signature: args.inner.signature,
                order: args.inner.order,
                client_id: args.inner.client_id,
                endpoint: args.inner.endpoint,
                api_key: args.inner.api_key,
                extra_fields: args.inner.extra_fields,
            };

            let result = async {
                let client = POLYMARKET_CLIENT.lock().await;
                client.submit_order(request).await
            }
            .await;

            let _ = result_sender.send(result);
        }
    }
}

// ============================================================================
// Error type
// ============================================================================

#[derive(Debug)]
pub struct PolymarketToolError(String);

impl std::fmt::Display for PolymarketToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PolymarketToolError: {}", self.0)
    }
}

impl std::error::Error for PolymarketToolError {}
