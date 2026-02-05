//! Account information tools using EvmGateway.
//!
//! These tools provide account information (balance, nonce) and transaction history
//! using the unified EvmGateway abstraction.

use crate::{AomiTool, AomiToolArgs, ToolCallCtx, with_topic};
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::future::Future;
use tokio::task;
use tracing::debug;

use super::gateway::get_gateway;

// ============================================================================
// Tool Definitions
// ============================================================================

/// Tool for getting account information (balance and nonce)
#[derive(Debug, Clone)]
pub struct GetAccountInfo;

/// Tool for getting transaction history with smart database caching
#[derive(Debug, Clone)]
pub struct GetAccountTransactionHistory;

// ============================================================================
// Argument Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAccountInfoArgs {
    pub address: String,
    pub chain_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAccountTransactionHistoryArgs {
    pub address: String,
    pub chain_id: u32,
    pub current_nonce: i64,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

impl AomiToolArgs for GetAccountInfoArgs {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "The Ethereum address to query (e.g., \"0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045\"). Must be a 42-character hex string starting with 0x"
                },
                "chain_id": {
                    "type": "number",
                    "description": "The chain ID as an integer (e.g., 1 for Ethereum mainnet, 137 for Polygon, 42161 for Arbitrum)"
                }
            },
            "required": ["address", "chain_id"]
        }))
    }
}

impl AomiToolArgs for GetAccountTransactionHistoryArgs {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "The Ethereum address to query transactions for"
                },
                "chain_id": {
                    "type": "number",
                    "description": "The chain ID as an integer (e.g., 1 for Ethereum mainnet)"
                },
                "current_nonce": {
                    "type": "number",
                    "description": "The current nonce of the account (use get_account_info to fetch this)"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of transactions to return (default: 100)"
                },
                "offset": {
                    "type": "number",
                    "description": "Number of transactions to skip for pagination (default: 0)"
                }
            },
            "required": ["address", "chain_id", "current_nonce"]
        }))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn run_sync<F, T>(future: F) -> Result<T, ToolError>
where
    F: Future<Output = Result<T, ToolError>> + Send + 'static,
    T: Send + 'static,
{
    task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

// ============================================================================
// Execute Functions
// ============================================================================

/// Execute get_account_info using the EvmGateway.
///
/// This is a unified implementation that works in both production and eval-test modes.
/// The gateway handles the differences internally:
/// - Production: Etherscan-first with Cast fallback
/// - Eval-test: Cast/RPC only
pub async fn execute_get_account_info(
    args: GetAccountInfoArgs,
) -> Result<serde_json::Value, ToolError> {
    debug!("get_account_info tool called with args: {:?}", args);

    run_sync(async move {
        let address = args.address;
        let chain_id = args.chain_id;

        debug!(
            "get_account_info called with address={}, chain_id={}",
            address, chain_id
        );

        // Get the gateway and fetch account info
        let gateway = get_gateway().await.map_err(|e| {
            ToolError::ToolCallError(format!("Failed to get gateway: {}", e).into())
        })?;

        let info = gateway
            .get_account_info(chain_id as u64, &address)
            .await
            .map_err(|e| {
                ToolError::ToolCallError(format!("Failed to get account info: {}", e).into())
            })?;

        debug!(
            "Successfully fetched account info: balance={} wei, nonce={}",
            info.balance, info.nonce
        );

        let response = json!({
            "address": info.address,
            "balance": info.balance,
            "nonce": info.nonce,
        });

        debug!("get_account_info succeeded");
        Ok(response)
    })
}

/// Execute get_account_transaction_history.
///
/// This tool fetches transaction history with smart database caching.
/// In eval-test mode (LocalGateway), this returns an empty list since
/// there's no DB access.
pub async fn execute_get_account_transaction_history(
    args: GetAccountTransactionHistoryArgs,
) -> Result<serde_json::Value, ToolError> {
    debug!(
        "get_account_transaction_history tool called with args: {:?}",
        args
    );

    run_sync(async move {
        let GetAccountTransactionHistoryArgs {
            address,
            chain_id,
            current_nonce,
            limit,
            offset,
        } = args;

        let address = address.to_lowercase();

        debug!(
            "get_account_transaction_history called with address={}, chain_id={}, nonce={}",
            address, chain_id, current_nonce
        );

        // Get the gateway
        let gateway = get_gateway().await.map_err(|e| {
            ToolError::ToolCallError(format!("Failed to get gateway: {}", e).into())
        })?;

        // Validate chain is supported
        if !gateway.is_supported(chain_id as u64) {
            return Err(ToolError::ToolCallError(
                format!(
                    "Chain {} is not supported. Supported chains: {:?}",
                    chain_id,
                    gateway.supported_chains()
                )
                .into(),
            ));
        }

        // Use the gateway's transaction history method
        let transactions = gateway
            .get_transaction_history(chain_id as u64, &address, current_nonce, limit, offset)
            .await
            .map_err(|e| {
                ToolError::ToolCallError(format!("Failed to get transaction history: {}", e).into())
            })?;

        debug!("Returning {} transactions", transactions.len());

        // Convert transactions to JSON
        let tx_json: Vec<serde_json::Value> = transactions
            .into_iter()
            .map(|tx| {
                json!({
                    "id": tx.id,
                    "chain_id": tx.chain_id,
                    "address": tx.address,
                    "hash": tx.hash,
                    "block_number": tx.block_number,
                    "timestamp": tx.timestamp,
                    "from_address": tx.from_address,
                    "to_address": tx.to_address,
                    "value": tx.value,
                    "gas": tx.gas,
                    "gas_price": tx.gas_price,
                    "gas_used": tx.gas_used,
                    "is_error": tx.is_error,
                    "input": tx.input,
                    "contract_address": tx.contract_address,
                })
            })
            .collect();

        Ok(json!({
            "transactions": tx_json,
            "count": tx_json.len(),
        }))
    })
}

// ============================================================================
// AomiTool Implementations
// ============================================================================

impl AomiTool for GetAccountInfo {
    const NAME: &'static str = "get_account_info";

    type Args = GetAccountInfoArgs;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Fetch account information (balance and nonce)."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_get_account_info(args)
                .await
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for GetAccountTransactionHistory {
    const NAME: &'static str = "get_account_transaction_history";

    type Args = GetAccountTransactionHistoryArgs;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Fetch transaction history with smart database caching."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_get_account_transaction_history(args)
                .await
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_get_account_info_tool() -> Result<(), Box<dyn std::error::Error>> {
        if std::env::var("ETHERSCAN_API_KEY").is_err() {
            eprintln!("Skipping test_get_account_info_tool: ETHERSCAN_API_KEY not set");
            return Ok(());
        }

        let args = GetAccountInfoArgs {
            address: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            chain_id: crate::etherscan::ETHEREUM_MAINNET,
        };

        let result = execute_get_account_info(args).await?;
        println!("Result: {}", serde_json::to_string_pretty(&result)?);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_get_account_transaction_history_tool() -> Result<(), Box<dyn std::error::Error>> {
        if std::env::var("ETHERSCAN_API_KEY").is_err() {
            eprintln!(
                "Skipping test_get_account_transaction_history_tool: ETHERSCAN_API_KEY not set"
            );
            return Ok(());
        }
        if std::env::var("DATABASE_URL").is_err() {
            eprintln!("Skipping test_get_account_transaction_history_tool: DATABASE_URL not set");
            return Ok(());
        }

        // First get account info
        let account_args = GetAccountInfoArgs {
            address: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            chain_id: crate::etherscan::ETHEREUM_MAINNET,
        };

        let account_result = execute_get_account_info(account_args).await?;
        let nonce = account_result["nonce"].as_i64().unwrap();

        // Then get transaction history
        let tx_args = GetAccountTransactionHistoryArgs {
            address: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            chain_id: crate::etherscan::ETHEREUM_MAINNET,
            current_nonce: nonce,
            limit: Some(5),
            offset: Some(0),
        };

        let result = execute_get_account_transaction_history(tx_args).await?;
        println!("Result: {}", serde_json::to_string_pretty(&result)?);

        Ok(())
    }
}
