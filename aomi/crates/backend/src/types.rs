use aomi_core::{CoreCommand, Message, SystemEvent, SystemEventQueue};
use aomi_tools::scheduler::SessionToolHandler;
use chrono::Local;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub use aomi_core::app::AomiApp;

// This is the limit of async events that can be buffered in the session state.
pub const ASYNC_EVENT_BUFFER_LIMIT: usize = 100;

/// User wallet state synced from frontend
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserState {
    /// Connected wallet address (0x...)
    pub address: Option<String>,
    /// Chain ID the wallet is connected to
    pub chain_id: Option<u64>,
    /// Whether the wallet is currently connected
    #[serde(default)]
    pub is_connected: bool,
    /// ENS name if resolved
    pub ens_name: Option<String>,
}

impl UserState {
    /// Format user state as a system message for the LLM
    pub fn format_for_llm(&self) -> String {
        if !self.is_connected {
            return "[[USER_STATE]] Wallet not connected".to_string();
        }

        let address = self.address.as_deref().unwrap_or("unknown");
        let chain_id = self
            .chain_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let ens = self.ens_name.as_deref().unwrap_or("none");

        format!(
            "[[USER_STATE]] Connected wallet: {} | Chain ID: {} | ENS: {}",
            address, chain_id, ens
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum MessageSender {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "agent")]
    Assistant,
    #[serde(rename = "system")]
    System,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatMessage {
    pub sender: MessageSender,
    pub content: String,
    pub tool_result: Option<(String, String)>, // (topic, content)
    pub timestamp: String,
    pub is_streaming: bool,
}

impl ChatMessage {
    pub fn new(sender: MessageSender, content: String, topic: Option<&str>) -> Self {
        let tool_result = topic.map(|t| (t.to_string(), String::new()));
        Self {
            sender,
            content,
            tool_result,
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        }
    }
}

impl From<ChatMessage> for Message {
    fn from(chat_message: ChatMessage) -> Self {
        match chat_message.sender {
            MessageSender::User => Message::user(chat_message.content),
            MessageSender::Assistant => Message::assistant(chat_message.content),
            // System msg in rig is user content
            MessageSender::System => Message::user(chat_message.content),
        }
    }
}

/// Unified session record for both listing and full retrieval.
/// - For listing: `messages` is empty, `public_key` is None
/// - For full load: `messages` populated, `public_key` from DB
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionRecord {
    pub session_id: String,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub public_key: Option<String>,
}

use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SessionState {
    pub is_processing: bool,
    // Channels
    pub input_sender: mpsc::Sender<String>,
    pub command_reciever: mpsc::Receiver<CoreCommand>,
    pub interrupt_sender: mpsc::Sender<()>,
    // User-specific session state
    pub messages: Vec<ChatMessage>,
    pub system_event_queue: SystemEventQueue,
    /// User wallet state synced from frontend (shared with processing task)
    pub user_state: Arc<RwLock<UserState>>,
    // Tool utilities
    pub(crate) handler: SessionToolHandler,
}

// Type alias for backward compatibility
pub type DefaultSessionState = SessionState;

pub type AomiBackend = dyn AomiApp<Command = CoreCommand>;

/// API response for session state (messages + metadata)
#[derive(Clone, Serialize)]
pub struct SessionResponse {
    pub messages: Vec<ChatMessage>,
    pub system_events: Vec<SystemEvent>,
    pub title: Option<String>,
    pub is_processing: bool,
}
