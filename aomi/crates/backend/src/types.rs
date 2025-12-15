use anyhow::Result;
use aomi_chat::{ChatApp, ChatCommand, Message, SystemEvent, SystemEventQueue, ToolResultStream};
use aomi_forge::ForgeApp;
use aomi_l2beat::L2BeatApp;
use async_trait::async_trait;
use chrono::Local;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

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

pub struct SessionState<S> {
    // Persistent
    pub sender_to_llm: mpsc::Sender<String>,
    pub receiver_from_llm: mpsc::Receiver<ChatCommand<S>>,
    pub interrupt_sender: mpsc::Sender<()>,
    pub messages: Vec<ChatMessage>,
    pub system_event_queue: SystemEventQueue,
    pub is_processing: bool,
    pub(crate) active_tool_streams: Vec<ActiveToolStream<S>>,
    pub active_system_events: Vec<SystemEvent>, // path 1
    pub pending_async_updates: Vec<Value>, // path 2
    pub(crate) last_system_event_idx: usize,
}

pub(crate) struct ActiveToolStream<S> {
    pub(crate) stream: S,
    pub(crate) message_index: usize,
}

// Type alias for backward compatibility
pub type DefaultSessionState = SessionState<ToolResultStream>;

// TODO: eventually AomiApp
#[async_trait]
pub trait AomiBackend: Send + Sync {
    type Command: Send; // LLMCommand
    async fn process_message(
        &self,
        history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        input: String,
        sender_to_ui: &mpsc::Sender<Self::Command>, // llm_outbound_sender
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()>;
}

pub type DynAomiBackend<S> = dyn AomiBackend<Command = ChatCommand<S>>;
pub type BackendwithTool = DynAomiBackend<ToolResultStream>;

/// Chat-stream-related state from SessionState (no metadata)
/// This is the core data that API response types in bin/backend build upon.
#[derive(Clone, Serialize)]
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub system_events: Vec<SystemEvent>,
}

#[async_trait]
impl AomiBackend for ChatApp {
    type Command = ChatCommand<ToolResultStream>;
    async fn process_message(
        &self,
        history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand<ToolResultStream>>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let mut history_guard = history.write().await;
        ChatApp::process_message(
            self,
            &mut history_guard,
            input,
            sender_to_ui,
            &system_events,
            interrupt_receiver,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to process message: {}", e))?;
        Ok(())
    }
}

#[async_trait]
impl AomiBackend for L2BeatApp {
    type Command = ChatCommand<ToolResultStream>;
    async fn process_message(
        &self,
        history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand<ToolResultStream>>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let mut history_guard = history.write().await;
        L2BeatApp::process_message(
            self,
            &mut history_guard,
            &system_events,
            input,
            sender_to_ui,
            interrupt_receiver,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to process message: {}", e))?;
        Ok(())
    }
}

#[async_trait]
impl AomiBackend for ForgeApp {
    type Command = ChatCommand<ToolResultStream>;
    async fn process_message(
        &self,
        history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand<ToolResultStream>>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let mut history_guard = history.write().await;
        ForgeApp::process_message(
            self,
            &mut history_guard,
            &system_events,
            input,
            sender_to_ui,
            interrupt_receiver,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to process message: {}", e))?;
        Ok(())
    }
}
