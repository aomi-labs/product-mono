//! Session key helpers for Telegram conversations.
//!
//! Re-exports and wraps bot-core session utilities for Telegram-specific types.

use aomi_bot_core::{Platform, PlatformKeyBuilder};
use teloxide::types::{ChatId, Message, UserId};

/// Builds a session key for a direct message thread.
///
/// Format: `telegram:dm:{user_id}`
pub fn dm_session_key(user_id: UserId) -> String {
    PlatformKeyBuilder::new(Platform::Telegram)
        .dm(user_id.0.to_string())
        .build()
}

/// Builds a session key for a group chat thread.
///
/// Format: `telegram:group:{chat_id}`
pub fn group_session_key(chat_id: ChatId) -> String {
    PlatformKeyBuilder::new(Platform::Telegram)
        .group(chat_id.0.to_string())
        .build()
}

/// Builds a session key for a forum topic (thread) within a group.
///
/// Format: `telegram:group:{chat_id}:thread:{thread_id}`
#[allow(dead_code)]
pub fn topic_session_key(chat_id: ChatId, thread_id: i32) -> String {
    PlatformKeyBuilder::new(Platform::Telegram)
        .group(chat_id.0.to_string())
        .thread(thread_id.to_string())
        .build()
}

/// Attempts to extract the sender's user id from a message.
///
/// Returns `None` for messages without a sender (e.g. anonymous admin posts).
pub fn user_id_from_message(message: &Message) -> Option<UserId> {
    message.from.as_ref().map(|user| user.id)
}
