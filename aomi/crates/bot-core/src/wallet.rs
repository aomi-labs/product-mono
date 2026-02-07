//! Wallet connection service for binding wallets to sessions/users.

use alloy::primitives::{Address, Signature};
use async_trait::async_trait;
use sqlx::{Any, Pool};
use tracing::{debug, info};

use crate::error::{BotError, BotResult};

/// Prefix for EIP-191 personal sign messages.
const EIP191_PREFIX: &str = "\x19Ethereum Signed Message:\n";

/// Service for managing wallet connections.
///
/// Challenges are stored per-session in `signup_challenges` table.
/// After verification, the session is linked to a user via `sessions.public_key`.
/// The `users.public_key` IS the wallet address.
#[async_trait]
pub trait WalletConnectService: Send + Sync {
    /// Generate a challenge message for the user to sign.
    async fn generate_challenge(&self, session_id: &str) -> BotResult<String>;

    /// Verify a signature and bind the wallet to the session.
    /// Creates user if needed, links session to user via public_key.
    async fn verify_and_bind(&self, session_id: &str, signature: &str) -> BotResult<Address>;

    /// Get the wallet (public_key) bound to a session, if any.
    async fn get_bound_wallet(&self, session_id: &str) -> BotResult<Option<String>>;

    /// Disconnect (unbind) the wallet from a session.
    async fn disconnect(&self, session_id: &str) -> BotResult<()>;
}

/// Database-backed wallet connect service.
pub struct DbWalletConnectService {
    pool: Pool<Any>,
}

impl DbWalletConnectService {
    pub fn new(pool: Pool<Any>) -> Self {
        Self { pool }
    }

    #[allow(dead_code)]
    fn generate_nonce() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.r#gen();
        hex::encode(bytes)
    }

    #[allow(dead_code)]
    fn build_challenge(session_key: &str, nonce: &str) -> String {
        format!(
            "Connect to Aomi\n\nSession: {}\nNonce: {}",
            session_key, nonce
        )
    }

    #[allow(dead_code)]
    fn eip191_hash(message: &str) -> [u8; 32] {
        use alloy::primitives::keccak256;
        let prefixed = format!("{}{}{}", EIP191_PREFIX, message.len(), message);
        keccak256(prefixed.as_bytes()).0
    }

    #[allow(dead_code)]
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
        let _ = session_key;
        Err(BotError::Wallet(
            "Challenge-based wallet connect is deprecated; use the mini-app bind flow."
                .to_string(),
        ))
    }

    async fn verify_and_bind(&self, session_key: &str, signature: &str) -> BotResult<Address> {
        let _ = (session_key, signature);
        Err(BotError::Wallet(
            "Challenge-based wallet connect is deprecated; use the mini-app bind flow."
                .to_string(),
        ))
    }

    async fn get_bound_wallet(&self, session_key: &str) -> BotResult<Option<String>> {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT public_key FROM sessions WHERE id = $1",
        )
        .bind(session_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BotError::Database(e.to_string()))?;

        Ok(row.and_then(|r| r.0))
    }

    async fn disconnect(&self, session_key: &str) -> BotResult<()> {
        sqlx::query("UPDATE sessions SET public_key = NULL WHERE id = $1")
            .bind(session_key)
            .execute(&self.pool)
            .await
            .map_err(|e| BotError::Database(e.to_string()))?;

        info!("Disconnected wallet for session {}", session_key);
        Ok(())
    }
}
