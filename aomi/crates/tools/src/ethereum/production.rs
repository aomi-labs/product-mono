//! ProductionGateway - Etherscan-first with Cast/RPC fallback.
//!
//! This gateway implementation is used in production builds. It prioritizes
//! Etherscan for account data (to get real-time mainnet data matching the user's
//! wallet), with fallback to Cast/RPC when Etherscan is unavailable.
//!
//! For local chains (configured with `local = true` in providers.toml),
//! it always uses Cast/RPC since Etherscan doesn't support them.

use alloy::primitives::Address;
use alloy_provider::Provider;
use aomi_anvil::ProviderManager;
use async_trait::async_trait;
use sqlx::any::AnyPoolOptions;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::clients::{external_clients, ExternalClients};
use crate::db::{Contract, ContractStore, ContractStoreApi, Transaction, TransactionStore, TransactionStoreApi, TransactionRecord};
use crate::etherscan::{self, EtherscanClient};

use super::gateway::{AccountInfo, Erc20BalanceResult, EvmGateway, WalletTransactionResult};

// ============================================================================
// ProductionGateway
// ============================================================================

pub struct ProductionGateway {
    clients: Arc<ExternalClients>,
    provider_manager: Arc<ProviderManager>,
}

impl ProductionGateway {
    pub async fn new() -> eyre::Result<Self> {
        let clients = external_clients().await;
        let provider_manager = aomi_anvil::provider_manager()
            .await
            .map_err(|e| eyre::eyre!("Failed to get provider manager: {}", e))?;

        Ok(Self {
            clients,
            provider_manager,
        })
    }

    /// Get the network key for a chain ID (from ProviderManager).
    fn network_key_for_chain(&self, chain_id: u64) -> Option<String> {
        self.provider_manager.network_key_for_chain(chain_id)
    }

    /// Get account info via Etherscan API.
    async fn account_info_via_etherscan(
        &self,
        client: &EtherscanClient,
        chain_id: u64,
        address: &str,
    ) -> eyre::Result<AccountInfo> {
        let balance = client
            .get_account_balance(chain_id as u32, address)
            .await
            .map_err(|e| eyre::eyre!("{}", e))?;

        let nonce_u64 = client
            .get_transaction_count(chain_id as u32, address)
            .await
            .map_err(|e| eyre::eyre!("{}", e))?;

        let nonce = i64::try_from(nonce_u64)?;

        Ok(AccountInfo {
            address: address.to_string(),
            balance,
            nonce,
        })
    }

    /// Get account info via Cast/RPC.
    async fn account_info_via_cast(
        &self,
        chain_id: u64,
        address: &str,
    ) -> eyre::Result<AccountInfo> {
        let network_key = self
            .network_key_for_chain(chain_id)
            .ok_or_else(|| eyre::eyre!("No network key for chain {}", chain_id))?;

        let cast_client = self
            .clients
            .get_cast_client(&network_key)
            .await
            .map_err(|e| eyre::eyre!("{}", e))?;

        let parsed_address = Address::from_str(address)?;

        let balance = cast_client
            .provider
            .get_balance(parsed_address)
            .await
            .map_err(|e| eyre::eyre!("Failed to fetch balance via RPC: {}", e))?;

        let nonce_u64 = cast_client
            .provider
            .get_transaction_count(parsed_address)
            .await
            .map_err(|e| eyre::eyre!("Failed to fetch nonce via RPC: {}", e))?;

        let nonce = i64::try_from(nonce_u64)?;

        Ok(AccountInfo {
            address: address.to_string(),
            balance: balance.to_string(),
            nonce,
        })
    }

    /// Get a database connection pool.
    async fn get_db_pool(&self) -> eyre::Result<sqlx::AnyPool> {
        sqlx::any::install_default_drivers();
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://aomi@localhost:5432/chatbot".to_string());

        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        Ok(pool)
    }
}

#[async_trait]
impl EvmGateway for ProductionGateway {
    // =========================================================================
    // Account Operations
    // =========================================================================

    async fn get_account_info(&self, chain_id: u64, address: &str) -> eyre::Result<AccountInfo> {
        let normalized = address.to_lowercase();

        // Validate chain is supported
        if !self.is_supported(chain_id) {
            eyre::bail!(
                "Chain {} is not supported. Supported chains: {:?}",
                chain_id,
                self.supported_chains()
            );
        }

        // Local chains: Always use Cast (Etherscan doesn't support them)
        if self.is_local_chain(chain_id) {
            debug!("Using Cast/RPC for local chain {} (address={})", chain_id, normalized);
            return self.account_info_via_cast(chain_id, &normalized).await;
        }

        // Public chains: Try Etherscan first, fallback to Cast
        if let Some(etherscan) = self.clients.etherscan_client() {
            match self
                .account_info_via_etherscan(&etherscan, chain_id, &normalized)
                .await
            {
                Ok(info) => {
                    debug!(
                        "Got account info via Etherscan: balance={}, nonce={}",
                        info.balance, info.nonce
                    );
                    return Ok(info);
                }
                Err(e) => {
                    warn!(
                        "Etherscan failed for chain {} ({}), falling back to RPC: {}",
                        chain_id, normalized, e
                    );
                }
            }
        }

        // Fallback to Cast/RPC
        self.account_info_via_cast(chain_id, &normalized).await
    }

    // =========================================================================
    // ERC20 Operations
    // =========================================================================

    async fn get_erc20_balance(
        &self,
        chain_id: u64,
        token_address: &str,
        holder_address: &str,
        block_tag: Option<&str>,
    ) -> eyre::Result<Erc20BalanceResult> {
        let normalized_token = token_address.to_lowercase();
        let normalized_holder = holder_address.to_lowercase();
        let tag = block_tag.unwrap_or("latest").to_lowercase();

        // Validate chain is supported
        if !self.is_supported(chain_id) {
            eyre::bail!("Chain {} is not supported", chain_id);
        }

        // Try Etherscan first
        if let Some(etherscan) = self.clients.etherscan_client() {
            if !self.is_local_chain(chain_id) {
                match etherscan
                    .get_erc20_balance(
                        chain_id as u32,
                        &normalized_token,
                        &normalized_holder,
                        Some(&tag),
                    )
                    .await
                {
                    Ok(balance) => {
                        return Ok(Erc20BalanceResult {
                            chain_id,
                            token_address: normalized_token,
                            holder_address: normalized_holder,
                            balance,
                            block_tag: tag,
                        });
                    }
                    Err(e) => {
                        warn!(
                            "Etherscan ERC20 balance failed for chain {}: {}, falling back to RPC",
                            chain_id, e
                        );
                    }
                }
            }
        }

        // Fallback to Cast/RPC - use the existing implementation from etherscan.rs
        // For now, return an error if Etherscan fails since Cast ERC20 balance
        // requires more complex setup
        eyre::bail!(
            "ERC20 balance lookup failed: Etherscan unavailable and RPC fallback not implemented for production"
        )
    }

    // =========================================================================
    // Transaction History
    // =========================================================================

    async fn get_transaction_history(
        &self,
        chain_id: u64,
        address: &str,
        current_nonce: i64,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> eyre::Result<Vec<Transaction>> {
        let normalized = address.to_lowercase();

        // Validate chain is supported
        if !self.is_supported(chain_id) {
            eyre::bail!("Chain {} is not supported", chain_id);
        }

        let pool = self.get_db_pool().await?;
        let store = TransactionStore::new(pool);

        // Check if we have cached data and if it's fresh
        let existing_record = store
            .get_transaction_record(chain_id as u32, normalized.clone())
            .await
            .map_err(|e| eyre::eyre!("{}", e))?;

        let should_fetch = match &existing_record {
            Some(record) => match record.nonce {
                Some(db_nonce) => {
                    debug!("Comparing nonces: current={}, db={}", current_nonce, db_nonce);
                    current_nonce > db_nonce
                }
                None => true,
            },
            None => true,
        };

        if should_fetch {
            debug!("Fetching fresh transactions from Etherscan");

            let etherscan_txs = etherscan::fetch_transaction_history(normalized.clone(), chain_id as u32)
                .await
                .map_err(|e| eyre::eyre!("{}", e))?;

            debug!("Fetched {} transactions from Etherscan", etherscan_txs.len());

            // Get the last block number from fetched transactions
            let last_block_number = etherscan_txs
                .first()
                .and_then(|tx| tx.block_number.parse::<i64>().ok());

            // Create initial record (required for foreign key constraint)
            let initial_record = TransactionRecord {
                chain_id: chain_id as u32,
                address: normalized.clone(),
                nonce: Some(current_nonce),
                last_fetched_at: Some(chrono::Utc::now().timestamp()),
                last_block_number,
                total_transactions: None,
            };
            store.upsert_transaction_record(initial_record).await
                .map_err(|e| eyre::eyre!("{}", e))?;

            // Store transactions
            for etherscan_tx in etherscan_txs {
                let db_tx = crate::db::Transaction {
                    id: None,
                    chain_id: chain_id as u32,
                    address: normalized.clone(),
                    hash: etherscan_tx.hash,
                    block_number: etherscan_tx.block_number.parse()?,
                    timestamp: etherscan_tx.timestamp.parse()?,
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
                store.store_transaction(db_tx).await
                    .map_err(|e| eyre::eyre!("{}", e))?;
            }

            // Update record with total count
            let total_transactions = store
                .get_transaction_count(chain_id as u32, normalized.clone())
                .await
                .map_err(|e| eyre::eyre!("{}", e))?;

            let record = TransactionRecord {
                chain_id: chain_id as u32,
                address: normalized.clone(),
                nonce: Some(current_nonce),
                last_fetched_at: Some(chrono::Utc::now().timestamp()),
                last_block_number,
                total_transactions: Some(total_transactions as i32),
            };
            store.upsert_transaction_record(record).await
                .map_err(|e| eyre::eyre!("{}", e))?;
        }

        // Return transactions from DB
        let transactions = store
            .get_transactions(chain_id as u32, normalized, limit, offset)
            .await
            .map_err(|e| eyre::eyre!("{}", e))?;

        Ok(transactions)
    }

    // =========================================================================
    // Contract Data
    // =========================================================================

    async fn get_contract(&self, chain_id: u64, address: &str) -> eyre::Result<Option<Contract>> {
        let normalized = address.to_lowercase();
        let pool = self.get_db_pool().await?;
        let store = ContractStore::new(pool);

        store.get_contract(chain_id as u32, normalized).await
            .map_err(|e| eyre::eyre!("{}", e))
    }

    async fn fetch_and_store_contract(
        &self,
        chain_id: u64,
        address: &str,
    ) -> eyre::Result<Contract> {
        let normalized = address.to_lowercase();
        let pool = self.get_db_pool().await?;
        let store = ContractStore::new(pool);

        etherscan::fetch_and_store_contract(chain_id as u32, normalized, &store).await
            .map_err(|e| eyre::eyre!("{}", e))
    }

    // =========================================================================
    // Wallet Transactions
    // =========================================================================

    async fn send_transaction_to_wallet(
        &self,
        _from: &str,
        to: &str,
        value: &str,
        data: &str,
        gas_limit: Option<&str>,
        description: &str,
    ) -> eyre::Result<WalletTransactionResult> {
        // Production gateway always returns PendingApproval
        // The frontend will handle showing this to the user's wallet
        Ok(WalletTransactionResult::PendingApproval {
            to: to.to_string(),
            value: value.to_string(),
            data: data.to_string(),
            gas: gas_limit.map(String::from),
            description: description.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    // =========================================================================
    // Chain Configuration
    // =========================================================================

    fn supported_chains(&self) -> Vec<u64> {
        self.provider_manager.supported_chain_ids()
    }

    fn is_local_chain(&self, chain_id: u64) -> bool {
        self.provider_manager.is_local_chain(chain_id)
    }

    // No autosign in production
    fn autosign_wallets(&self) -> &[Address] {
        &[]
    }
}
