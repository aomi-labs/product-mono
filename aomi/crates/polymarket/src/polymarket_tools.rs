use crate::client::{GetMarketsParams, GetTradesParams, PolymarketClient, SubmitOrderRequest};
use aomi_tools::impl_rig_tool_clone;
use rig_derive::rig_tool;
use std::sync::LazyLock;
use tokio::sync::Mutex;

// Global client instance
static POLYMARKET_CLIENT: LazyLock<Mutex<PolymarketClient>> =
    LazyLock::new(|| Mutex::new(PolymarketClient::new().expect("Failed to create client")));

// ============================================================================
// Tool 1: Get Markets
// ============================================================================

#[rig_tool(
    description = "Query Polymarket prediction markets with filtering options. Returns a list of markets with their current prices, volumes, liquidity, and other metadata. Use this to discover markets, analyze trends, or find specific prediction opportunities.",
    params(
        limit = "Optional: Maximum number of markets to return (default: 100, max: 1000)",
        offset = "Optional: Pagination offset (default: 0)",
        active = "Optional: Filter for active markets (true/false)",
        closed = "Optional: Filter for closed markets (true/false)",
        archived = "Optional: Filter for archived markets (true/false)",
        tag = "Optional: Filter by tag/category (e.g., 'crypto', 'sports', 'politics')"
    )
)]
pub async fn get_markets(
    limit: Option<u32>,
    offset: Option<u32>,
    active: Option<bool>,
    closed: Option<bool>,
    archived: Option<bool>,
    tag: Option<String>,
) -> Result<String, rig::tool::ToolError> {
    let client = POLYMARKET_CLIENT.lock().await;

    let params = GetMarketsParams {
        limit,
        offset,
        active,
        closed,
        archived,
        tag,
    };

    let markets = client.get_markets(params).await.map_err(|e| {
        rig::tool::ToolError::ToolCallError(format!("Failed to fetch markets: {}", e).into())
    })?;

    // Format output with key market information
    let formatted_markets: Vec<serde_json::Value> = markets
        .iter()
        .map(|m| {
            serde_json::json!({
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

    let output = serde_json::json!({
        "markets_count": formatted_markets.len(),
        "markets": formatted_markets,
    });

    serde_json::to_string_pretty(&output).map_err(|e| rig::tool::ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 2: Get Market Details
// ============================================================================

#[rig_tool(
    description = "Get detailed information about a specific Polymarket prediction market by its ID or slug. Returns comprehensive market data including current prices, trading volume, liquidity, outcomes, and metadata.",
    params(
        market_id_or_slug = "Market ID or slug (e.g., 'will-bitcoin-reach-100k-by-2025' or market ID)"
    )
)]
pub async fn get_market_details(market_id_or_slug: String) -> Result<String, rig::tool::ToolError> {
    let client = POLYMARKET_CLIENT.lock().await;

    let market = client.get_market(&market_id_or_slug).await.map_err(|e| {
        rig::tool::ToolError::ToolCallError(
            format!("Failed to fetch market '{}': {}", market_id_or_slug, e).into(),
        )
    })?;

    // Format detailed output
    let output = serde_json::json!({
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
        "extra_fields": market.extra,
    });

    serde_json::to_string_pretty(&output).map_err(|e| rig::tool::ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 3: Get Trades
// ============================================================================

#[rig_tool(
    description = "Retrieve historical trades from Polymarket's Data API. Returns trade history with timestamps, prices, sizes, and user information. Use this to analyze trading patterns, track specific markets, or monitor user activity.",
    params(
        limit = "Optional: Maximum number of trades to return (default: 100, max: 10000)",
        offset = "Optional: Pagination offset (default: 0)",
        market = "Optional: Filter by market condition ID (comma-separated for multiple)",
        user = "Optional: Filter by user wallet address (0x-prefixed)",
        side = "Optional: Filter by trade side ('BUY' or 'SELL')"
    )
)]
pub async fn get_trades(
    limit: Option<u32>,
    offset: Option<u32>,
    market: Option<String>,
    user: Option<String>,
    side: Option<String>,
) -> Result<String, rig::tool::ToolError> {
    let client = POLYMARKET_CLIENT.lock().await;

    let params = GetTradesParams {
        limit,
        offset,
        market,
        user,
        side,
    };

    let trades = client.get_trades(params).await.map_err(|e| {
        rig::tool::ToolError::ToolCallError(format!("Failed to fetch trades: {}", e).into())
    })?;

    // Format output with key trade information
    let formatted_trades: Vec<serde_json::Value> = trades
        .iter()
        .map(|t| {
            serde_json::json!({
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

    let output = serde_json::json!({
        "trades_count": formatted_trades.len(),
        "trades": formatted_trades,
    });

    serde_json::to_string_pretty(&output).map_err(|e| rig::tool::ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 4: Place Order
// ============================================================================

#[rig_tool(
    description = "Submit a signed Polymarket order to the CLOB API. Provide the wallet address that signed, the 0x signature string, and the order JSON you signed (token IDs, price, size, expiration, etc.).",
    params(
        owner = "Wallet address (0x-prefixed) that signed the order",
        signature = "0x signature returned from the wallet",
        order = "JSON object describing the order payload per Polymarket docs",
        client_id = "Optional client order id for idempotency",
        endpoint = "Optional override URL for the orders endpoint",
        api_key = "Optional API key value inserted as X-API-KEY",
        extra_fields = "Optional JSON object with additional top-level fields to merge into the request"
    )
)]
pub async fn place_polymarket_order(
    owner: String,
    signature: String,
    order: serde_json::Value,
    client_id: Option<String>,
    endpoint: Option<String>,
    api_key: Option<String>,
    extra_fields: Option<serde_json::Value>,
) -> Result<String, rig::tool::ToolError> {
    let client = POLYMARKET_CLIENT.lock().await;

    let request = SubmitOrderRequest {
        owner,
        signature,
        order,
        client_id,
        endpoint,
        api_key,
        extra_fields,
    };

    let response = client.submit_order(request).await.map_err(|e| {
        rig::tool::ToolError::ToolCallError(format!("Failed to place order: {}", e).into())
    })?;

    serde_json::to_string_pretty(&response)
        .map_err(|e| rig::tool::ToolError::ToolCallError(e.into()))
}

// Implement Clone for all rig_tool functions
impl_rig_tool_clone!(
    GetMarkets,
    GetMarketsParameters,
    [limit, offset, active, closed, archived, tag]
);
impl_rig_tool_clone!(
    GetMarketDetails,
    GetMarketDetailsParameters,
    [market_id_or_slug]
);
impl_rig_tool_clone!(
    GetTrades,
    GetTradesParameters,
    [limit, offset, market, user, side]
);
impl_rig_tool_clone!(
    PlacePolymarketOrder,
    PlacePolymarketOrderParameters,
    [
        owner,
        signature,
        order,
        client_id,
        endpoint,
        api_key,
        extra_fields
    ]
);
