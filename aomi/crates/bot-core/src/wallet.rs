//! Wallet connection service for binding wallets to chat sessions.

use alloy::primitives::{Address, Signature};
use async_trait::async_trait;
use sqlx::{Any, Pool};
use tracing::{debug, info};

use crate::error::{BotError, BotResult};

/// Prefix for EIP-191 personal sign messages.
const EIP191_PREFIX: &str = "\x19Ethereum Signed Message:\n";

/// Service for managing wallet connections.
#[async_trait]
pub trait WalletConnectService: Send + Sync {
    /// Generate a challenge message for the user to sign.
    async fn generate_challenge(&self, session_key: &str) -> BotResult<String>;

    /// Verify a signature and bind the wallet to the session.
    async fn verify_and_bind(&self, session_key: &str, signature: &str) -> BotResult<Address>;

    /// Get the wallet bound to a session, if any.
    async fn get_bound_wallet(&self, session_key: &str) -> BotResult<Option<String>>;

    /// Disconnect (unbind) the wallet from a session.
    async fn disconnect(&self, session_key: &str) -> BotResult<()>;
}

/// Database-backed wallet connect service.
pub struct DbWalletConnectService {
    pool: Pool<Any>,
}

impl DbWalletConnectService {
    pub fn new(pool: Pool<Any>) -> Self {
        Self { pool }
    }

    fn generate_nonce() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.r#gen();
        hex::encode(bytes)
    }

    fn build_challenge(session_key: &str, nonce: &str) -> String {
        format!("Connect to Aomi\n\nSession: {}\nNonce: {}", session_key, nonce)
    }

    fn eip191_hash(message: &str) -> [u8; 32] {
        use alloy::primitives::keccak256;
        let prefixed = format!("{}{}{}", EIP191_PREFIX, message.len(), message);
        keccak256(prefixed.as_bytes()).0
    }

    fn recover_signer(message: &str, signature_hex: &str) -> BotResult<Address> {
        let sig_bytes = hex::decode(signature_hex.trim_start_matches("0x"))
            .map_err(|e| BotError::Wallet(format!("Invalid signature hex: {}", e)))?;

        if sig_bytes.len() != 65 {
            return Err(BotError::Wallet(format!(
                "Invalid signature length: expected 65, got {}",
                sig_bytes.len()
            )));
        }

        let sig = Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| BotError::Wallet(format!("Invalid signature: {}", e)))?;

        let hash = Self::eip191_hash(message);
        
        sig.recover_address_from_prehash(&hash.into())
            .map_err(|e| BotError::Wallet(format!("Failed to recover address: {}", e)))
    }
}

#[async_trait]
impl WalletConnectService for DbWalletConnectService {
    async fn generate_challenge(&self, session_key: &str) -> BotResult<String> {
        let nonce = Self::generate_nonce();
        let challenge = Self::build_challenge(session_key, &nonce);

        sqlx::query(
            "INSERT INTO wallet_challenges (session_key, nonce, created_at) VALUES ($1, $2, NOW()) ON CONFLICT (session_key) DO UPDATE SET nonce = $2, created_at = NOW()",
        )
        .bind(session_key)
        .bind(&nonce)
        .execute(&self.pool)
        .await
        .map_err(|e| BotError::Database(e.to_string()))?;

        debug!("Generated challenge for session {}", session_key);
        Ok(challenge)
    }

    async fn verify_and_bind(&self, session_key: &str, signature: &str) -> BotResult<Address> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT nonce FROM wallet_challenges WHERE session_key = $1",
        )
        .bind(session_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BotError::Database(e.to_string()))?;

        let nonce = row
            .ok_or_else(|| BotError::Wallet("No pending challenge. Use /connect first.".into()))?
            .0;

        let challenge = Self::build_challenge(session_key, &nonce);
        let address = Self::recover_signer(&challenge, signature)?;
        let address_str = format!("{:?}", address);

        info!("Verified wallet {} for session {}", address_str, session_key);

        sqlx::query(
            "INSERT INTO user_wallets (session_key, wallet_address, verified_at) VALUES ($1, $2, NOW()) ON CONFLICT (session_key) DO UPDATE SET wallet_address = $2, verified_at = NOW()",
        )
        .bind(session_key)
        .bind(&address_str)
        .execute(&self.pool)
        .await
        .map_err(|e| BotError::Database(e.to_string()))?;

        sqlx::query("DELETE FROM wallet_challenges WHERE session_key = $1")
            .bind(session_key)
            .execute(&self.pool)
            .await
            .map_err(|e| BotError::Database(e.to_string()))?;

        Ok(address)
    }

    async fn get_bound_wallet(&self, session_key: &str) -> BotResult<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT wallet_address FROM user_wallets WHERE session_key = $1",
        )
        .bind(session_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BotError::Database(e.to_string()))?;

        Ok(row.map(|r| r.0))
    }

    async fn disconnect(&self, session_key: &str) -> BotResult<()> {
        sqlx::query("DELETE FROM user_wallets WHERE session_key = $1")
            .bind(session_key)
            .execute(&self.pool)
            .await
            .map_err(|e| BotError::Database(e.to_string()))?;

        info!("Disconnected wallet for session {}", session_key);
        Ok(())
    }
}
