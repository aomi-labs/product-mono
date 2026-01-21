use aomi_core::{CoreCommand, Message, SystemEventQueue};
use aomi_tools::scheduler::SessionToolHandler;
use chrono::Local;
use serde::Serialize;
use tokio::sync::mpsc;

pub use aomi_core::app::AomiApp;

// This is the limit of async events that can be buffered in the session state.
pub const ASYNC_EVENT_BUFFER_LIMIT: usize = 100;

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
    pub tool_stream: Option<(String, String)>, // (topic, content)
    pub timestamp: String,
    pub is_streaming: bool,
}

impl ChatMessage {
    pub fn new(sender: MessageSender, content: String, topic: Option<&str>) -> Self {
        let tool_stream = topic.map(|t| (t.to_string(), String::new()));
        Self {
            sender,
            content,
            tool_stream,
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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HistorySession {
    pub title: String,
    pub session_id: String,
}

pub struct SessionState {
    pub is_processing: bool,
    // Channels
    pub input_sender: mpsc::Sender<String>,
    pub command_reciever: mpsc::Receiver<CoreCommand>,
    pub interrupt_sender: mpsc::Sender<()>,
    // User-specific session state
    pub messages: Vec<ChatMessage>,
    pub system_event_queue: SystemEventQueue,
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
    // pub system_events: Vec<SystemEvent>,
    pub title: Option<String>,
    pub is_processing: bool,
}
