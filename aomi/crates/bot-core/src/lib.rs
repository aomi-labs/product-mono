//! Shared infrastructure for Aomi chat bots (Telegram, Discord, etc.)
//!
//! This crate provides platform-agnostic abstractions for:
//! - Session key management
//! - Message handling and response polling
//! - Policy enforcement (DM/group permissions)
//! - Common types and traits
//!
//! Platform-specific implementations (telegram, discord) build on these primitives.

pub mod error;
pub mod handler;
pub mod policy;
pub mod session;
pub mod types;
pub mod wallet;

pub use error::{BotError, BotResult};
pub use handler::{MessageHandler, ResponsePoller};
pub use policy::{DmPolicy, GroupPolicy, PolicyConfig};
pub use session::{ChatType, SessionKeyBuilder};
pub use types::{BotMessage, BotResponse, Platform};
pub use wallet::{DbWalletConnectService, WalletConnectService};
