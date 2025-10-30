use super::{
    history::{self, UserHistory},
    manager::SessionManager,
    session::{ChatBackend, ChatMessage, MessageSender, SessionState},
};
use anyhow::Result;
use aomi_agent::{ChatCommand, Message};
use async_trait::async_trait;
use std::{
    collections::VecDeque,
    sync::Arc,
    time::Instant,
};
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    task::yield_now,
};

#[derive(Clone)]
struct MockChatBackend {
    interactions: Arc<Mutex<VecDeque<MockInteraction>>>,
    history_lengths: Arc<Mutex<Vec<usize>>>,
}

#[derive(Clone)]
struct MockInteraction {
    expected_input: String,
    streaming_chunks: Vec<String>,
    tool_calls: Vec<(String, String)>,
    final_reply: String,
}

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
impl ChatBackend for MockChatBackend {
    async fn process_message(
        &self,
        history: Arc<RwLock<Vec<Message>>>,
        input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand>,
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

        for (name, args) in interaction.tool_calls.iter().cloned() {
            sender_to_ui
                .send(ChatCommand::ToolCall { name, args })
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

fn test_message(sender: MessageSender, content: &str) -> ChatMessage {
    ChatMessage {
        sender,
        content: content.to_string(),
        timestamp: "00:00:00 UTC".to_string(),
        is_streaming: false,
    }
}

fn history_snapshot(messages: Vec<ChatMessage>, last_activity: Instant) -> UserHistory {
    UserHistory::new(messages, last_activity)
}

async fn flush_state(state: &mut SessionState) {
    for _ in 0..8 {
        yield_now().await;
        state.update_state().await;
        if !state.is_processing {
            break;
        }
    }
}

#[tokio::test]
async fn rehydrated_session_keeps_agent_history_in_sync() {
    let backend_impl = Arc::new(MockChatBackend::new(vec![MockInteraction::streaming_only(
        "continue after restore",
        "Restored context reply",
    )]));
    let backend: Arc<dyn ChatBackend> = backend_impl.clone();
    let session_manager = SessionManager::with_backend(backend);

    let now = Instant::now();
    let restored_messages = vec![
        test_message(MessageSender::User, "first question"),
        test_message(MessageSender::Assistant, "first answer"),
    ];
    let restored_history = history_snapshot(restored_messages.clone(), now);

    let session_id = "rehydrate-session";
    let session_state = session_manager
        .get_or_create_session(session_id)
        .await
        .expect("initial session");

    {
        let mut state = session_state.lock().await;
        flush_state(&mut state).await;
    }

    // Seed restored history via public key mapping and user_history store
    let public_key = "0xREHYDRATE".to_string();
    session_manager.set_session_public_key(session_id, Some(public_key.clone()));
    session_manager
        .update_user_history(session_id, Some(public_key.clone()), &restored_history.messages())
        .await;

    let session_state = session_manager
        .get_or_create_session(session_id)
        .await
        .expect("rehydrated session");

    let (agent_handle, current_messages) = {
        let state = session_state.lock().await;
        (state.agent_history_handle(), state.messages.clone())
    };
    assert_eq!(
        current_messages, restored_messages,
        "UI history should sync before new traffic"
    );
    assert_eq!(
        agent_handle.read().await.len(),
        restored_messages.len(),
        "agent history should preload restored transcript"
    );

    {
        let mut state = session_state.lock().await;
        state.update_state().await;
        state
            .process_user_message("continue after restore".into())
            .await
            .expect("process restored message");
    }

    yield_now().await;

    {
        let mut state = session_state.lock().await;
        flush_state(&mut state).await;
    }

    let lengths = backend_impl.history_lengths().await;
    assert_eq!(
        lengths,
        vec![restored_history.messages().len()],
        "agent-side history should include rehydrated transcript"
    );
}

#[tokio::test]
async fn multiple_sessions_store_and_retrieve_history_by_public_key() {
    let backend_impl = Arc::new(MockChatBackend::new(vec![
        MockInteraction::with_tool_call(
            "Hello from user 1",
            "Reply for user 1",
            "set_network",
            r#"{"network":"testnet"}"#,
        ),
        MockInteraction::with_tool_call(
            "Hello from user 2",
            "Reply for user 2",
            "set_network",
            r#"{"network":"polygon"}"#,
        ),
        MockInteraction::with_tool_call(
            "Hello from user 3",
            "Reply for user 3",
            "set_network",
            r#"{"network":"base"}"#,
        ),
    ]));
    let backend: Arc<dyn ChatBackend> = backend_impl.clone();
    let session_manager = SessionManager::with_backend(backend);

    for i in 1..=3 {
        let session_id = format!("session-{i}");
        let public_key = format!("0xUSER{i}");
        let user_message = format!("Hello from user {i}");
        let expected_reply = format!("Reply for user {i}");

        let session_state = session_manager
            .get_or_create_session(&session_id)
            .await
            .expect("session creation");

        {
            let mut state = session_state.lock().await;
            flush_state(&mut state).await;
            state
                .process_user_message(user_message.clone())
                .await
                .expect("process user input");
        }

        yield_now().await;

        {
            let mut state = session_state.lock().await;
            flush_state(&mut state).await;
            assert!(
                state
                    .messages
                    .iter()
                    .any(|m| m.content.contains(&expected_reply)),
                "assistant reply should be present"
            );
            assert!(
                state
                    .messages
                    .iter()
                    .any(|m| m.content.starts_with("tool: set_network")),
                "tool call should be logged to transcript"
            );
            session_manager
                .update_user_history(&session_id, Some(public_key.clone()), &state.messages)
                .await;
        }

        let stored = session_manager
            .get_or_create_history(&Some(public_key.clone()))
            .await
            .expect("stored history");
        assert!(
            stored
                .messages()
                .iter()
                .any(|m| m.content.contains(&expected_reply)),
            "persisted history must include assistant reply"
        );
    }

    assert_eq!(
        backend_impl.history_lengths().await,
        vec![0, 0, 0],
        "each fresh session should start with empty agent history"
    );
}

#[tokio::test]
async fn public_key_history_rehydrates_new_session_context() {
    let backend_impl = Arc::new(MockChatBackend::new(vec![
        MockInteraction::streaming_only("first turn", "Initial reply"),
        MockInteraction::streaming_only("second turn", "Continuation reply"),
    ]));
    let backend: Arc<dyn ChatBackend> = backend_impl.clone();
    let session_manager = SessionManager::with_backend(backend);
    let public_key = "0xABC";

    let initial_session = session_manager
        .get_or_create_session("session-initial")
        .await
        .expect("initial session create");

    {
        let mut state = initial_session.lock().await;
        flush_state(&mut state).await;
        state
            .process_user_message("first turn".into())
            .await
            .expect("first turn");
    }

    yield_now().await;

    let stored_history = {
        let mut state = initial_session.lock().await;
        flush_state(&mut state).await;
        let snapshot = state.messages.clone();
        session_manager
            .update_user_history("session-initial", Some(public_key.to_string()), &snapshot)
            .await;
        snapshot
    };

    let retrieved = session_manager
        .get_or_create_history(&Some(public_key.to_string()))
        .await
        .expect("retrieved history");

    assert_eq!(
        stored_history.len(),
        retrieved.messages().len(),
        "persisted history should match retrieved snapshot"
    );

    // Map public key to resume session and persist retrieved history before creation
    session_manager.set_session_public_key("session-resume", Some(public_key.to_string()));
    session_manager
        .update_user_history("session-resume", Some(public_key.to_string()), &retrieved.messages())
        .await;
    let resume_session = session_manager
        .get_or_create_session("session-resume")
        .await
        .expect("resume session");

    yield_now().await;

    {
        let mut state = resume_session.lock().await;
        state.update_state().await;
        assert!(
            !state.is_processing,
            "rehydrated session should not be processing when queue is idle"
        );
        state
            .process_user_message("second turn".into())
            .await
            .expect("second turn");
    }

    yield_now().await;

    {
        let mut state = resume_session.lock().await;
        flush_state(&mut state).await;
        let snapshot = state.messages.clone();
        drop(state);
        assert!(
            snapshot
                .iter()
                .any(|m| m.content.contains("Continuation reply")),
            "resumed transcript should include follow-up reply: {:?}",
            snapshot
        );
    }

    let lengths = backend_impl.history_lengths().await;
    let expected_history_len = history::filter_system_messages(&stored_history).len();
    assert_eq!(
        lengths,
        vec![0, expected_history_len],
        "restored session must reuse stored agent context"
    );
}
