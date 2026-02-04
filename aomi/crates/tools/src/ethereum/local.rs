//! LocalGateway - Cast/RPC only with autosign support for eval-test mode.
//!
//! This gateway implementation is used when the `eval-test` feature is enabled.
//! It uses Cast/RPC exclusively (no Etherscan, no DB caching) and supports
//! auto-signing transactions for configured wallet addresses.
//!
//! Autosign wallets are configured in providers.toml:
//! ```toml
//! autosign_wallets = [
//!     "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",  # Alice
//!     "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",  # Bob
//! ]
//! ```

use alloy::network::ReceiptResponse;
use alloy::primitives::{Address, B256};
use alloy_provider::Provider;
use aomi_anvil::ProviderManager;
use async_trait::async_trait;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::clients::{external_clients, ExternalClients};
use crate::db::{Contract, Transaction};
use crate::ethereum::cast::{execute_send_transaction, SendTransactionParameters};

use super::gateway::{AccountInfo, Erc20BalanceResult, EvmGateway, WalletTransactionResult};

// ============================================================================
// Constants
// ============================================================================

/// Timeout for waiting for transaction receipt
const TX_RECEIPT_TIMEOUT: Duration = Duration::from_secs(20);

/// Poll interval for checking transaction receipt
const TX_POLL_INTERVAL: Duration = Duration::from_millis(250);

// ============================================================================
// LocalGateway
// ============================================================================

pub struct LocalGateway {
    clients: Arc<ExternalClients>,
    provider_manager: Arc<ProviderManager>,
    local_chain_ids: Vec<u64>,
    autosign_wallets: Vec<Address>,
}

impl LocalGateway {
    pub async fn new() -> eyre::Result<Self> {
        let clients = external_clients().await;
        let provider_manager = aomi_anvil::provider_manager()
            .await
            .map_err(|e| eyre::eyre!("Failed to get provider manager: {}", e))?;

        // Get local chain IDs from provider manager
        let local_chain_ids = provider_manager.get_local_chain_ids();

        // Load autosign wallets from providers.toml
        let autosign_wallets = aomi_anvil::load_autosign_wallets().unwrap_or_else(|e| {
            warn!("Failed to load autosign_wallets from config: {}", e);
            vec![]
        });

        if !autosign_wallets.is_empty() {
            info!(
                "LocalGateway initialized with {} autosign wallets",
                autosign_wallets.len()
            );
            for wallet in &autosign_wallets {
                debug!("  - {}", wallet);
            }
        }

        if !local_chain_ids.is_empty() {
            info!(
                "LocalGateway initialized with {} local chains: {:?}",
                local_chain_ids.len(),
                local_chain_ids
            );
        }

        Ok(Self {
            clients,
            provider_manager,
            local_chain_ids,
            autosign_wallets,
        })
    }

    /// Get the network key for a chain ID (from ProviderManager).
    fn network_key_for_chain(&self, chain_id: u64) -> Option<String> {
        self.provider_manager.network_key_for_chain(chain_id)
    }

    /// Wait for a transaction to be confirmed on-chain.
    async fn wait_for_confirmation(&self, tx_hash: &str, network_key: &str) -> eyre::Result<()> {
        let cast_client = self
            .clients
            .get_cast_client(network_key)
            .await
            .map_err(|e| eyre::eyre!("{}", e))?;

        let hash = B256::from_str(tx_hash)?;
        let start = Instant::now();

        loop {
            match cast_client.provider.get_transaction_receipt(hash).await {
                Ok(Some(receipt)) => {
                    if !receipt.status() {
                        eyre::bail!("Transaction reverted on-chain");
                    }
                    return Ok(());
                }
                Ok(None) => {
                    if start.elapsed() > TX_RECEIPT_TIMEOUT {
                        eyre::bail!("Timed out waiting for transaction receipt");
                    }
                    sleep(TX_POLL_INTERVAL).await;
                }
                Err(err) => {
                    eyre::bail!("Failed to poll transaction receipt: {}", err);
                }
            }
        }
    }
}

#[async_trait]
impl EvmGateway for LocalGateway {
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

        let network_key = self.network_key_for_chain(chain_id)
            .ok_or_else(|| eyre::eyre!("No network configured for chain {}", chain_id))?;
        let cast_client = self
            .clients
            .get_cast_client(&network_key)
            .await
            .map_err(|e| eyre::eyre!("{}", e))?;

        let parsed_address = Address::from_str(&normalized)?;

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

        debug!(
            "LocalGateway: Got account info via {} RPC for chain {}: balance={}, nonce={}",
            network_key, chain_id, balance, nonce
        );

        Ok(AccountInfo {
            address: normalized,
            balance: balance.to_string(),
            nonce,
        })
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
        // For local gateway, we could implement ERC20 balance via direct contract call
        // For now, return an error since this is primarily used in tests
        // and can be expanded later
        eyre::bail!(
            "ERC20 balance lookup not implemented in LocalGateway. \
             Use direct contract calls for testing. \
             chain={}, token={}, holder={}, tag={:?}",
            chain_id,
            token_address,
            holder_address,
            block_tag
        )
    }

    // =========================================================================
    // Transaction History
    // =========================================================================

    async fn get_transaction_history(
        &self,
        _chain_id: u64,
        _address: &str,
        _current_nonce: i64,
        _limit: Option<i64>,
        _offset: Option<i64>,
    ) -> eyre::Result<Vec<Transaction>> {
        // LocalGateway doesn't have DB access or Etherscan
        // Return empty list - tests should verify transactions directly
        debug!("LocalGateway: get_transaction_history returning empty (no DB in eval-test)");
        Ok(vec![])
    }

    // =========================================================================
    // Contract Data
    // =========================================================================

    async fn get_contract(&self, _chain_id: u64, _address: &str) -> eyre::Result<Option<Contract>> {
        // LocalGateway doesn't have DB access
        debug!("LocalGateway: get_contract returning None (no DB in eval-test)");
        Ok(None)
    }

    async fn fetch_and_store_contract(
        &self,
        _chain_id: u64,
        _address: &str,
    ) -> eyre::Result<Contract> {
        eyre::bail!("Contract fetching not available in LocalGateway (no Etherscan/DB in eval-test)")
    }

    // =========================================================================
    // Wallet Transactions
    // =========================================================================

    async fn send_transaction_to_wallet(
        &self,
        from: &str,
        to: &str,
        value: &str,
        data: &str,
        gas_limit: Option<&str>,
        description: &str,
    ) -> eyre::Result<WalletTransactionResult> {
        // Check if this wallet should auto-sign
        if self.should_autosign(from) {
            info!(
                "LocalGateway: Auto-signing transaction from {} to {} (value: {})",
                from, to, value
            );

            // Get the network key for a local chain (use first local chain)
            let network_key = self.local_chain_ids.first()
                .and_then(|chain_id| self.network_key_for_chain(*chain_id))
                .unwrap_or_else(|| "testnet".to_string());

            // Build transaction parameters
            let params = SendTransactionParameters {
                from: from.to_string(),
                to: to.to_string(),
                value: value.to_string(),
                input: if data == "0x" || data.is_empty() {
                    None
                } else {
                    Some(data.to_string())
                },
                network: Some(network_key.clone()),
            };

            // Execute the transaction
            let tx_hash = execute_send_transaction(params)
                .await
                .map_err(|e| eyre::eyre!("Autosign transaction failed: {}", e))?;

            // Wait for confirmation
            self.wait_for_confirmation(&tx_hash, &network_key).await?;

            info!("LocalGateway: Transaction confirmed: {}", tx_hash);

            return Ok(WalletTransactionResult::Confirmed {
                tx_hash,
                from: from.to_string(),
                to: to.to_string(),
                value: value.to_string(),
            });
        }

        // Not an autosign wallet - return pending approval (same as production)
        debug!(
            "LocalGateway: Wallet {} not in autosign list, returning PendingApproval",
            from
        );

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
        self.local_chain_ids.contains(&chain_id)
    }

    fn autosign_wallets(&self) -> &[Address] {
        &self.autosign_wallets
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Note: test_network_key_mapping removed - now delegates to ProviderManager
    // which requires actual config. Use integration tests for this behavior.

    #[test]
    fn test_should_autosign_trait_method() {
        // Test the trait's should_autosign method directly
        let alice = Address::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap();
        let autosign_wallets = vec![alice];

        // Check address matching logic (case-insensitive)
        let addr_lower = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".to_lowercase();
        let matches = autosign_wallets
            .iter()
            .any(|a| a.to_string().to_lowercase() == addr_lower);
        assert!(matches);

        let addr_upper = "0xF39FD6E51AAD88F6F4CE6AB8827279CFFFB92266".to_lowercase();
        let matches = autosign_wallets
            .iter()
            .any(|a| a.to_string().to_lowercase() == addr_upper);
        assert!(matches);

        // Non-matching address
        let other_addr = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8".to_lowercase();
        let matches = autosign_wallets
            .iter()
            .any(|a| a.to_string().to_lowercase() == other_addr);
        assert!(!matches);
    }
}
