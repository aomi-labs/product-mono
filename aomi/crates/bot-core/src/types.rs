//! Common types for chat bots.

use std::fmt;

/// Supported chat platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Telegram,
    Discord,
}

impl Platform {
    /// Returns the platform identifier used in session keys.
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Telegram => "telegram",
            Platform::Discord => "discord",
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A message received from a chat platform.
#[derive(Debug, Clone)]
pub struct BotMessage {
    /// Platform this message came from
    pub platform: Platform,
    /// User ID of the sender
    pub user_id: String,
    /// Chat/channel ID where the message was sent
    pub chat_id: String,
    /// Whether this is a direct message (vs group)
    pub is_dm: bool,
    /// The text content of the message
    pub text: String,
    /// Whether the bot was mentioned (for group messages)
    pub is_mention: bool,
    /// Optional reply-to message ID
    pub reply_to: Option<String>,
}

impl BotMessage {
    /// Create a new DM message.
    pub fn dm(platform: Platform, user_id: impl Into<String>, text: impl Into<String>) -> Self {
        let user_id = user_id.into();
        Self {
            platform,
            chat_id: user_id.clone(),
            user_id,
            is_dm: true,
            text: text.into(),
            is_mention: false,
            reply_to: None,
        }
    }

    /// Create a new group message.
    pub fn group(
        platform: Platform,
        user_id: impl Into<String>,
        chat_id: impl Into<String>,
        text: impl Into<String>,
        is_mention: bool,
    ) -> Self {
        Self {
            platform,
            user_id: user_id.into(),
            chat_id: chat_id.into(),
            is_dm: false,
            text: text.into(),
            is_mention,
            reply_to: None,
        }
    }

    /// Set the reply-to message ID.
    pub fn with_reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }
}

/// A response to be sent back to the chat.
#[derive(Debug, Clone)]
pub struct BotResponse {
    /// The text content to send
    pub text: String,
    /// Optional message ID to reply to
    pub reply_to: Option<String>,
}

impl BotResponse {
    /// Create a new response with text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            reply_to: None,
        }
    }

    /// Create a response that replies to a specific message.
    pub fn reply_to(text: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            reply_to: Some(message_id.into()),
        }
    }

    /// Check if the response is empty.
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}
