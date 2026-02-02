//! Session key helpers for Discord conversations.

use serenity::model::id::{ChannelId, UserId};

/// Builds a session key for a direct message thread.
///
/// Format: `discord:dm:{user_id}`
pub fn dm_session_key(user_id: UserId) -> String {
    format!("discord:dm:{}", user_id.get())
}

/// Builds a session key for a guild channel thread.
///
/// Format: `discord:channel:{channel_id}`
pub fn channel_session_key(channel_id: ChannelId) -> String {
    format!("discord:channel:{}", channel_id.get())
}
