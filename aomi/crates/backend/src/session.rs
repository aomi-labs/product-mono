use anyhow::Result;
use aomi_chat::{ChatApp, ChatCommand, Message, SystemEvent, SystemEventQueue, ToolResultStream};
use aomi_forge::ForgeApp;
use aomi_l2beat::L2BeatApp;
use async_trait::async_trait;
use chrono::Local;
use futures::stream::{Stream, StreamExt};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::error;

use crate::history;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum SystemUpdate {
    TitleChanged {
        session_id: String,
        new_title: String,
    },
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

    // Append only as state changes
    pub messages: Vec<ChatMessage>,
    pub system_event_queue: SystemEventQueue,

    // UI <> System
    // path 1:
    //          Synchronous path, events incurred during the convo, rendered immediately
    //          UI <- conversation stream inclues active events <- System
    // path 2:
    //          Asynchronous path, event triggered in the backend
    //          UI <- broadcase notification <- System every 1 sec
    //          UI -> pull the actual events from system_event_queue

    // Change or drained as state changes
    pub is_processing: bool,
    active_tool_streams: Vec<ActiveToolStream<S>>,
    pub active_system_events: Vec<SystemEvent>, // path 1
    pub broadcasted_system_event_idx: usize,    // path 2
}

struct ActiveToolStream<S> {
    stream: S,
    message_index: usize,
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

impl<S: Send + std::fmt::Debug + StreamExt + Unpin + 'static> SessionState<S>
where
    S: Stream<Item = (String, Result<serde_json::Value, String>)>,
{
    pub async fn new(
        chat_backend: Arc<DynAomiBackend<S>>,
        history: Vec<ChatMessage>,
    ) -> Result<Self> {
        let (sender_to_llm, receiver_from_ui) = mpsc::channel(100);
        let (sender_to_ui, receiver_from_llm) = mpsc::channel(1000);
        let (interrupt_sender, interrupt_receiver) = mpsc::channel(100);
        let system_event_queue = SystemEventQueue::new();

        let initial_history = history.clone();

        let agent_history = Arc::new(RwLock::new(history::to_rig_messages(&history)));
        let backend = Arc::clone(&chat_backend);
        let agent_history_for_task = Arc::clone(&agent_history);
        let system_event_queue_for_task = system_event_queue.clone();

        tokio::spawn(async move {
            let mut receiver_from_ui = receiver_from_ui;
            let mut interrupt_receiver = interrupt_receiver;
            system_event_queue_for_task.push(SystemEvent::SystemNotice("Backend connected".into()));

            while let Some(input) = receiver_from_ui.recv().await {
                if let Err(err) = backend
                    .process_message(
                        agent_history_for_task.clone(),
                        system_event_queue_for_task.clone(),
                        input,
                        &sender_to_ui,
                        &mut interrupt_receiver,
                    )
                    .await
                {
                    let _ = sender_to_ui
                        .send(ChatCommand::Error(format!(
                            "Failed to process message: {err}"
                        )))
                        .await;
                }
            }
        });

        Ok(Self {
            messages: initial_history,
            is_processing: false,
            system_event_queue,
            active_system_events: Vec::new(),
            broadcasted_system_event_idx: 0,
            sender_to_llm,
            receiver_from_llm,
            interrupt_sender,
            active_tool_streams: Vec::new(),
        })
    }

    pub async fn process_user_message(&mut self, message: String) -> Result<()> {
        if self.is_processing {
            return Ok(());
        }

        let message = message.trim();
        if message.is_empty() {
            return Ok(());
        }

        self.add_user_message(message);
        self.is_processing = true;

        if let Err(e) = self.sender_to_llm.send(message.to_string()).await {
            self.system_event_queue
                .push(SystemEvent::SystemError(format!(
                    "Failed to send message: {e}. Agent may have disconnected."
                )));
            self.is_processing = false;
            return Ok(());
        }

        self.add_assistant_message_streaming();
        Ok(())
    }

    pub async fn relay_system_message_to_llm(&mut self, message: &str) -> Result<()> {
        let raw_message = format!("[[SYSTEM:{}]]", message);
        self.sender_to_llm.send(raw_message).await?;
        Ok(())
    }

    // UI -> System -> Agent
    pub async fn process_system_message(&mut self, message: String) -> Result<ChatMessage> {
        let content = message.trim();
        let chat_message = ChatMessage::new(MessageSender::System, content.to_string(), None);

        self.messages.push(chat_message.clone());

        self.relay_system_message_to_llm(content).await?;
        Ok(chat_message)
    }

    pub async fn interrupt_processing(&mut self) -> Result<()> {
        if self.is_processing {
            if self.interrupt_sender.send(()).await.is_err() {
                self.system_event_queue.push(SystemEvent::SystemError(
                    "Failed to interrupt: agent not responding".into(),
                ));
            } else {
                self.system_event_queue.push(SystemEvent::UserRequest {
                    kind: "Interuption".to_string(),
                    payload: "Interrupted by user".into(),
                });
            }
            self.is_processing = false;
        }
        Ok(())
    }

    pub async fn update_state(&mut self) {
        // LLM -> UI + System
        // ChatCommand is the primary structure coming out from the LLM, which can be a command to UI or System
        // For LLM -> UI, we add it to Vec<ChatMessage> or active_tool_streams for immediate tool stream rendering
        // For LLM -> System, we add it to system_event_queue, and process that seperately at self.handle_system_event
        //                    if it's a SystemBroadcast, we gotta impl the broadcast mechanism to UI

        while let Ok(msg) = self.receiver_from_llm.try_recv() {
            // tracing::debug!("[Session][v2]receiver_from_llm: {:?}", msg);
            match msg {
                ChatCommand::StreamingText(text) => {
                    let needs_new_message = match self.messages.last() {
                        Some(last_msg) => {
                            !(matches!(last_msg.sender, MessageSender::Assistant)
                                && last_msg.is_streaming)
                        }
                        None => true,
                    };

                    if needs_new_message {
                        self.add_assistant_message_streaming();
                    }

                    if let Some(streaming_msg) =
                        self.messages.iter_mut().rev().find(|m| {
                            m.is_streaming && matches!(m.sender, MessageSender::Assistant)
                        })
                    {
                        if let Some((_, content)) = streaming_msg.tool_stream.as_mut() {
                            content.push_str(&text);
                        } else {
                            streaming_msg.content.push_str(&text);
                        }
                    }
                }
                ChatCommand::ToolCall { topic, stream } => {
                    // Turn off the streaming flag of the last Assistant msg which init this tool call
                    if let Some(active_msg) =
                        self.messages.iter_mut().rev().find(|m| {
                            matches!(m.sender, MessageSender::Assistant) && m.is_streaming
                        })
                    {
                        active_msg.is_streaming = false;
                    }

                    // Tool msg with streaming, add to queue with flag on
                    let idx = self.add_tool_message_streaming(topic.clone());
                    self.active_tool_streams.push(ActiveToolStream {
                        stream,
                        message_index: idx,
                    });
                }
                ChatCommand::AsyncToolResult {
                    call_id,
                    tool_name,
                    result,
                } => {
                    self.system_event_queue
                        .push(SystemEvent::SystemToolDisplay {
                            tool_name,
                            call_id,
                            result,
                        });
                }
                ChatCommand::Complete => {
                    // Clear streaming flag on ALL messages, not just the last one
                    // This ensures orphaned streaming messages are properly closed
                    for msg in self.messages.iter_mut() {
                        if msg.is_streaming {
                            msg.is_streaming = false;
                        }
                    }
                    self.is_processing = false;
                }
                ChatCommand::Error(err) => {
                    error!("ChatCommand::Error {err}");
                    self.system_event_queue.push(SystemEvent::SystemError(err));
                    self.is_processing = false;
                }
                ChatCommand::Interrupted => {
                    if let Some(last_msg) = self.messages.last_mut() {
                        if matches!(last_msg.sender, MessageSender::Assistant) {
                            last_msg.is_streaming = false;
                        }
                    }
                    self.is_processing = false;
                }
            }
        }

        let new_events = self
            .system_event_queue
            .slice_from(self.broadcasted_system_event_idx);
        self.broadcasted_system_event_idx += new_events.len();
        for event in new_events {
            self.handle_system_event(event).await;
        }

        // Poll existing tool streams
        // tool 1 msg: [....] <- poll
        // tool 2 msg: [....] <- poll
        // tool 3 msg: [....] <- poll
        // ...
        self.poll_tool_streams().await;
    }

    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageSender::User,
            content: content.to_string(),
            tool_stream: None,
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    pub fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
            content: content.to_string(),
            tool_stream: None,
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    pub fn add_assistant_message_streaming(&mut self) {
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
            content: String::new(),
            tool_stream: None,
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: true,
        });
    }

    async fn handle_system_event(&mut self, event: SystemEvent) {
        match event {
            // Inline events are added to active_system_events for immediate rendering
            SystemEvent::SystemNotice(..) | SystemEvent::SystemError(..) => {
                self.active_system_events.push(event);
            }

            // Broadcast events handling
            SystemEvent::SystemBroadcast(..) => {
                // TODO
                self.active_system_events.push(event);
            }

            // Wallet events are special case that requires immediate relay btw UI <> LLLM
            SystemEvent::WalletTxRequest { payload } => {
                self.active_system_events
                    .push(SystemEvent::WalletTxRequest { payload });
            }
            SystemEvent::SystemToolDisplay { .. } => {
                self.active_system_events.push(event);
            }
            SystemEvent::WalletTxResponse {
                status,
                tx_hash,
                detail,
            } => {
                self.active_system_events
                    .push(SystemEvent::WalletTxResponse {
                        status: status.clone(),
                        tx_hash: tx_hash.clone(),
                        detail: detail.clone(),
                    });
                let mut message = status;
                if let Some(hash) = tx_hash {
                    message.push_str(&format!(" (tx hash: {hash})"));
                }
                if let Some(extra) = detail {
                    if !extra.is_empty() {
                        message.push_str(&format!(": {extra}"));
                    }
                }
                let _ = self.relay_system_message_to_llm(&message).await;
            }
            _ => {
                // intentionally no-op;
            }
        }
    }

    fn add_tool_message_streaming(&mut self, topic: String) -> usize {
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
            content: String::new(),
            tool_stream: Some((topic, String::new())),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: true,
        });
        self.messages.len() - 1
    }

    async fn poll_tool_streams(&mut self) {
        let mut still_active = Vec::with_capacity(self.active_tool_streams.len());

        for mut active_tool in self.active_tool_streams.drain(..) {
            let message_index = active_tool.message_index;
            let channel_closed = loop {
                match active_tool.stream.next().await {
                    Some((_tool_call_id, res)) => {
                        if let Some(ChatMessage {
                            tool_stream: Some((_, ref mut content)),
                            ..
                        }) = self.messages.get_mut(message_index)
                        {
                            if !content.is_empty() && !content.ends_with('\n') {
                                content.push('\n');
                            }
                            // If tools return error while streaming, just print to frontend
                            let chunk = match res {
                                Ok(chunk) => chunk.to_string(),
                                Err(e) => e.to_string(),
                            };
                            content.push_str(&chunk.to_string());
                        }
                        continue;
                    }
                    None => break true,
                }
            };

            if !channel_closed {
                still_active.push(active_tool);
            } else if let Some(message) = self.messages.get_mut(message_index) {
                message.is_streaming = false;
            }
        }

        self.active_tool_streams = still_active;
    }

    pub fn get_messages_mut(&mut self) -> &mut Vec<ChatMessage> {
        &mut self.messages
    }

    /// Returns the chat-stream-related state (messages, processing status, system events)
    /// Metadata (title, history_sessions, etc.) must be added by SessionManager
    pub fn get_chat_state(&mut self) -> ChatState {
        ChatState {
            messages: self.messages.clone(),
            is_processing: self.is_processing,
            system_events: self.take_system_events(),
            active_tool_streams_count: self.active_tool_streams.len(),
        }
    }

    pub fn take_system_events(&mut self) -> Vec<SystemEvent> {
        std::mem::take(&mut self.active_system_events)
    }

    pub fn send_to_llm(&self) -> &mpsc::Sender<String> {
        &self.sender_to_llm
    }
}

/// Chat-stream-related state from SessionState (no metadata)
/// This is the core data that API response types in bin/backend build upon.
#[derive(Clone, Serialize)]
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub system_events: Vec<SystemEvent>,
    pub active_tool_streams_count: usize,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history::HistoryBackend,
        manager::{generate_session_id, SessionManager},
    };
    use std::sync::Arc;

    // Mock HistoryBackend for tests
    struct MockHistoryBackend;

    #[async_trait::async_trait]
    impl HistoryBackend for MockHistoryBackend {
        async fn get_or_create_history(
            &self,
            _pubkey: Option<String>,
            _session_id: String,
            _title: Option<String>,
        ) -> anyhow::Result<Option<ChatMessage>> {
            Ok(None)
        }

        fn update_history(&self, _session_id: &str, _messages: &[ChatMessage]) {
            // No-op for tests
        }

        async fn flush_history(
            &self,
            _pubkey: Option<String>,
            _session_id: String,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_history_sessions(
            &self,
            _public_key: &str,
            _limit: usize,
        ) -> anyhow::Result<Vec<HistorySession>> {
            Ok(Vec::new())
        }

        async fn update_session_title(
            &self,
            _session_id: &str,
            _title: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_session_manager_create_session() {
        let chat_app = match ChatApp::new().await {
            Ok(app) => Arc::new(app),
            Err(_) => return,
        };
        let chat_backend: Arc<BackendwithTool> = chat_app;
        let history_backend = Arc::new(MockHistoryBackend);
        let session_manager = SessionManager::with_backend(chat_backend, history_backend);

        let session_id = "test-session-1";
        let session_state = session_manager
            .get_or_create_session(session_id, None, None)
            .await
            .expect("Failed to create session");

        let state = session_state.lock().await;
        assert_eq!(state.messages.len(), 0);
    }

    #[tokio::test]
    async fn test_session_manager_multiple_sessions() {
        let chat_app = match ChatApp::new().await {
            Ok(app) => Arc::new(app),
            Err(_) => return,
        };
        let chat_backend: Arc<BackendwithTool> = chat_app;
        let history_backend = Arc::new(MockHistoryBackend);
        let session_manager = SessionManager::with_backend(chat_backend, history_backend);

        let session1_id = "test-session-1";
        let session2_id = "test-session-2";

        let session1_state = session_manager
            .get_or_create_session(session1_id, None, None)
            .await
            .expect("Failed to create session 1");

        let session2_state = session_manager
            .get_or_create_session(session2_id, None, None)
            .await
            .expect("Failed to create session 2");

        assert_ne!(
            Arc::as_ptr(&session1_state),
            Arc::as_ptr(&session2_state),
            "Sessions should be different instances"
        );
        assert_eq!(session_manager.get_active_session_count().await, 2);
    }

    #[tokio::test]
    async fn test_session_manager_reuse_session() {
        let chat_app = match ChatApp::new().await {
            Ok(app) => Arc::new(app),
            Err(_) => return,
        };
        let chat_backend: Arc<BackendwithTool> = chat_app;
        let history_backend = Arc::new(MockHistoryBackend);
        let session_manager = SessionManager::with_backend(chat_backend, history_backend);
        let session_id = "test-session-reuse";

        let session_state_1 = session_manager
            .get_or_create_session(session_id, None, None)
            .await
            .expect("Failed to create session first time");

        let session_state_2 = session_manager
            .get_or_create_session(session_id, None, None)
            .await
            .expect("Failed to get session second time");

        assert_eq!(
            Arc::as_ptr(&session_state_1),
            Arc::as_ptr(&session_state_2),
            "Should reuse existing session"
        );
        assert_eq!(session_manager.get_active_session_count().await, 1);
    }

    #[tokio::test]
    async fn test_session_manager_remove_session() {
        let chat_app = match ChatApp::new().await {
            Ok(app) => Arc::new(app),
            Err(_) => return,
        };
        let chat_backend: Arc<BackendwithTool> = chat_app;
        let history_backend = Arc::new(MockHistoryBackend);
        let session_manager = SessionManager::with_backend(chat_backend, history_backend);
        let session_id = "test-session-remove";

        let _session_state = session_manager
            .get_or_create_session(session_id, None, None)
            .await
            .expect("Failed to create session");

        assert_eq!(session_manager.get_active_session_count().await, 1);

        session_manager.remove_session(session_id).await;

        assert_eq!(session_manager.get_active_session_count().await, 0);
    }

    #[tokio::test]
    async fn test_generate_session_id_uniqueness() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();

        assert_ne!(id1, id2, "Session IDs should be unique");
        assert!(!id1.is_empty(), "Session ID should not be empty");
        assert!(!id2.is_empty(), "Session ID should not be empty");
    }
}
