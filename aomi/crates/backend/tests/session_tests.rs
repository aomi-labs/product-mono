use anyhow::Result;
use aomi_backend::session::{AomiBackend, DefaultSessionState, MessageSender};
use aomi_chat::{ChatCommand, Message, ToolResultStream};
use async_trait::async_trait;
use std::{collections::VecDeque, sync::Arc};
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    task::yield_now,
};

#[allow(dead_code)]
#[derive(Clone)]
struct MockChatBackend {
    interactions: Arc<Mutex<VecDeque<MockInteraction>>>,
    history_lengths: Arc<Mutex<Vec<usize>>>,
}

#[allow(dead_code)]
#[derive(Clone)]
struct MockInteraction {
    expected_input: String,
    streaming_chunks: Vec<String>,
    tool_calls: Vec<(String, String)>,
    final_reply: String,
}

#[allow(dead_code)]
impl MockInteraction {
    fn streaming_only(input: &str, reply: &str) -> Self {
        Self {
            expected_input: input.to_string(),
            streaming_chunks: vec![reply.to_string()],
            tool_calls: Vec::new(),
            final_reply: reply.to_string(),
        }
    }

    fn with_tool_call(input: &str, reply: &str, tool_name: &str, tool_args: &str) -> Self {
        Self {
            expected_input: input.to_string(),
            streaming_chunks: vec![reply.to_string()],
            tool_calls: vec![(tool_name.to_string(), tool_args.to_string())],
            final_reply: reply.to_string(),
        }
    }
}

#[allow(dead_code)]
impl MockChatBackend {
    fn new(interactions: Vec<MockInteraction>) -> Self {
        Self {
            interactions: Arc::new(Mutex::new(interactions.into())),
            history_lengths: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn history_lengths(&self) -> Vec<usize> {
        self.history_lengths.lock().await.clone()
    }
}

#[async_trait]
impl AomiBackend for MockChatBackend {
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

async fn flush_state(state: &mut DefaultSessionState) {
    for _ in 0..8 {
        yield_now().await;
        state.update_state().await;
        if !state.is_processing {
            break;
        }
    }
}

#[derive(Clone)]
struct StreamingToolBackend;

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

#[tokio::test]
#[ignore = "History restoration being refactored with new HistoryBackend trait"]
async fn rehydrated_session_keeps_agent_history_in_sync() {
    // TODO: Reimplement with new HistoryBackend trait
}

#[tokio::test]
#[ignore = "History persistence being refactored with new HistoryBackend trait"]
async fn multiple_sessions_store_and_retrieve_history_by_public_key() {
    // TODO: Reimplement with new HistoryBackend trait
}

#[tokio::test]
#[ignore = "History persistence being refactored with new HistoryBackend trait"]
async fn public_key_history_rehydrates_new_session_context() {
    // TODO: Reimplement with new HistoryBackend trait
}

#[tokio::test]
async fn streaming_tool_content_is_accumulated() {
    use aomi_backend::session::BackendwithTool;
    let backend: Arc<BackendwithTool> = Arc::new(StreamingToolBackend);
    let mut state = DefaultSessionState::new(backend, Vec::new(), Vec::new(), None)
        .await
        .expect("session init");

    state
        .process_user_message("trigger streaming tool".into())
        .await
        .expect("send user message");

    flush_state(&mut state).await;
    state.update_state().await;

    let tool_message = state
        .messages
        .iter()
        .find(|msg| {
            msg.tool_stream.is_some()
                && matches!(msg.sender, MessageSender::Assistant | MessageSender::System)
        })
        .cloned()
        .expect("tool message present");

    let (topic, content) = tool_message.tool_stream.expect("tool stream content");

    assert_eq!(topic, "streaming_tool");
    assert!(
        content.contains("first chunk second chunk"),
        "content missing expected content: {content}"
    );
    assert!(
        !tool_message.is_streaming,
        "tool message should be marked as completed"
    );
}
