use aomi_backend::{
    history::UserHistory,
    session::{AomiBackend, ChatMessage, DefaultSessionState, MessageSender},
};
use anyhow::Result;
use aomi_chat::{ChatCommand, Message, ToolResultStream};
use async_trait::async_trait;
use std::{collections::VecDeque, sync::Arc, time::Instant};
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    task::yield_now,
};

#[derive(Clone)]
pub struct MockBackend {
    interactions: Arc<Mutex<VecDeque<MockInteraction>>>,
    history_lengths: Arc<Mutex<Vec<usize>>>,
}

#[derive(Clone)]
pub struct MockInteraction {
    pub expected_input: String,
    pub streaming_chunks: Vec<String>,
    pub tool_calls: Vec<(String, String)>,
    pub final_reply: String,
}

impl MockInteraction {
    pub fn streaming_only(input: &str, reply: &str) -> Self {
        Self {
            expected_input: input.to_string(),
            streaming_chunks: vec![reply.to_string()],
            tool_calls: Vec::new(),
            final_reply: reply.to_string(),
        }
    }

    pub fn with_tool_call(input: &str, reply: &str, tool_name: &str, tool_args: &str) -> Self {
        Self {
            expected_input: input.to_string(),
            streaming_chunks: vec![reply.to_string()],
            tool_calls: vec![(tool_name.to_string(), tool_args.to_string())],
            final_reply: reply.to_string(),
        }
    }
}

impl MockBackend {
    pub fn new(interactions: Vec<MockInteraction>) -> Self {
        Self {
            interactions: Arc::new(Mutex::new(interactions.into())),
            history_lengths: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn history_lengths(&self) -> Vec<usize> {
        self.history_lengths.lock().await.clone()
    }
}

#[async_trait]
impl AomiBackend for MockBackend {
    type Command = ChatCommand<ToolResultStream>;

    async fn process_message(
        &self,
        history: Arc<RwLock<Vec<Message>>>,
        input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand<ToolResultStream>>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        while interrupt_receiver.try_recv().is_ok() {}

        let interaction = {
            let mut queued = self.interactions.lock().await;
            queued
                .pop_front()
                .expect("no scripted interaction remaining")
        };

        assert_eq!(
            interaction.expected_input, input,
            "unexpected user input routed to agent"
        );

        let snapshot_len = history.read().await.len();
        self.history_lengths.lock().await.push(snapshot_len);

        for chunk in interaction.streaming_chunks.iter() {
            sender_to_ui
                .send(ChatCommand::StreamingText(chunk.clone()))
                .await
                .expect("streaming chunk send");
        }

        for (name, args) in interaction.tool_calls.iter() {
            let topic = format!("{}: {}", name, args);
            let stream = ToolResultStream::empty();
            sender_to_ui
                .send(ChatCommand::ToolCall { topic, stream })
                .await
                .expect("tool call send");
        }

        sender_to_ui
            .send(ChatCommand::Complete)
            .await
            .expect("complete send");

        {
            let mut history_guard = history.write().await;
            history_guard.push(Message::user(input));
            if !interaction.final_reply.is_empty() {
                history_guard.push(Message::assistant(interaction.final_reply));
            }
        }

        Ok(())
    }
}

pub fn test_message(sender: MessageSender, content: &str) -> ChatMessage {
    ChatMessage {
        sender,
        content: content.to_string(),
        tool_stream: None,
        timestamp: "00:00:00 UTC".to_string(),
        is_streaming: false,
    }
}

pub fn history_snapshot(messages: Vec<ChatMessage>, last_activity: Instant) -> UserHistory {
    UserHistory::new(messages, last_activity)
}

pub async fn flush_state(state: &mut DefaultSessionState) {
    for _ in 0..8 {
        yield_now().await;
        state.update_state().await;
        if !state.is_processing {
            break;
        }
    }
}

#[derive(Clone)]
pub struct StreamingToolBackend;

#[async_trait]
impl AomiBackend for StreamingToolBackend {
    type Command = ChatCommand<ToolResultStream>;

    async fn process_message(
        &self,
        _history: Arc<RwLock<Vec<Message>>>,
        _input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand<ToolResultStream>>,
        _interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        sender_to_ui
            .send(ChatCommand::StreamingText("Thinking...".to_string()))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send text: {}", e))?;

        use serde_json::json;
        sender_to_ui
            .send(ChatCommand::ToolCall {
                topic: "streaming_tool".to_string(),
                stream: ToolResultStream::from_result(
                    "test_id".to_string(),
                    Ok(json!("first chunk second chunk")),
                ),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send tool call: {}", e))?;

        sender_to_ui
            .send(ChatCommand::Complete)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send complete: {}", e))?;

        Ok(())
    }
}
