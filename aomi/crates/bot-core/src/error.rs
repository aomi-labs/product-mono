//! Error types for chat bots.

use thiserror::Error;

/// Result type for bot operations.
pub type BotResult<T> = Result<T, BotError>;

/// Errors that can occur in bot operations.
#[derive(Debug, Error)]
pub enum BotError {
    /// Session-related error.
    #[error("Session error: {0}")]
    Session(String),

    /// Timeout waiting for response.
    #[error("Timeout waiting for response")]
    Timeout,

    /// Empty response from LLM.
    #[error("No response generated")]
    EmptyResponse,

    /// Message sending failed.
    #[error("Failed to send message: {0}")]
    SendFailed(String),

    /// Platform API error.
    #[error("Platform error: {0}")]
    Platform(String),

    /// Policy violation (user not allowed).
    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),
}
