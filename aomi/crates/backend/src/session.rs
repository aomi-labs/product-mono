use crate::history;
use anyhow::Result;
use aomi_core::{
    app::{CoreCtx, CoreState},
    CoreCommand, SystemEvent, SystemEventQueue, ToolReturn,
};
use aomi_tools::scheduler::SessionToolHandler;
use chrono::Local;
use serde_json::json;
use std::{fmt::format, sync::Arc, time::Duration};
use tokio::sync::{mpsc, RwLock};
use tracing::error;

pub use crate::types::{
    AomiApp, AomiBackend, ChatMessage, DefaultSessionState, HistorySession, MessageSender,
    SessionResponse, SessionState,
};



impl SessionState {
    pub async fn new(chat_backend: Arc<AomiBackend>, history: Vec<ChatMessage>) -> Result<Self> {
        let (input_sender, input_reciever) = mpsc::channel(100);
        let (command_sender, command_reciever) = mpsc::channel(1000);
        let (interrupt_sender, interrupt_receiver) = mpsc::channel(100);
        let system_event_queue = SystemEventQueue::new();
        let scheduler = aomi_tools::scheduler::ToolScheduler::get_or_init()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get tool scheduler: {}", e))?;
        // TODO: Get actual session ID and namespaces from user context
        let session_id = "default_session".to_string();
        let namespaces = vec![
            "default".to_string(),
            "forge".to_string(),
            "ethereum".to_string(),
        ];
        let handler = scheduler.get_session_handler(session_id.clone(), namespaces.clone());

        // Create shared user state
        let user_state = Arc::new(RwLock::new(crate::types::UserState::default()));

        Self::start_processing(
            Arc::clone(&chat_backend),
            input_reciever,
            interrupt_receiver,
            command_sender.clone(),
            system_event_queue.clone(),
            history.clone(),
            session_id,
            namespaces,
            Arc::clone(&user_state),
        );

        Self::start_polling_tools(
            system_event_queue.clone(),
            handler.clone(),
            input_sender.clone(),
        );

        Ok(Self {
            messages: history,
            is_processing: false,
            system_event_queue,
            input_sender,
            command_reciever,
            interrupt_sender,
            user_state,
            handler,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn start_processing(
        backend: Arc<AomiBackend>,
        mut input_reciever: mpsc::Receiver<String>,
        mut interrupt_receiver: mpsc::Receiver<()>,
        command_sender: mpsc::Sender<CoreCommand>,
        system_event_queue: SystemEventQueue,
        initial_history: Vec<ChatMessage>,
        session_id: String,
        namespaces: Vec<String>,
        user_state: Arc<RwLock<crate::types::UserState>>,
    ) {
        tokio::spawn(async move {
            system_event_queue.push(SystemEvent::SystemNotice("Backend connected".into()));
            let agent_history_for_task =
                Arc::new(RwLock::new(history::to_rig_messages(&initial_history)));

            while let Some(input) = input_reciever.recv().await {
                let history_snapshot = {
                    let history_guard = agent_history_for_task.read().await;
                    history_guard.clone()
                };

                // Get current user state and convert to CoreState's UserState type
                let user_state_snapshot = {
                    let guard = user_state.read().await;
                    aomi_core::UserState {
                        address: guard.address.clone(),
                        chain_id: guard.chain_id,
                        is_connected: guard.is_connected,
                        ens_name: guard.ens_name.clone(),
                    }
                };

                let mut state = CoreState {
                    user_state: user_state_snapshot,
                    history: history_snapshot,
                    system_events: Some(system_event_queue.clone()),
                    session_id: session_id.clone(),
                    namespaces: namespaces.clone(),
                    tool_namespaces: backend.tool_namespaces(),
                };
                let ctx = CoreCtx {
                    command_sender: command_sender.clone(),
                    interrupt_receiver: Some(&mut interrupt_receiver),
                };
                if let Err(err) = backend.process_message(input, &mut state, ctx).await {
                    let _ = command_sender
                        .send(CoreCommand::Error(format!(
                            "Failed to process message: {err}"
                        )))
                        .await;
                } else {
                    let mut history_guard = agent_history_for_task.write().await;
                    *history_guard = state.history;
                }
            }
        });
    }

    fn start_polling_tools(
        system_event_queue: SystemEventQueue,
        handler: SessionToolHandler,
        _input_sender: mpsc::Sender<String>,
    ) {
        tokio::spawn(async move {
            loop {
                let mut handler_guard = handler.lock().await;
                let _ = handler_guard.poll_once();
                let completed = handler_guard.take_completed_calls();
                drop(handler_guard);

                if !completed.is_empty() {
                    for completion in completed {
                        if completion.metadata.is_async {
                            // let message = completion.to_string();
                            system_event_queue.push_tool_update(completion);
                        }
                        // let _ = input_sender.send(format!("[[SYSTEM:{}]]", message)).await;
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        });
    }

    pub async fn send_user_input(&mut self, message: String) -> Result<()> {
        if self.is_processing {
            return Ok(());
        }

        let message = message.trim();
        if message.is_empty() {
            return Ok(());
        }

        self.add_user_message(message);
        self.is_processing = true;

        if let Err(e) = self.input_sender.send(message.to_string()).await {
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

    pub async fn send_system_prompt(&mut self, message: &str) -> Result<()> {
        let raw_message = format!("[[SYSTEM:{}]]", message);
        self.input_sender.send(raw_message).await?;
        Ok(())
    }

    // UI -> System -> Agent
    pub async fn send_ui_event(&mut self, message: String) -> Result<ChatMessage> {
        let content = message.trim();
        let chat_message = ChatMessage::new(
            MessageSender::System, 
            format!("Response of system endpoint: {}", content).to_string(), 
            None
        );

        self.messages.push(chat_message.clone());

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
            self.system_event_queue
                .push(SystemEvent::AsyncCallback(value)); // "wallet_tx_response"
        } else {
            self.system_event_queue
                .push(SystemEvent::SystemNotice(content.to_string()));
        }
        Ok(chat_message)
    }

    pub async fn interrupt_processing(&mut self) -> Result<()> {
        if self.is_processing {
            if self.interrupt_sender.send(()).await.is_err() {
                self.system_event_queue.push(SystemEvent::SystemError(
                    "Failed to interrupt: agent not responding".into(),
                ));
            } else {
                self.system_event_queue
                    .push(SystemEvent::InlineCall(json!({
                        "type": "user_request",
                        "kind": "Interuption",
                        "payload": "Interrupted by user"
                    })));
            }
            self.is_processing = false;
        }
        Ok(())
    }

    /// Sync user wallet state from frontend
    pub async fn sync_user_state(&mut self, new_state: crate::types::UserState) {
        let mut guard = self.user_state.write().await;
        *guard = new_state;
    }

    pub async fn sync_state(&mut self) { // { pubkey, current_chain }
        // LLM -> UI + System
        // CoreCommand is the primary structure coming out from the LLM, which can be a command to UI or System
        // For LLM -> UI, we add it to Vec<ChatMessage> for immediate tool rendering
        // For LLM -> System, we add it to system_event_queue, and process that seperately at self.send_events_to_history
        //                    if it's a SystemBroadcast, we gotta impl the broadcast mechanism to UI

        while let Ok(msg) = self.command_reciever.try_recv() {
            // tracing::debug!("[Session][v2]receiver_from_llm: {:?}", msg);
            match msg {
                CoreCommand::StreamingText(text) => {
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
                CoreCommand::ToolCall { topic, stream } => {
                    // Turn off the streaming flag of the last Assistant msg which init this tool call
                    if let Some(active_msg) =
                        self.messages.iter_mut().rev().find(|m| {
                            matches!(m.sender, MessageSender::Assistant) && m.is_streaming
                        })
                    {
                        active_msg.is_streaming = false;
                    }

                    // Tool msg with immediate content
                    self.add_tool_message_sync(topic.clone(), stream);
                }
                CoreCommand::Complete => {
                    // Clear streaming flag on ALL messages, not just the last one
                    // This ensures orphaned streaming messages are properly closed
                    for msg in self.messages.iter_mut() {
                        if msg.is_streaming {
                            msg.is_streaming = false;
                        }
                    }
                    self.is_processing = false;
                }
                CoreCommand::Error(err) => {
                    error!("CoreCommand::Error {err}");
                    self.system_event_queue.push(SystemEvent::SystemError(err));
                    self.is_processing = false;
                }
                CoreCommand::Interrupted => {
                    if let Some(last_msg) = self.messages.last_mut() {
                        if matches!(last_msg.sender, MessageSender::Assistant) {
                            last_msg.is_streaming = false;
                        }
                    }
                    self.is_processing = false;
                }
            }
        }

        // Tool returns are immediate; async updates are handled via SystemEventQueue.
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

    fn add_tool_message_sync(&mut self, topic: String, stream: ToolReturn) {
        let content = if let Some(text) = stream.inner.as_str() {
            text.to_string()
        } else {
            serde_json::to_string_pretty(&stream.inner).unwrap_or_else(|_| stream.inner.to_string())
        };
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
            content: String::new(),
            tool_stream: Some((topic, content)),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    pub fn get_messages_mut(&mut self) -> &mut Vec<ChatMessage> {
        &mut self.messages
    }

    /// Returns session response with messages, processing status, system events, and title.
    /// System events include HTTP events (InlineCall, SystemError) for sync delivery.
    pub fn get_session_response(&mut self, title: Option<String>) -> SessionResponse {
        SessionResponse {
            messages: self.messages.clone(),
            system_events: self.system_event_queue.advance_http_events(),
            title,
            is_processing: self.is_processing,
        }
    }

    /// Advance SSE event counter and return new SSE events (SystemNotice, AsyncCallback).
    /// Used by broadcast_async_notifications.
    pub fn advance_sse_events(&mut self) -> Vec<SystemEvent> {
        self.system_event_queue.advance_sse_events()
    }

    /// Advance SSE event counter and return new SSE events (SystemNotice, AsyncCallback).
    /// Used by broadcast_async_notifications.
    pub fn advance_http_events(&mut self) -> Vec<SystemEvent> {
        self.system_event_queue.advance_http_events()
    }

    pub fn send_to_llm(&self) -> &mpsc::Sender<String> {
        &self.input_sender
    }

    /// Check if there are any ongoing tool calls that haven't completed yet
    pub async fn has_ongoing_tool_calls(&self) -> bool {
        self.handler.lock().await.has_ongoing_calls()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history::HistoryBackend,
        manager::{generate_session_id, SessionManager},
    };
    use aomi_core::CoreApp;
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
        let chat_app = match CoreApp::default().await {
            Ok(app) => Arc::new(app),
            Err(_) => return,
        };
        let chat_backend: Arc<AomiBackend> = chat_app;
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
        let chat_app = match CoreApp::default().await {
            Ok(app) => Arc::new(app),
            Err(_) => return,
        };
        let chat_backend: Arc<AomiBackend> = chat_app;
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
        let chat_app = match CoreApp::default().await {
            Ok(app) => Arc::new(app),
            Err(_) => return,
        };
        let chat_backend: Arc<AomiBackend> = chat_app;
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
        let chat_app = match CoreApp::default().await {
            Ok(app) => Arc::new(app),
            Err(_) => return,
        };
        let chat_backend: Arc<AomiBackend> = chat_app;
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
