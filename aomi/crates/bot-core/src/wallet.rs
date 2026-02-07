//! Wallet connection service for binding wallets to sessions/users.

use alloy::primitives::Address;
use async_trait::async_trait;

use crate::error::BotResult;

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
