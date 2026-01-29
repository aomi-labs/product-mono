mod utils;

use aomi_backend::session::{AomiBackend, DefaultSessionState, MessageSender};
use aomi_core::SystemEvent;
use std::sync::Arc;
use utils::{flush_state, StreamingToolBackend};

#[tokio::test]
async fn tool_content_is_recorded() {
    let backend: Arc<AomiBackend> = Arc::new(StreamingToolBackend);
    let mut state = DefaultSessionState::new(backend, Vec::new())
        .await
        .expect("session init");

    state
        .send_user_input("trigger streaming tool".into())
        .await
        .expect("send user message");

    // Inject wallet request/response events to ensure they surface alongside tool output
    state
        .system_event_queue
        .push(SystemEvent::InlineCall(serde_json::json!({
            "type": "wallet_tx_request",
            "payload": {"amount": 1},
        })));
    state
        .system_event_queue
        .push(SystemEvent::InlineCall(serde_json::json!({
            "type": "wallet_tx_response",
            "status": "ok",
            "tx_hash": "0xdeadbeef",
            "detail": "details",
        })));

    flush_state(&mut state).await;
    state.sync_state().await;

    let tool_message = state
        .messages
        .iter()
        .find(|msg| {
            msg.tool_result.is_some()
                && matches!(msg.sender, MessageSender::Assistant | MessageSender::System)
        })
        .cloned()
        .expect("tool message present");

    let (topic, content) = tool_message.tool_result.expect("tool stream content");

    assert_eq!(topic, "streaming_tool");
    assert!(
        content.contains("first chunk second chunk"),
        "content missing expected content: {content}"
    );
    assert!(
        !tool_message.is_streaming,
        "tool message should be marked as completed"
    );

    let wallet_events: Vec<_> = state
        .advance_http_events()
        .into_iter()
        .filter(|event| {
            if let SystemEvent::InlineCall(payload) = event {
                if let Some(event_type) = payload.get("type").and_then(|v| v.as_str()) {
                    return event_type == "wallet_tx_request" || event_type == "wallet_tx_response";
                }
            }
            false
        })
        .collect();
    assert_eq!(
        wallet_events.len(),
        2,
        "expected wallet request and response to surface"
    );
}
