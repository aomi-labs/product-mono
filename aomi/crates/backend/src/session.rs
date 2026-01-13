use anyhow::Result;
use aomi_chat::{ChatCommand, Message, SystemEvent, SystemEventQueue};
use chrono::Local;
use futures::stream::{Stream, StreamExt};
use serde_json::json;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::{mpsc, Mutex as TokioMutex, RwLock};
use tracing::error;

use crate::{history, types::ActiveToolStream};

pub use crate::types::{
    AomiBackend, BackendwithTool, ChatMessage, ChatState, DefaultSessionState, DynAomiBackend,
    HistorySession, MessageSender, SessionState,
};

impl<S> SessionState<S>
where
    S: Send + std::fmt::Debug + StreamExt + Unpin + 'static,
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
        let scheduler = aomi_tools::scheduler::ToolScheduler::get_or_init()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get tool scheduler: {}", e))?;
        let handler = Arc::new(TokioMutex::new(scheduler.get_handler()));

        let initial_history = history.clone();

        let agent_history = Arc::new(RwLock::new(history::to_rig_messages(&history)));
        let backend = Arc::clone(&chat_backend);
        let agent_history_for_task = Arc::clone(&agent_history);
        let system_event_queue_for_task = system_event_queue.clone();
        let handler_for_task = handler.clone();
        let handler_for_poller = handler.clone();
        let system_event_queue_for_poller = system_event_queue.clone();

        tokio::spawn(async move {
            let mut receiver_from_ui = receiver_from_ui;
            let mut interrupt_receiver = interrupt_receiver;
            system_event_queue_for_task.push(SystemEvent::SystemNotice("Backend connected".into()));

            while let Some(input) = receiver_from_ui.recv().await {
                if let Err(err) = backend
                    .process_message(
                        agent_history_for_task.clone(),
                        system_event_queue_for_task.clone(),
                        handler_for_task.clone(),
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

        tokio::spawn(async move {
            loop {
                let mut handler_guard = handler_for_poller.lock().await;
                let _ = handler_guard.poll_streams_once();
                let completed = handler_guard.take_completed_calls();
                drop(handler_guard);

                if !completed.is_empty() {
                    for completion in completed {
                        system_event_queue_for_poller.push_tool_update(completion);
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        });

        Ok(Self {
            agent_history,
            relayed_async_calls: HashSet::new(),
            messages: initial_history,
            is_processing: false,
            system_event_queue,
            tool_handler: handler,
            sender_to_llm,
            receiver_from_llm,
            interrupt_sender,
            active_tool_streams: Vec::new(),
        })
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

    pub async fn send_system_prompt(&mut self, message: &str) -> Result<()> {
        let raw_message = format!("[[SYSTEM:{}]]", message);
        self.sender_to_llm.send(raw_message).await?;
        Ok(())
    }

    async fn send_continue_hint(&mut self, message: &str) {
        if !self.is_processing {
            self.add_assistant_message_streaming();
            self.is_processing = true;
        }

        if let Err(err) = self.send_system_prompt(message).await {
            self.system_event_queue
                .push(SystemEvent::SystemError(format!(
                    "Failed to send system hint: {err}. Agent may have disconnected."
                )));
            self.is_processing = false;
        }
    }

    // UI -> System -> Agent
    pub async fn send_ui_event(&mut self, message: String) -> Result<ChatMessage> {
        let content = message.trim();
        let chat_message = ChatMessage::new(MessageSender::System, content.to_string(), None);

        self.messages.push(chat_message.clone());

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
            self.system_event_queue
                .push(SystemEvent::InlineDisplay(value));
        } else {
            self.system_event_queue
                .push(SystemEvent::SystemNotice(content.to_string()));
        }
        Ok(chat_message)
    }

    pub async fn sync_system_events(&mut self) {
        let _ = self.advance_frontend_events();
        for event in self.system_event_queue.advance_llm_events() {
            self.send_events_to_history(event).await;
        }
    }

    pub async fn interrupt_processing(&mut self) -> Result<()> {
        if self.is_processing {
            if self.interrupt_sender.send(()).await.is_err() {
                self.system_event_queue.push(SystemEvent::SystemError(
                    "Failed to interrupt: agent not responding".into(),
                ));
            } else {
                self.system_event_queue
                    .push(SystemEvent::InlineDisplay(json!({
                        "type": "user_request",
                        "kind": "Interuption",
                        "payload": "Interrupted by user"
                    })));
            }
            self.is_processing = false;
        }
        Ok(())
    }

    pub async fn sync_state(&mut self) {
        // LLM -> UI + System
        // ChatCommand is the primary structure coming out from the LLM, which can be a command to UI or System
        // For LLM -> UI, we add it to Vec<ChatMessage> or active_tool_streams for immediate tool stream rendering
        // For LLM -> System, we add it to system_event_queue, and process that seperately at self.send_events_to_history
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
                    self.add_tool_message_streaming(topic.clone(), stream);
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

        // Poll existing tool streams
        // tool 1 msg: [....] <- poll
        // tool 2 msg: [....] <- poll
        // tool 3 msg: [....] <- poll
        // ...
        self.poll_ui_streams().await;
        self.sync_system_events().await;
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

    async fn send_events_to_history(&mut self, event: SystemEvent) {
        if let SystemEvent::InlineDisplay(value) = &event {
            if let Some(event_type) = value.get("type").and_then(|v| v.as_str()) {
                if event_type == "wallet_tx_response" {
                    let mut message = value
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();

                    if let Some(hash) = value.get("tx_hash").and_then(|v| v.as_str()) {
                        message.push_str(&format!(" (tx hash: {hash})"));
                    }

                    if let Some(extra) = value.get("detail").and_then(|v| v.as_str()) {
                        if !extra.is_empty() {
                            message.push_str(&format!(": {extra}"));
                        }
                    }

                    let mut history = self.agent_history.write().await;
                    history.push(Message::user(format!(
                        "[[SYSTEM]] Wallet tx response: {}",
                        message
                    )));
                }
            }
        }

        if let Some((call_id, tool_name, result, _is_async)) = tool_update_from_event(&event) {
            let should_continue = !self.relayed_async_calls.contains(&call_id);
            {
                let mut history = self.agent_history.write().await;
                let result_text = match result {
                    Ok(value) => {
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
                    }
                    Err(err) => format!("tool_error: {}", err),
                };
                history.push(Message::user(format!(
                    "[[SYSTEM]] Tool result for {} with call id {}: {}",
                    tool_name, call_id, result_text
                )));
            }
            if should_continue {
                self.relayed_async_calls.insert(call_id.clone());
                self.send_continue_hint("Tool result ready. Continue.")
                    .await;
            }
        }
    }

    fn add_tool_message_streaming(&mut self, topic: String, stream: S) {
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
            content: String::new(),
            tool_stream: Some((topic, String::new())),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: true,
        });
        let idx = self.messages.len() - 1;
        self.active_tool_streams.push(ActiveToolStream {
            stream,
            message_index: idx,
        });
    }

    async fn poll_ui_streams(&mut self) {
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
            system_events: self.advance_frontend_events(),
        } // POST
    }

    pub fn advance_frontend_events(&mut self) -> Vec<SystemEvent> {
        // Frontend should call advance_frontend_events on the shared SystemEventQueue.
        self.system_event_queue.advance_frontend_events()
    }

    pub fn send_to_llm(&self) -> &mpsc::Sender<String> {
        &self.sender_to_llm
    }
}

fn tool_update_from_event(
    event: &SystemEvent,
) -> Option<(String, String, Result<serde_json::Value, String>, bool)> {
    let (value, is_async) = match event {
        SystemEvent::SyncUpdate(value) => (value, false),
        SystemEvent::AsyncUpdate(value) => (value, true),
        _ => return None,
    };

    if value
        .get("type")
        .and_then(|v| v.as_str())
        .is_none_or(|t| t != "tool_completion")
    {
        return None;
    }

    let call_id = value.get("call_id")?.as_str()?.to_string();
    let tool_name = value.get("tool_name")?.as_str()?.to_string();
    let result = value.get("result")?.clone();

    let parsed = if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        Err(error.to_string())
    } else {
        Ok(result)
    };

    Some((call_id, tool_name, parsed, is_async))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history::HistoryBackend,
        manager::{generate_session_id, SessionManager},
    };
    use aomi_chat::ChatApp;
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
