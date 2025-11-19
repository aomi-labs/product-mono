#[cfg(any(test, feature = "eval-test"))]
use crate::clients::ExternalClients;
use crate::clients::external_clients;
use crate::db::{TransactionRecord, TransactionStore, TransactionStoreApi};
use crate::etherscan;
#[cfg(any(test, feature = "eval-test"))]
use crate::etherscan::{EtherscanClient, Network};
#[cfg(any(test, feature = "eval-test"))]
use alloy::primitives::Address;
#[cfg(any(test, feature = "eval-test"))]
use alloy_provider::Provider;
use chrono;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::any::AnyPoolOptions;
use std::future::Future;
#[cfg(any(test, feature = "eval-test"))]
use std::str::FromStr;
use tokio::task;
#[cfg(any(test, feature = "eval-test"))]
use tracing::warn;
use tracing::{debug, error, info};

#[cfg(any(test, feature = "eval-test"))]
const TESTNET_NETWORK_KEY: &str = "testnet";
#[cfg(any(test, feature = "eval-test"))]
const LOCAL_CHAIN_IDS: [u32; 2] = [1337, 31337];

/// Tool for getting account information (balance and nonce) from Etherscan
#[derive(Debug, Clone)]
pub struct GetAccountInfo;

/// Tool for getting transaction history with smart database caching
#[derive(Debug, Clone)]
pub struct GetAccountTransactionHistory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAccountInfoArgs {
    /// One-line note on what info is being requested for
    pub topic: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAccountTransactionHistoryArgs {
    /// One-line note on what this transaction review covers
    pub topic: String,
    pub address: String,
    pub chain_id: u32,
    pub current_nonce: i64,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

fn run_sync<F, T>(future: F) -> Result<T, ToolError>
where
    F: Future<Output = Result<T, ToolError>> + Send + 'static,
    T: Send + 'static,
{
    task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

#[cfg(not(any(test, feature = "eval-test")))]
pub async fn execute_get_account_info(
    args: GetAccountInfoArgs,
) -> Result<serde_json::Value, ToolError> {
    info!("get_account_info tool called with args: {:?}", args);
    run_sync(async move {
        let address = args.address;
        let chain_id = args.chain_id;
        info!(
            "get_account_info called with address={}, chain_id={}",
            address, chain_id
        );

        let client = external_clients().await.etherscan_client().ok_or_else(|| {
            ToolError::ToolCallError("ETHERSCAN_API_KEY environment variable not set".into())
        })?;

        let normalized_address = address.to_lowercase();

        debug!("Fetching balance from Etherscan");
        let balance = client
            .get_account_balance(chain_id, &normalized_address)
            .await
            .map_err(|e| {
                ToolError::ToolCallError(format!("Failed to fetch balance: {}", e).into())
            })?;

        debug!("Fetching nonce from Etherscan");
        let nonce_u64 = client
            .get_transaction_count(chain_id, &normalized_address)
            .await
            .map_err(|e| {
                ToolError::ToolCallError(format!("Failed to fetch nonce: {}", e).into())
            })?;

        let nonce = i64::try_from(nonce_u64).map_err(|e| {
            ToolError::ToolCallError(format!("Failed to convert nonce to i64: {}", e).into())
        })?;

        info!(
            "Successfully fetched account info: balance={} wei, nonce={}",
            balance, nonce
        );

        let response = json!({
            "address": normalized_address,
            "balance": balance,
            "nonce": nonce,
        });

        info!("get_account_info succeeded");
        Ok(response)
    })
}

#[cfg(any(test, feature = "eval-test"))]
pub async fn execute_get_account_info(
    args: GetAccountInfoArgs,
) -> Result<serde_json::Value, ToolError> {
    info!("get_account_info tool called with args: {:?}", args);
    run_sync(async move {
        let address = args.address;
        let chain_id = args.chain_id;
        info!(
            "get_account_info called with address={}, chain_id={}",
            address, chain_id
        );

        let clients = external_clients().await;

        let normalized_address = address.to_lowercase();

        let account_info = if should_use_local_testnet(chain_id) {
            info!(
                "Using local RPC fallback for chain {} (address={})",
                chain_id, normalized_address
            );
            account_info_via_cast(
                clients.as_ref(),
                TESTNET_NETWORK_KEY,
                &normalized_address,
                chain_id,
            )
            .await?
        } else {
            match clients.etherscan_client() {
                Some(client) => {
                    match account_info_via_etherscan(client.clone(), chain_id, &normalized_address)
                        .await
                    {
                        Ok(info) => info,
                        Err(err) => {
                            if let Some(network_key) = network_key_for_chain(chain_id) {
                                warn!(
                                    "Etherscan lookup failed for chain {} ({}). Falling back to {} RPC: {}",
                                    chain_id, normalized_address, network_key, err
                                );
                                account_info_via_cast(
                                    clients.as_ref(),
                                    &network_key,
                                    &normalized_address,
                                    chain_id,
                                )
                                .await?
                            } else {
                                return Err(err);
                            }
                        }
                    }
                }
                None => {
                    let network_key = network_key_for_chain(chain_id).ok_or_else(|| {
                        ToolError::ToolCallError(
                            "ETHERSCAN_API_KEY environment variable not set".into(),
                        )
                    })?;
                    info!(
                        "No Etherscan client available; using {} RPC for chain {}",
                        network_key, chain_id
                    );
                    account_info_via_cast(
                        clients.as_ref(),
                        &network_key,
                        &normalized_address,
                        chain_id,
                    )
                    .await?
                }
            }
        };

        info!(
            "Successfully fetched account info: balance={} wei, nonce={}",
            account_info.balance, account_info.nonce
        );

        let response = json!({
            "address": account_info.address,
            "balance": account_info.balance,
            "nonce": account_info.nonce,
        });

        info!("get_account_info succeeded");
        Ok(response)
    })
}

#[cfg(any(test, feature = "eval-test"))]
fn should_use_local_testnet(chain_id: u32) -> bool {
    LOCAL_CHAIN_IDS.contains(&chain_id)
}

#[cfg(any(test, feature = "eval-test"))]
fn network_key_for_chain(chain_id: u32) -> Option<String> {
    if should_use_local_testnet(chain_id) {
        return Some(TESTNET_NETWORK_KEY.to_string());
    }

    Network::try_from(chain_id)
        .ok()
        .map(|network| match network {
            Network::Mainnet => "ethereum".to_string(),
            other => other.canonical_name().to_string(),
        })
}

#[cfg(any(test, feature = "eval-test"))]
async fn account_info_via_etherscan(
    client: EtherscanClient,
    chain_id: u32,
    normalized_address: &str,
) -> Result<AccountInfo, ToolError> {
    debug!("Fetching balance from Etherscan");
    let balance = client
        .get_account_balance(chain_id, normalized_address)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Failed to fetch balance: {}", e).into()))?;

    debug!("Fetching nonce from Etherscan");
    let nonce_u64 = client
        .get_transaction_count(chain_id, normalized_address)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Failed to fetch nonce: {}", e).into()))?;

    let nonce = i64::try_from(nonce_u64).map_err(|e| {
        ToolError::ToolCallError(format!("Failed to convert nonce to i64: {}", e).into())
    })?;

    Ok(AccountInfo {
        address: normalized_address.to_string(),
        balance,
        nonce,
    })
}

#[cfg(any(test, feature = "eval-test"))]
async fn account_info_via_cast(
    clients: &ExternalClients,
    network_key: &str,
    normalized_address: &str,
    chain_id: u32,
) -> Result<AccountInfo, ToolError> {
    let cast_client = clients.get_cast_client(network_key).await?;
    let parsed_address = Address::from_str(normalized_address).map_err(|e| {
        ToolError::ToolCallError(format!("Invalid address '{normalized_address}': {e}").into())
    })?;

    let balance = cast_client
        .provider
        .get_balance(parsed_address)
        .await
        .map_err(|e| {
            ToolError::ToolCallError(
                format!(
                    "Failed to fetch balance via {} (chain {}): {}",
                    network_key, chain_id, e
                )
                .into(),
            )
        })?;

    let nonce_u64 = cast_client
        .provider
        .get_transaction_count(parsed_address)
        .await
        .map_err(|e| {
            ToolError::ToolCallError(
                format!(
                    "Failed to fetch nonce via {} (chain {}): {}",
                    network_key, chain_id, e
                )
                .into(),
            )
        })?;

    let nonce = i64::try_from(nonce_u64).map_err(|e| {
        ToolError::ToolCallError(format!("Failed to convert nonce to i64: {}", e).into())
    })?;

    info!(
        "Fetched account info via {} RPC for chain {}: balance={} nonce={}",
        network_key, chain_id, balance, nonce
    );

    Ok(AccountInfo {
        address: normalized_address.to_string(),
        balance: balance.to_string(),
        nonce,
    })
}

pub async fn execute_get_account_transaction_history(
    args: GetAccountTransactionHistoryArgs,
) -> Result<serde_json::Value, ToolError> {
    info!(
        "get_account_transaction_history tool called with args: {:?}",
        args
    );

    run_sync(async move {
        let GetAccountTransactionHistoryArgs {
            topic: _,
            address,
            chain_id,
            current_nonce,
            limit,
            offset,
        } = args;

        let address = address.to_lowercase();

        info!(
            "get_account_transaction_history called with address={}, chain_id={}, nonce={}",
            address, chain_id, current_nonce
        );

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
            Some(record) => match record.nonce {
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
            },
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
                    ToolError::ToolCallError(
                        format!("Failed to fetch from Etherscan: {}", e).into(),
                    )
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
                        ToolError::ToolCallError(
                            format!("Failed to parse block number: {}", e).into(),
                        )
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
                    ToolError::ToolCallError(
                        format!("Failed to get transaction count: {}", e).into(),
                    )
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
                ToolError::ToolCallError(
                    format!("Failed to update transaction record: {}", e).into(),
                )
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
    })
}

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
            topic: "Check Uniswap account status".to_string(),
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
            topic: "Prepare to fetch tx history".to_string(),
            address: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            chain_id: crate::etherscan::ETHEREUM_MAINNET,
        };

        let account_result = execute_get_account_info(account_args).await?;
        let nonce = account_result["nonce"].as_i64().unwrap();

        // Then get transaction history
        let tx_args = GetAccountTransactionHistoryArgs {
            topic: "Fetch last 5 txs".to_string(),
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
