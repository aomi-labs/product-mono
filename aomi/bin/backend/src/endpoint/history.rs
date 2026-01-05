use std::sync::Arc;

use aomi_backend::{ChatMessage, SessionManager};

fn is_history_ready(messages: &[ChatMessage], is_processing: bool) -> bool {
    if is_processing {
        return false;
    }

    !messages.iter().any(|msg| msg.is_streaming)
}

pub async fn maybe_update_history(
    session_manager: &Arc<SessionManager>,
    session_id: &str,
    messages: &[ChatMessage],
    is_processing: bool,
) {
    if !is_history_ready(messages, is_processing) {
        return;
    }

    let public_key = session_manager.get_public_key(session_id);
    session_manager
        .update_user_history(session_id, public_key, messages)
        .await;
}
