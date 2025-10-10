//! 0x API v2 integration for swap pricing - optimized for AI agents
use eyre::Result;
use reqwest::Client;
use rmcp::{ErrorData, handler::server::tool::Parameters, model::CallToolResult, tool};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

const ZEROX_API_BASE: &str = "https://api.0x.org";
const CACHE_DURATION_SECS: u64 = 30; // 0x recommends 30 second cache

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SwapPriceParams {
    #[schemars(
        description = "The chain ID (1 for Ethereum mainnet, 137 for Polygon, 42161 for Arbitrum, 10 for Optimism, 8453 for Base)"
    )]
    pub chain_id: u64,

    #[schemars(
        description = "The contract address of the token being sold (use 'ETH' or '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE' for native ETH)"
    )]
    pub sell_token: String,

    #[schemars(
        description = "The contract address of the token being bought (use 'ETH' or '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE' for native ETH)"
    )]
    pub buy_token: String,

    #[schemars(
        description = "The amount of sell_token to sell (in wei or smallest unit). Exactly one of sell_amount or buy_amount required."
    )]
    pub sell_amount: Option<String>,

    #[schemars(
        description = "The amount of buy_token to buy (in wei or smallest unit). Exactly one of sell_amount or buy_amount required."
    )]
    pub buy_amount: Option<String>,

    #[schemars(
        description = "The address that will execute the swap (optional for price, required for quote)"
    )]
    pub taker: Option<String>,

    #[schemars(description = "Slippage tolerance as a decimal (e.g., 0.01 for 1%). Default: 0.01")]
    pub slippage_percentage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SwapQuoteParams {
    #[schemars(
        description = "The chain ID (1 for Ethereum mainnet, 137 for Polygon, 42161 for Arbitrum, 10 for Optimism, 8453 for Base)"
    )]
    pub chain_id: u64,

    #[schemars(
        description = "The contract address of the token being sold (use 'ETH' or '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE' for native ETH)"
    )]
    pub sell_token: String,

    #[schemars(
        description = "The contract address of the token being bought (use 'ETH' or '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE' for native ETH)"
    )]
    pub buy_token: String,

    #[schemars(
        description = "The amount of sell_token to sell (in wei or smallest unit). Exactly one of sell_amount or buy_amount required."
    )]
    pub sell_amount: Option<String>,

    #[schemars(
        description = "The amount of buy_token to buy (in wei or smallest unit). Exactly one of sell_amount or buy_amount required."
    )]
    pub buy_amount: Option<String>,

    #[schemars(description = "The address that will execute the swap (REQUIRED for quote)")]
    pub taker: String,

    #[schemars(description = "Slippage tolerance as a decimal (e.g., 0.01 for 1%). Default: 0.01")]
    pub slippage_percentage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceResponse {
    #[serde(rename = "blockNumber")]
    pub block_number: Option<String>,
    #[serde(rename = "buyAmount")]
    pub buy_amount: String,
    #[serde(rename = "buyToken")]
    pub buy_token: String,
    #[serde(rename = "sellAmount")]
    pub sell_amount: String,
    #[serde(rename = "sellToken")]
    pub sell_token: String,
    pub gas: Option<String>,
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<String>,
    #[serde(rename = "liquidityAvailable")]
    pub liquidity_available: Option<bool>,
    #[serde(rename = "minBuyAmount")]
    pub min_buy_amount: Option<String>,
    pub route: Option<RouteInfo>,
    pub fees: Option<serde_json::Value>,
    pub issues: Option<serde_json::Value>,
    #[serde(rename = "tokenMetadata")]
    pub token_metadata: Option<serde_json::Value>,
    #[serde(rename = "totalNetworkFee")]
    pub total_network_fee: Option<String>,
    pub zid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteResponse {
    #[serde(rename = "blockNumber")]
    pub block_number: Option<String>,
    #[serde(rename = "buyAmount")]
    pub buy_amount: String,
    #[serde(rename = "buyToken")]
    pub buy_token: String,
    #[serde(rename = "sellAmount")]
    pub sell_amount: String,
    #[serde(rename = "sellToken")]
    pub sell_token: String,
    pub to: Option<String>,
    pub data: Option<String>,
    pub value: Option<String>,
    pub gas: Option<String>,
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<String>,
    #[serde(rename = "liquidityAvailable")]
    pub liquidity_available: Option<bool>,
    #[serde(rename = "minBuyAmount")]
    pub min_buy_amount: Option<String>,
    pub route: Option<RouteInfo>,
    pub fees: Option<serde_json::Value>,
    pub issues: Option<serde_json::Value>,
    #[serde(rename = "tokenMetadata")]
    pub token_metadata: Option<serde_json::Value>,
    #[serde(rename = "allowanceTarget")]
    pub allowance_target: Option<String>,
    pub permit2: Option<Permit2Data>,
    pub zid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquiditySource {
    pub name: String,
    pub proportion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteInfo {
    pub fills: Vec<Fill>,
    pub tokens: Vec<TokenInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub from: String,
    pub to: String,
    pub source: String,
    #[serde(rename = "proportionBps")]
    pub proportion_bps: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub address: String,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permit2Data {
    pub eip712: serde_json::Value,
    pub signature: Option<String>,
}

#[derive(Clone)]
struct CachedPrice {
    response: PriceResponse,
    timestamp: u64,
}

#[derive(Clone)]
pub struct ZeroXTool {
    client: Client,
    api_key: Option<String>,
    price_cache: Arc<RwLock<HashMap<String, CachedPrice>>>,
}

impl ZeroXTool {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            price_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn get_chain_name(&self, chain_id: u64) -> &str {
        match chain_id {
            1 => "Ethereum",
            137 => "Polygon",
            42161 => "Arbitrum",
            10 => "Optimism",
            8453 => "Base",
            56 => "BSC",
            43114 => "Avalanche",
            _ => "Unknown",
        }
    }

    fn format_native_token(&self, address: &str) -> String {
        if address == "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE"
            || address.to_lowercase() == "eth"
        {
            "ETH".to_string()
        } else {
            address.to_string()
        }
    }

    fn normalize_token_address(&self, token: &str) -> String {
        // Allow both "ETH" and the special address for native ETH
        if token.to_lowercase() == "eth" {
            "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE".to_string()
        } else {
            token.to_string()
        }
    }

    async fn make_request<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        params: Vec<(&str, String)>,
    ) -> Result<T, ErrorData> {
        let mut request = self
            .client
            .get(format!("{ZEROX_API_BASE}{endpoint}"))
            .query(&params)
            .header("0x-version", "v2")
            .header("Content-Type", "application/json");

        // Add API key if available
        if let Some(api_key) = &self.api_key {
            request = request.header("0x-api-key", api_key);
        }

        let response = request.send().await.map_err(|e| {
            ErrorData::internal_error(format!("Failed to fetch from 0x: {e}"), None)
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            if status.as_u16() == 429 {
                return Err(ErrorData::internal_error(
                    "Rate limit exceeded. Please wait before retrying or use an API key.",
                    None,
                ));
            }

            return Err(ErrorData::internal_error(
                format!("0x API error ({status}): {error_text}"),
                None,
            ));
        }

        response
            .json()
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to parse response: {e}"), None))
    }

    #[tool(
        description = "Get a price estimate for swapping tokens using 0x API. This is a lightweight endpoint for price discovery without generating transaction data. Use this for displaying prices to users. Works with Anvil/test environments.

        Price should NOT be used for calldata that was not provided by the 0x API."
    )]
    pub async fn get_swap_price(
        &self,
        Parameters(params): Parameters<SwapPriceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Validate that either sell_amount or buy_amount is provided
        if params.sell_amount.is_none() && params.buy_amount.is_none() {
            return Err(ErrorData::invalid_params(
                "Either sell_amount or buy_amount must be provided",
                None,
            ));
        }

        if params.sell_amount.is_some() && params.buy_amount.is_some() {
            return Err(ErrorData::invalid_params(
                "Only one of sell_amount or buy_amount should be provided",
                None,
            ));
        }

        // Check cache
        let cache_key = format!(
            "{}-{}-{}-{:?}-{:?}",
            params.chain_id,
            params.sell_token,
            params.buy_token,
            params.sell_amount,
            params.buy_amount
        );

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Check if we have a cached price
        {
            let cache = self.price_cache.read().await;
            if let Some(cached) = cache.get(&cache_key)
                && now - cached.timestamp < CACHE_DURATION_SECS {
                    return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                        format!(
                            "Cached ({}s ago): {}",
                            now - cached.timestamp,
                            self.format_price_response(
                                &cached.response,
                                &params.sell_token,
                                &params.buy_token,
                                params.chain_id
                            )
                        ),
                    )]));
                }
        }

        // Build query parameters - normalize token addresses for v1 API
        let mut query_params = vec![
            ("chainId", params.chain_id.to_string()),
            (
                "sellToken",
                self.normalize_token_address(&params.sell_token),
            ),
            ("buyToken", self.normalize_token_address(&params.buy_token)),
        ];

        if let Some(amount) = params.sell_amount {
            query_params.push(("sellAmount", amount));
        }

        if let Some(amount) = params.buy_amount {
            query_params.push(("buyAmount", amount));
        }

        if let Some(taker) = params.taker {
            query_params.push(("taker", taker));
        }

        let slippage = params.slippage_percentage.unwrap_or(0.01).to_string();
        query_params.push(("slippagePercentage", slippage));

        // Make the API request - use permit2 endpoint (requires API key)
        let price_response: PriceResponse = self
            .make_request("/swap/permit2/price", query_params)
            .await?;

        // Cache the response
        {
            let mut cache = self.price_cache.write().await;
            cache.insert(
                cache_key,
                CachedPrice {
                    response: price_response.clone(),
                    timestamp: now,
                },
            );
        }

        let output = self.format_price_response(
            &price_response,
            &params.sell_token,
            &params.buy_token,
            params.chain_id,
        );

        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            output,
        )]))
    }

    #[tool(
        description = "Get an executable swap quote with transaction data using 0x API. This generates the complete transaction data needed to execute a swap. The taker address is REQUIRED for quotes. Works with Anvil/test environments."
    )]
    pub async fn get_swap_quote(
        &self,
        Parameters(params): Parameters<SwapQuoteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Validate that either sell_amount or buy_amount is provided
        if params.sell_amount.is_none() && params.buy_amount.is_none() {
            return Err(ErrorData::invalid_params(
                "Either sell_amount or buy_amount must be provided",
                None,
            ));
        }

        if params.sell_amount.is_some() && params.buy_amount.is_some() {
            return Err(ErrorData::invalid_params(
                "Only one of sell_amount or buy_amount should be provided",
                None,
            ));
        }

        // Build query parameters - normalize token addresses for v1 API
        let mut query_params = vec![
            ("chainId", params.chain_id.to_string()),
            (
                "sellToken",
                self.normalize_token_address(&params.sell_token),
            ),
            ("buyToken", self.normalize_token_address(&params.buy_token)),
            ("taker", params.taker.clone()),
        ];

        if let Some(amount) = params.sell_amount {
            query_params.push(("sellAmount", amount));
        }

        if let Some(amount) = params.buy_amount {
            query_params.push(("buyAmount", amount));
        }

        let slippage = params.slippage_percentage.unwrap_or(0.01).to_string();
        query_params.push(("slippagePercentage", slippage));

        // Make the API request - use permit2 endpoint (requires API key)
        let quote: QuoteResponse = self
            .make_request("/swap/permit2/quote", query_params)
            .await?;

        let output = self.format_quote_response(
            &quote,
            &params.sell_token,
            &params.buy_token,
            params.chain_id,
        );

        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            output,
        )]))
    }

    fn format_price_response(
        &self,
        price: &PriceResponse,
        sell_token: &str,
        buy_token: &str,
        chain_id: u64,
    ) -> String {
        // Calculate price
        let buy_amt: f64 = price.buy_amount.parse().unwrap_or(0.0);
        let sell_amt: f64 = price.sell_amount.parse().unwrap_or(0.0);
        let calculated_price = if sell_amt > 0.0 {
            buy_amt / sell_amt
        } else {
            0.0
        };

        let mut output = format!(
            "Price on {}: {} {} → {} {} | Rate: {:.6} {} per {}",
            self.get_chain_name(chain_id),
            price.sell_amount,
            self.format_native_token(sell_token),
            price.buy_amount,
            self.format_native_token(buy_token),
            calculated_price,
            self.format_native_token(buy_token),
            self.format_native_token(sell_token)
        );

        if let Some(min_buy) = &price.min_buy_amount {
            output.push_str(&format!(" | Min: {min_buy} wei"));
        }

        if let Some(gas) = &price.gas {
            output.push_str(&format!(" | Gas: {gas}"));
        }

        if let Some(route) = &price.route
            && !route.fills.is_empty() {
                output.push_str(" | Route: ");
                let routes: Vec<String> = route
                    .fills
                    .iter()
                    .map(|fill| {
                        if let Some(bps) = &fill.proportion_bps {
                            let percentage = bps.parse::<f64>().unwrap_or(0.0) / 100.0;
                            format!("{} {:.1}%", fill.source, percentage)
                        } else {
                            fill.source.clone()
                        }
                    })
                    .collect();
                output.push_str(&routes.join(", "));
            }

        output
    }

    fn format_quote_response(
        &self,
        quote: &QuoteResponse,
        sell_token: &str,
        buy_token: &str,
        chain_id: u64,
    ) -> String {
        // Calculate price
        let buy_amt: f64 = quote.buy_amount.parse().unwrap_or(0.0);
        let sell_amt: f64 = quote.sell_amount.parse().unwrap_or(0.0);
        let calculated_price = if sell_amt > 0.0 {
            buy_amt / sell_amt
        } else {
            0.0
        };

        let mut output = format!(
            "Quote on {}: {} {} → {} {} | Rate: {:.6}",
            self.get_chain_name(chain_id),
            quote.sell_amount,
            self.format_native_token(sell_token),
            quote.buy_amount,
            self.format_native_token(buy_token),
            calculated_price,
        );

        if let Some(gas) = &quote.gas {
            output.push_str(&format!(" | Gas: {gas}"));
        }

        // Transaction details on next line
        if let Some(to) = &quote.to {
            output.push_str(&format!("\nTo: {to}"));
            if let Some(value) = &quote.value
                && value != "0" {
                    output.push_str(&format!(" | Value: {value}"));
                }
        }

        if let Some(allowance_target) = &quote.allowance_target {
            output.push_str(&format!("\nApproval needed: {allowance_target}"));
        }

        if let Some(data) = &quote.data {
            output.push_str(&format!("\nCalldata: {data}"));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_name() {
        let tool = ZeroXTool::new(None);
        assert_eq!(tool.get_chain_name(1), "Ethereum");
        assert_eq!(tool.get_chain_name(137), "Polygon");
        assert_eq!(tool.get_chain_name(42161), "Arbitrum");
    }

    #[test]
    fn test_native_token_format() {
        let tool = ZeroXTool::new(None);
        assert_eq!(
            tool.format_native_token("0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE"),
            "ETH".to_string()
        );
        assert_eq!(
            tool.format_native_token("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"),
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string()
        );
    }
}
