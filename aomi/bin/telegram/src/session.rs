//! Session key helpers for Telegram conversations.

use teloxide::types::{ChatId, Message, UserId};

/// Builds a session key for a direct message thread.
///
/// Format: `telegram:dm:{user_id}`
pub fn dm_session_key(user_id: UserId) -> String {
    format!("telegram:dm:{}", user_id.0)
}

/// Builds a session key for a group chat thread.
///
/// Format: `telegram:group:{chat_id}`
pub fn group_session_key(chat_id: ChatId) -> String {
    format!("telegram:group:{}", chat_id.0)
}

/// Attempts to extract the sender's user id from a message.
///
/// Returns `None` for messages without a sender (e.g. anonymous admin posts).
pub fn user_id_from_message(message: &Message) -> Option<UserId> {
    message.from.as_ref().map(|user| user.id)
}
