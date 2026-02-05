//! EvmGateway - Unified abstraction for EVM blockchain data access.
//!
//! This module provides a trait-based abstraction for accessing EVM chain data,
//! with two implementations:
//!
//! - `ProductionGateway`: Etherscan-first with Cast/RPC fallback, includes DB caching
//! - `LocalGateway`: Cast/RPC only, supports autosign for eval-test mode
//!
//! # Usage
//!
//! ```rust,ignore
//! use aomi_tools::ethereum::gateway::get_gateway;
//!
//! let gateway = get_gateway().await;
//! let info = gateway.get_account_info(1, "0x...").await?;
//! ```

use alloy::primitives::Address;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::OnceCell;

use crate::db::{Contract, Transaction};

// ============================================================================
// Core Types
// ============================================================================

/// Account information from the blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub address: String,
    /// Balance in wei (as string to avoid precision loss)
    pub balance: String,
    pub nonce: i64,
}

/// Result of a wallet transaction request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum WalletTransactionResult {
    /// Transaction was auto-signed and confirmed (eval-test mode with autosign wallet)
    #[serde(rename = "confirmed")]
    Confirmed {
        tx_hash: String,
        from: String,
        to: String,
        value: String,
    },
    /// Transaction request pending user approval (production mode or non-autosign wallet)
    #[serde(rename = "pending_approval")]
    PendingApproval {
        to: String,
        value: String,
        data: String,
        gas: Option<String>,
        description: String,
        timestamp: String,
    },
}

/// ERC20 balance result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Erc20BalanceResult {
    pub chain_id: u64,
    pub token_address: String,
    pub holder_address: String,
    pub balance: String,
    pub block_tag: String,
}

// ============================================================================
// EvmGateway Trait
// ============================================================================

/// Unified interface for EVM blockchain data access.
///
/// This trait abstracts over different data sources (Etherscan, RPC, DB cache)
/// and provides a consistent API for tools to access blockchain data.
#[async_trait]
pub trait EvmGateway: Send + Sync {
    // =========================================================================
    // Account Operations
    // =========================================================================

    /// Get account information (balance and nonce) for an address.
    async fn get_account_info(&self, chain_id: u64, address: &str) -> eyre::Result<AccountInfo>;

    // =========================================================================
    // ERC20 Operations
    // =========================================================================

    /// Get ERC20 token balance for a holder address.
    async fn get_erc20_balance(
        &self,
        chain_id: u64,
        token_address: &str,
        holder_address: &str,
        block_tag: Option<&str>,
    ) -> eyre::Result<Erc20BalanceResult>;

    // =========================================================================
    // Transaction History
    // =========================================================================

    /// Get transaction history for an address with smart caching.
    ///
    /// The `current_nonce` is used to determine if cached data is stale.
    async fn get_transaction_history(
        &self,
        chain_id: u64,
        address: &str,
        current_nonce: i64,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> eyre::Result<Vec<Transaction>>;

    // =========================================================================
    // Contract Data
    // =========================================================================

    /// Get a contract from the cache (DB) if available.
    async fn get_contract(&self, chain_id: u64, address: &str) -> eyre::Result<Option<Contract>>;

    /// Fetch a contract from Etherscan and store it in the cache.
    async fn fetch_and_store_contract(
        &self,
        chain_id: u64,
        address: &str,
    ) -> eyre::Result<Contract>;

    // =========================================================================
    // Wallet Transactions
    // =========================================================================

    /// Send a transaction to the user's wallet for signing.
    ///
    /// In production mode, this returns `PendingApproval` for the frontend to handle.
    /// In eval-test mode with an autosign wallet, this executes the transaction
    /// directly and returns `Confirmed`.
    async fn send_transaction_to_wallet(
        &self,
        from: &str,
        to: &str,
        value: &str,
        data: &str,
        gas_limit: Option<&str>,
        description: &str,
    ) -> eyre::Result<WalletTransactionResult>;

    // =========================================================================
    // Chain Configuration
    // =========================================================================

    /// Get all supported chain IDs.
    fn supported_chains(&self) -> Vec<u64>;

    /// Check if a chain ID is supported.
    fn is_supported(&self, chain_id: u64) -> bool {
        self.supported_chains().contains(&chain_id)
    }

    /// Check if a chain ID is a local testnet.
    fn is_local_chain(&self, chain_id: u64) -> bool;

    // =========================================================================
    // Autosign Configuration
    // =========================================================================

    /// Get wallet addresses that should auto-sign transactions.
    ///
    /// Returns an empty slice in production mode.
    fn autosign_wallets(&self) -> &[Address] {
        &[]
    }

    /// Check if an address should auto-sign transactions.
    fn should_autosign(&self, address: &str) -> bool {
        let addr_lower = address.to_lowercase();
        self.autosign_wallets()
            .iter()
            .any(|a| a.to_string().to_lowercase() == addr_lower)
    }
}

// ============================================================================
// Gateway Singleton
// ============================================================================

static GATEWAY: OnceCell<Arc<dyn EvmGateway>> = OnceCell::const_new();

/// Get the global EvmGateway instance.
///
/// The gateway is lazily initialized on first access. The implementation
/// depends on the build configuration:
/// - Default: `ProductionGateway` (Etherscan-first, Cast fallback)
/// - `eval-test` feature: `LocalGateway` (Cast-only, autosign support)
pub async fn get_gateway() -> eyre::Result<Arc<dyn EvmGateway>> {
    GATEWAY
        .get_or_try_init(|| async { create_gateway().await })
        .await
        .map(Arc::clone)
}

#[cfg(not(any(test, feature = "eval-test")))]
async fn create_gateway() -> eyre::Result<Arc<dyn EvmGateway>> {
    use super::production::ProductionGateway;
    Ok(Arc::new(ProductionGateway::new().await?))
}

#[cfg(any(test, feature = "eval-test"))]
async fn create_gateway() -> eyre::Result<Arc<dyn EvmGateway>> {
    use super::local::LocalGateway;
    Ok(Arc::new(LocalGateway::new().await?))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_transaction_result_serialization() {
        let confirmed = WalletTransactionResult::Confirmed {
            tx_hash: "0x123".to_string(),
            from: "0xabc".to_string(),
            to: "0xdef".to_string(),
            value: "1000000000000000000".to_string(),
        };

        let json = serde_json::to_string(&confirmed).unwrap();
        assert!(json.contains("\"status\":\"confirmed\""));
        assert!(json.contains("\"tx_hash\":\"0x123\""));

        let pending = WalletTransactionResult::PendingApproval {
            to: "0xdef".to_string(),
            value: "0".to_string(),
            data: "0x1234".to_string(),
            gas: Some("21000".to_string()),
            description: "Test transaction".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&pending).unwrap();
        assert!(json.contains("\"status\":\"pending_approval\""));
        assert!(json.contains("\"description\":\"Test transaction\""));
    }
}
