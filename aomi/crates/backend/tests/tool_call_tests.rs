mod utils;

use aomi_backend::session::{BackendwithTool, DefaultSessionState, MessageSender};
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
