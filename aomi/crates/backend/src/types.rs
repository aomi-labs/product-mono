use anyhow::Result;
use aomi_chat::{CoreApp, CoreCommand, Message, SystemEvent, SystemEventQueue, ToolStream, app::{CoreCtx, CoreState}};
use aomi_forge::ForgeApp;
use aomi_l2beat::L2BeatApp;
use aomi_tools::scheduler::{SessionToolHander, ToolHandler};
use async_trait::async_trait;
use chrono::Local;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};

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

pub struct SessionState<S> {
    pub is_processing: bool,
    // Channels
    pub input_sender: mpsc::Sender<String>,
    pub command_reciever: mpsc::Receiver<CoreCommand<S>>,
    pub interrupt_sender: mpsc::Sender<()>,
    // User-specific session state
    pub messages: Vec<ChatMessage>,
    pub system_event_queue: SystemEventQueue,
    // Tool utilities
    pub(crate) active_tool_streams: Vec<ActiveToolStream<S>>,
    pub tool_handler: SessionToolHander,
}

pub(crate) struct ActiveToolStream<S> {
    pub(crate) stream: S,
    pub(crate) message_index: usize,
}

// Type alias for backward compatibility
pub type DefaultSessionState = SessionState<ToolStream>;

// TODO: eventually AomiApp
#[async_trait]
pub trait AomiBackend: Send + Sync {
    type Command: Send; // LLMCommand
    async fn process_message(
        &self,
        input: String,
        history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        handler: SessionToolHander,
        command_sender: &mpsc::Sender<Self::Command>, // llm_outbound_sender
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()>;
}

pub type DynAomiBackend<S> = dyn AomiBackend<Command = CoreCommand<S>>;
pub type BackendwithTool = DynAomiBackend<ToolStream>;


/// API response for session state (messages + metadata)
#[derive(Clone, Serialize)]
pub struct SessionResponse {
    pub messages: Vec<ChatMessage>,
    pub system_events: Vec<SystemEvent>,
    pub title: Option<String>,
    pub is_processing: bool,
}

#[async_trait]
impl AomiBackend for CoreApp {
    type Command = CoreCommand<ToolStream>;
    async fn process_message(
        &self,
        input: String,
        history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        handler: SessionToolHander,
        command_sender: &mpsc::Sender<CoreCommand<ToolStream>>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let mut history_guard = history.write().await;
        let mut state = CoreState {
            history: history_guard.clone(),
            system_events: Some(system_events.clone()),
        };
        let ctx = CoreCtx {
            handler: Some(handler),
            command_sender: command_sender.clone(),
            interrupt_receiver: Some(interrupt_receiver),
        };
        CoreApp::process_message(self, input, &mut state, ctx)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to process message: {}", e))?;
        *history_guard = state.history;
        Ok(())
    }
}

#[async_trait]
impl AomiBackend for L2BeatApp {
    type Command = CoreCommand<ToolStream>;
    async fn process_message(
        &self,
        input: String,
        history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        handler: SessionToolHander,
        command_sender: &mpsc::Sender<CoreCommand<ToolStream>>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let mut history_guard = history.write().await;
        L2BeatApp::process_message(
            self,
            &mut history_guard,
            &system_events,
            handler,
            input,
            command_sender,
            interrupt_receiver,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to process message: {}", e))?;
        Ok(())
    }
}

#[async_trait]
impl AomiBackend for ForgeApp {
    type Command = CoreCommand<ToolStream>;
    async fn process_message(
        &self,
        input: String,
        history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        handler: SessionToolHander,
        command_sender: &mpsc::Sender<CoreCommand<ToolStream>>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let mut history_guard = history.write().await;
        ForgeApp::process_message(
            self,
            &mut history_guard,
            &system_events,
            handler,
            input,
            command_sender,
            interrupt_receiver,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to process message: {}", e))?;
        Ok(())
    }
}
