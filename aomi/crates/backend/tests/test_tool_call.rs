mod utils;

use aomi_backend::session::{BackendwithTool, DefaultSessionState, MessageSender};
use aomi_chat::SystemEvent;
use std::sync::Arc;
use utils::{StreamingToolBackend, flush_state};

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

    // Inject wallet request/response events to ensure they surface alongside tool output
    state
        .system_event_queue
        .push(SystemEvent::WalletTxRequest { payload: serde_json::json!({"amount": 1}) });
    state
        .system_event_queue
        .push(SystemEvent::WalletTxResponse {
            status: "ok".to_string(),
            tx_hash: Some("0xdeadbeef".to_string()),
            detail: Some("details".to_string()),
        });

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

    // Wallet events should be surfaced in active_system_events
    let wallet_events: Vec<_> = state
        .active_system_events
        .iter()
        .filter(|event| matches!(event, SystemEvent::WalletTxRequest { .. } | SystemEvent::WalletTxResponse { .. }))
        .collect();
    assert_eq!(wallet_events.len(), 2, "expected wallet request and response to surface");
}
