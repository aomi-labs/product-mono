//! Session key management for chat bots.

use crate::types::Platform;

/// Type of chat session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatType {
    /// Direct message (1:1 conversation)
    Dm,
    /// Group chat
    Group,
    /// Channel (broadcast-style)
    Channel,
    /// Thread within a group or channel
    Thread,
}

impl ChatType {
    /// Returns the chat type identifier used in session keys.
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatType::Dm => "dm",
            ChatType::Group => "group",
            ChatType::Channel => "channel",
            ChatType::Thread => "thread",
        }
    }
}

/// Builder for constructing session keys.
///
/// Session keys follow the format: `{platform}:{chat_type}:{id}`
///
/// # Examples
/// ```
/// use aomi_bot_core::{PlatformKeyBuilder, Platform, ChatType};
///
/// let key = PlatformKeyBuilder::new(Platform::Telegram)
///     .dm("123456789")
///     .build();
/// assert_eq!(key, "telegram:dm:123456789");
///
/// let key = PlatformKeyBuilder::new(Platform::Discord)
///     .group("987654321")
///     .build();
/// assert_eq!(key, "discord:group:987654321");
/// ```
#[derive(Debug, Clone)]
pub struct PlatformKeyBuilder {
    platform: Platform,
    chat_type: Option<ChatType>,
    id: Option<String>,
    thread_id: Option<String>,
}

impl PlatformKeyBuilder {
    /// Create a new session key builder for a platform.
    pub fn new(platform: Platform) -> Self {
        Self {
            platform,
            chat_type: None,
            id: None,
            thread_id: None,
        }
    }

    /// Set as a DM session with the given user ID.
    pub fn dm(mut self, user_id: impl Into<String>) -> Self {
        self.chat_type = Some(ChatType::Dm);
        self.id = Some(user_id.into());
        self
    }

    /// Set as a group session with the given chat ID.
    pub fn group(mut self, chat_id: impl Into<String>) -> Self {
        self.chat_type = Some(ChatType::Group);
        self.id = Some(chat_id.into());
        self
    }

    /// Set as a channel session with the given channel ID.
    pub fn channel(mut self, channel_id: impl Into<String>) -> Self {
        self.chat_type = Some(ChatType::Channel);
        self.id = Some(channel_id.into());
        self
    }

    /// Add a thread ID for thread-specific sessions.
    pub fn thread(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    /// Build the session key string.
    ///
    /// # Panics
    /// Panics if chat type or ID has not been set.
    pub fn build(self) -> String {
        let chat_type = self.chat_type.expect("chat_type must be set");
        let id = self.id.expect("id must be set");

        let base = format!("{}:{}:{}", self.platform.as_str(), chat_type.as_str(), id);

        match self.thread_id {
            Some(thread_id) => format!("{}:thread:{}", base, thread_id),
            None => base,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dm_session_key() {
        let key = PlatformKeyBuilder::new(Platform::Telegram)
            .dm("123456789")
            .build();
        assert_eq!(key, "telegram:dm:123456789");

        let key = PlatformKeyBuilder::new(Platform::Discord)
            .dm("987654321")
            .build();
        assert_eq!(key, "discord:dm:987654321");
    }

    #[test]
    fn test_group_session_key() {
        let key = PlatformKeyBuilder::new(Platform::Telegram)
            .group("-100123456789")
            .build();
        assert_eq!(key, "telegram:group:-100123456789");
    }

    #[test]
    fn test_thread_session_key() {
        let key = PlatformKeyBuilder::new(Platform::Telegram)
            .group("-100123456789")
            .thread("42")
            .build();
        assert_eq!(key, "telegram:group:-100123456789:thread:42");
    }
}
