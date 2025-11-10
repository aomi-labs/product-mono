mod utils;

use utils::{
    flush_state, history_snapshot, test_message, MockBackend, MockInteraction, StreamingToolBackend,
};

use aomi_backend::{
    history,
    manager::SessionManager,
    session::{BackendwithTool, DefaultSessionState, MessageSender},
};
use std::{sync::Arc, time::Instant};
use tokio::task::yield_now;

#[tokio::test]
#[ignore = "History restoration not yet implemented"]
async fn rehydrated_session_keeps_agent_history_in_sync() {
    let backend_impl = Arc::new(MockBackend::new(vec![MockInteraction::streaming_only(
        "continue after restore",
        "Restored context reply",
    )]));
    let backend: Arc<BackendwithTool> = backend_impl.clone();
    let session_manager = SessionManager::with_backend(backend);

    let now = Instant::now();
    let restored_messages = vec![
        test_message(MessageSender::User, "first question"),
        test_message(MessageSender::Assistant, "first answer"),
    ];
    let restored_history = history_snapshot(restored_messages.clone(), now);

    let session_id = "rehydrate-session";
    let session_state = session_manager
        .get_or_create_session(session_id, None)
        .await
        .expect("initial session");

    {
        let mut state = session_state.lock().await;
        flush_state(&mut state).await;
    }

    let public_key = "0xREHYDRATE".to_string();
    session_manager.set_session_public_key(session_id, Some(public_key.clone()));
    session_manager
        .update_user_history(
            session_id,
            Some(public_key.clone()),
            restored_history.messages(),
        )
        .await;

    let session_state = session_manager
        .get_or_create_session(session_id, None)
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
    let backend_impl = Arc::new(MockBackend::new(vec![
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
    let backend: Arc<BackendwithTool> = backend_impl.clone();
    let session_manager = SessionManager::with_backend(backend);

    for i in 1..=3 {
        let session_id = format!("session-{i}");
        let public_key = format!("0xUSER{i}");
        let user_message = format!("Hello from user {i}");
        let expected_reply = format!("Reply for user {i}");

        let session_state = session_manager
            .get_or_create_session(&session_id, None)
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
                state.messages.iter().any(|m| {
                    if let Some((topic, _)) = &m.tool_stream {
                        topic.contains("set_network")
                    } else {
                        false
                    }
                }),
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
    let backend_impl = Arc::new(MockBackend::new(vec![
        MockInteraction::streaming_only("first turn", "Initial reply"),
        MockInteraction::streaming_only("second turn", "Continuation reply"),
    ]));
    let backend: Arc<BackendwithTool> = backend_impl.clone();
    let session_manager = SessionManager::with_backend(backend);
    let public_key = "0xABC";

    let initial_session = session_manager
        .get_or_create_session("session-initial", None)
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

    session_manager.set_session_public_key("session-resume", Some(public_key.to_string()));
    session_manager
        .update_user_history(
            "session-resume",
            Some(public_key.to_string()),
            retrieved.messages(),
        )
        .await;
    let resume_session = session_manager
        .get_or_create_session("session-resume", None)
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

#[tokio::test]
async fn streaming_tool_content_is_accumulated() {
    let backend: Arc<BackendwithTool> = Arc::new(StreamingToolBackend);
    let mut state = DefaultSessionState::new(backend, Vec::new())
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
