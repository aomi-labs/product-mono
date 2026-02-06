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

    fn generate_nonce() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.r#gen();
        hex::encode(bytes)
    }

    fn build_challenge(session_key: &str, nonce: &str) -> String {
        format!(
            "Connect to Aomi\n\nSession: {}\nNonce: {}",
            session_key, nonce
        )
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
    async fn generate_challenge(&self, session_id: &str) -> BotResult<String> {
        let nonce = Self::generate_nonce();
        let challenge = Self::build_challenge(session_id, &nonce);

        sqlx::query(
            "INSERT INTO signup_challenges (session_id, nonce, created_at) VALUES ($1, $2, NOW()) ON CONFLICT (session_id) DO UPDATE SET nonce = $2, created_at = NOW()",
        )
        .bind(session_id)
        .bind(&nonce)
        .execute(&self.pool)
        .await
        .map_err(|e| BotError::Database(e.to_string()))?;

        debug!("Generated challenge for session {}", session_id);
        Ok(challenge)
    }

    async fn verify_and_bind(&self, session_id: &str, signature: &str) -> BotResult<Address> {
        // Get the challenge nonce
        let row: Option<(String,)> =
            sqlx::query_as("SELECT nonce FROM signup_challenges WHERE session_id = $1")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| BotError::Database(e.to_string()))?;

        let nonce = row
            .ok_or_else(|| BotError::Wallet("No pending challenge. Use /connect first.".into()))?
            .0;

        let challenge = Self::build_challenge(session_id, &nonce);
        let address = Self::recover_signer(&challenge, signature)?;
        let address_str = format!("{:?}", address);

        info!("Verified wallet {} for session {}", address_str, session_id);

        // Create user if needed (public_key IS the wallet address)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO users (public_key, created_at) VALUES ($1, $2) ON CONFLICT (public_key) DO NOTHING",
        )
        .bind(&address_str)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| BotError::Database(e.to_string()))?;

        // Link session to user
        sqlx::query("UPDATE sessions SET public_key = $1 WHERE id = $2")
            .bind(&address_str)
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| BotError::Database(e.to_string()))?;

        // Clean up the challenge
        sqlx::query("DELETE FROM signup_challenges WHERE session_id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| BotError::Database(e.to_string()))?;

        Ok(address)
    }

    async fn get_bound_wallet(&self, session_id: &str) -> BotResult<Option<String>> {
        // Get public_key (wallet address) from session
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT public_key FROM sessions WHERE id = $1")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| BotError::Database(e.to_string()))?;

        Ok(row.and_then(|r| r.0))
    }

    async fn disconnect(&self, session_id: &str) -> BotResult<()> {
        // Unlink session from user (set public_key to NULL)
        sqlx::query("UPDATE sessions SET public_key = NULL WHERE id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| BotError::Database(e.to_string()))?;

        info!("Disconnected wallet for session {}", session_id);
        Ok(())
    }
}
