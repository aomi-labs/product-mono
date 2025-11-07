use crate::db::{TransactionRecord, TransactionStore, TransactionStoreApi};
use crate::etherscan::{self, EtherscanResponse};
use chrono;
use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::any::AnyPoolOptions;
use tracing::{debug, error, info};

// JSON-RPC response format used by Etherscan proxy endpoints
#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: i32,
    result: T,
}

// ============================================================================
// GetAccountInfo Tool
// ============================================================================

/// Tool for getting account information (balance and nonce) from Etherscan
#[derive(Debug, Clone)]
pub struct GetAccountInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAccountInfoArgs {
    pub address: String,
    pub chain_id: u32,
}

/// Account information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub address: String,
    pub balance: String,
    pub nonce: i64,
}

impl Tool for GetAccountInfo {
    const NAME: &'static str = "get_account_info";

    type Error = ToolError;
    type Args = GetAccountInfoArgs;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        info!("GetAccountInfo::definition called");
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetches account information (balance and nonce) from Etherscan API. The balance is returned in wei (smallest unit). Use this to check an account's current state.".to_string(),
            parameters: json!({
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
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!("get_account_info tool called with args: {:?}", args);

        let result = tokio::spawn(get_account_info_impl(args.address, args.chain_id))
            .await
            .map_err(|e| {
                let error_msg = format!("Task join error: {}", e);
                error!("{}", error_msg);
                ToolError::ToolCallError(error_msg.into())
            })?;

        match &result {
            Ok(_) => info!("get_account_info succeeded"),
            Err(e) => error!("get_account_info failed: {:?}", e),
        }

        result
    }
}

async fn get_account_info_impl(
    address: String,
    chain_id: u32,
) -> Result<serde_json::Value, ToolError> {
    info!(
        "get_account_info called with address={}, chain_id={}",
        address, chain_id
    );

    let api_key = std::env::var("ETHERSCAN_API_KEY").map_err(|_| {
        ToolError::ToolCallError("ETHERSCAN_API_KEY environment variable not set".into())
    })?;

    // Validate address format
    if !address.starts_with("0x") || address.len() != 42 {
        return Err(ToolError::ToolCallError(
            "Invalid address format. Must be a 42-character hex string starting with 0x".into(),
        ));
    }

    let client = reqwest::Client::new();

    // Fetch balance
    debug!("Fetching balance from Etherscan");
    let balance_response = client
        .get("https://api.etherscan.io/v2/api")
        .query(&[
            ("chainid", chain_id.to_string().as_str()),
            ("module", "account"),
            ("action", "balance"),
            ("address", address.as_str()),
            ("tag", "latest"),
            ("apikey", api_key.as_str()),
        ])
        .send()
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Failed to fetch balance: {}", e).into()))?;

    if !balance_response.status().is_success() {
        return Err(ToolError::ToolCallError(
            format!(
                "Etherscan API request failed with status: {}",
                balance_response.status()
            )
            .into(),
        ));
    }

    let balance_data: EtherscanResponse<String> = balance_response.json().await.map_err(|e| {
        ToolError::ToolCallError(format!("Failed to parse balance response: {}", e).into())
    })?;

    if balance_data.status != "1" {
        return Err(ToolError::ToolCallError(
            format!("Etherscan API error: {}", balance_data.message).into(),
        ));
    }

    // Fetch transaction count (nonce)
    debug!("Fetching nonce from Etherscan");
    let nonce_response = client
        .get("https://api.etherscan.io/v2/api")
        .query(&[
            ("chainid", chain_id.to_string().as_str()),
            ("module", "proxy"),
            ("action", "eth_getTransactionCount"),
            ("address", address.as_str()),
            ("tag", "latest"),
            ("apikey", api_key.as_str()),
        ])
        .send()
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Failed to fetch nonce: {}", e).into()))?;

    if !nonce_response.status().is_success() {
        return Err(ToolError::ToolCallError(
            format!(
                "Etherscan API request failed with status: {}",
                nonce_response.status()
            )
            .into(),
        ));
    }

    // Proxy endpoint returns JSON-RPC format, not standard Etherscan format
    let nonce_data: JsonRpcResponse<String> = nonce_response.json().await.map_err(|e| {
        ToolError::ToolCallError(format!("Failed to parse nonce response: {}", e).into())
    })?;

    // Parse hex nonce to decimal
    let nonce_hex = nonce_data.result.trim_start_matches("0x");
    let nonce = i64::from_str_radix(nonce_hex, 16).map_err(|e| {
        ToolError::ToolCallError(format!("Failed to parse nonce from hex: {}", e).into())
    })?;

    info!(
        "Successfully fetched account info: balance={} wei, nonce={}",
        balance_data.result, nonce
    );

    Ok(json!({
        "address": address.to_lowercase(),
        "balance": balance_data.result,
        "nonce": nonce,
    }))
}

// ============================================================================
// GetAccountTransactionHistory Tool
// ============================================================================

/// Tool for getting transaction history with smart database caching
#[derive(Debug, Clone)]
pub struct GetAccountTransactionHistory;

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

impl Tool for GetAccountTransactionHistory {
    const NAME: &'static str = "get_account_transaction_history";

    type Error = ToolError;
    type Args = GetAccountTransactionHistoryArgs;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        info!("GetAccountTransactionHistory::definition called");
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetches transaction history for an address with smart database caching. Automatically syncs with Etherscan if the nonce is newer than the cached data. Returns transactions ordered by block number (newest first).".to_string(),
            parameters: json!({
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
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(
            "get_account_transaction_history tool called with args: {:?}",
            args
        );

        let result = tokio::spawn(get_account_transaction_history_impl(
            args.address,
            args.chain_id,
            args.current_nonce,
            args.limit,
            args.offset,
        ))
        .await
        .map_err(|e| {
            let error_msg = format!("Task join error: {}", e);
            error!("{}", error_msg);
            ToolError::ToolCallError(error_msg.into())
        })?;

        match &result {
            Ok(_) => info!("get_account_transaction_history succeeded"),
            Err(e) => error!("get_account_transaction_history failed: {:?}", e),
        }

        result
    }
}

async fn get_account_transaction_history_impl(
    address: String,
    chain_id: u32,
    current_nonce: i64,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<serde_json::Value, ToolError> {
    info!(
        "get_account_transaction_history called with address={}, chain_id={}, nonce={}",
        address, chain_id, current_nonce
    );

    let address = address.to_lowercase();

    // Connect to database
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://ceciliazhang@localhost:5432/chatbot".to_string());

    debug!("Connecting to database: {}", database_url);

    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .map_err(|e| {
            let error_msg = format!("Database connection error: {}", e);
            error!("{}", error_msg);
            ToolError::ToolCallError(error_msg.into())
        })?;

    debug!("Database connection successful");

    let store = TransactionStore::new(pool);

    // Get existing transaction record from database
    let existing_record = store
        .get_transaction_record(chain_id, address.clone())
        .await
        .map_err(|e| {
            ToolError::ToolCallError(format!("Failed to get transaction record: {}", e).into())
        })?;

    let should_fetch = match &existing_record {
        Some(record) => {
            // Check if we have a newer nonce
            match record.nonce {
                Some(db_nonce) => {
                    debug!(
                        "Comparing nonces: current={}, db={}",
                        current_nonce, db_nonce
                    );
                    current_nonce > db_nonce
                }
                None => {
                    debug!("No nonce in DB record, fetching");
                    true
                }
            }
        }
        None => {
            debug!("No transaction record exists, fetching");
            true
        }
    };

    if should_fetch {
        info!("Fetching latest transactions from Etherscan");

        // Fetch latest transactions from Etherscan
        let etherscan_txs = etherscan::fetch_transaction_history(address.clone(), chain_id)
            .await
            .map_err(|e| {
                ToolError::ToolCallError(format!("Failed to fetch from Etherscan: {}", e).into())
            })?;

        info!(
            "Fetched {} transactions from Etherscan",
            etherscan_txs.len()
        );

        // Get the last block number from the fetched transactions
        let last_block_number = etherscan_txs
            .first()
            .and_then(|tx| tx.block_number.parse::<i64>().ok());

        // Create or update transaction record FIRST (required for foreign key constraint)
        let initial_record = TransactionRecord {
            chain_id,
            address: address.clone(),
            nonce: Some(current_nonce),
            last_fetched_at: Some(chrono::Utc::now().timestamp()),
            last_block_number,
            total_transactions: None, // Will update after storing transactions
        };

        store
            .upsert_transaction_record(initial_record)
            .await
            .map_err(|e| {
                ToolError::ToolCallError(
                    format!("Failed to create transaction record: {}", e).into(),
                )
            })?;

        debug!("Created/updated transaction record");

        // Convert and store transactions
        for etherscan_tx in etherscan_txs {
            let db_tx = crate::db::Transaction {
                id: None,
                chain_id,
                address: address.clone(),
                hash: etherscan_tx.hash,
                block_number: etherscan_tx.block_number.parse().map_err(|e| {
                    ToolError::ToolCallError(format!("Failed to parse block number: {}", e).into())
                })?,
                timestamp: etherscan_tx.timestamp.parse().map_err(|e| {
                    ToolError::ToolCallError(format!("Failed to parse timestamp: {}", e).into())
                })?,
                from_address: etherscan_tx.from,
                to_address: etherscan_tx.to,
                value: etherscan_tx.value,
                gas: etherscan_tx.gas,
                gas_price: etherscan_tx.gas_price,
                gas_used: etherscan_tx.gas_used,
                is_error: etherscan_tx.is_error,
                input: etherscan_tx.input,
                contract_address: if etherscan_tx.contract_address.is_empty() {
                    None
                } else {
                    Some(etherscan_tx.contract_address)
                },
            };

            store.store_transaction(db_tx).await.map_err(|e| {
                ToolError::ToolCallError(format!("Failed to store transaction: {}", e).into())
            })?;
        }

        debug!("Stored transactions in database");

        // Get total transaction count from DB
        let total_transactions = store
            .get_transaction_count(chain_id, address.clone())
            .await
            .map_err(|e| {
                ToolError::ToolCallError(format!("Failed to get transaction count: {}", e).into())
            })?;

        // Update or create transaction record
        let record = TransactionRecord {
            chain_id,
            address: address.clone(),
            nonce: Some(current_nonce),
            last_fetched_at: Some(chrono::Utc::now().timestamp()),
            last_block_number,
            total_transactions: Some(total_transactions as i32),
        };

        store.upsert_transaction_record(record).await.map_err(|e| {
            ToolError::ToolCallError(format!("Failed to update transaction record: {}", e).into())
        })?;

        info!("Updated transaction record");
    } else {
        info!("Using cached transactions from database");
    }

    // Fetch and return the requested range of transactions from DB
    let transactions = store
        .get_transactions(chain_id, address.clone(), limit, offset)
        .await
        .map_err(|e| {
            ToolError::ToolCallError(format!("Failed to fetch transactions: {}", e).into())
        })?;

    info!("Returning {} transactions", transactions.len());

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_get_account_info_tool() -> Result<(), Box<dyn std::error::Error>> {
        let tool = GetAccountInfo;
        let args = GetAccountInfoArgs {
            address: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            chain_id: crate::etherscan::ETHEREUM_MAINNET,
        };

        let result = tool.call(args).await?;
        println!("Result: {}", serde_json::to_string_pretty(&result)?);

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_get_account_transaction_history_tool() -> Result<(), Box<dyn std::error::Error>> {
        // First get account info
        let account_tool = GetAccountInfo;
        let account_args = GetAccountInfoArgs {
            address: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            chain_id: crate::etherscan::ETHEREUM_MAINNET,
        };

        let account_result = account_tool.call(account_args).await?;
        let nonce = account_result["nonce"].as_i64().unwrap();

        // Then get transaction history
        let tx_tool = GetAccountTransactionHistory;
        let tx_args = GetAccountTransactionHistoryArgs {
            address: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            chain_id: crate::etherscan::ETHEREUM_MAINNET,
            current_nonce: nonce,
            limit: Some(5),
            offset: Some(0),
        };

        let result = tx_tool.call(tx_args).await?;
        println!("Result: {}", serde_json::to_string_pretty(&result)?);

        Ok(())
    }
}
