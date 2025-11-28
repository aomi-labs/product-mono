//! API response types for the backend endpoints.
//!
//! These types are specific to the HTTP API layer and combine data from
//! SessionState (chat stream) and SessionManager (metadata).

use aomi_backend::{session::HistorySession, ChatMessage, ChatState};
use serde::Serialize;

/// API response for session state (combines ChatState + metadata from SessionManager)
#[derive(Serialize)]
pub struct SessionResponse {
    pub messages: Vec<ChatMessage>,
    pub title: Option<String>,
    pub is_processing: bool,
    pub pending_wallet_tx: Option<String>,
}

impl SessionResponse {
    pub fn from_chat_state(chat_state: ChatState, title: Option<String>) -> Self {
        Self {
            messages: chat_state.messages,
            title,
            is_processing: chat_state.is_processing,
            pending_wallet_tx: chat_state.pending_wallet_tx,
        }
    }
}

/// Full session state for debugging/admin endpoints
#[derive(Serialize)]
pub struct FullSessionState {
    pub session_id: Option<String>,
    pub pubkey: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub title: Option<String>,
    pub is_processing: bool,
    pub pending_wallet_tx: Option<String>,
    pub is_archived: bool,
    pub has_sent_welcome: bool,
    pub last_summarized_msg: usize,
    pub active_tool_streams_count: usize,
    pub history_sessions: Vec<HistorySession>,
}

impl FullSessionState {
    pub fn from_chat_state(
        chat_state: ChatState,
        session_id: Option<String>,
        pubkey: Option<String>,
        title: Option<String>,
        is_archived: bool,
        last_summarized_msg: usize,
        history_sessions: Vec<HistorySession>,
    ) -> Self {
        Self {
            session_id,
            pubkey,
            messages: chat_state.messages,
            title,
            is_processing: chat_state.is_processing,
            pending_wallet_tx: chat_state.pending_wallet_tx,
            is_archived,
            has_sent_welcome: chat_state.has_sent_welcome,
            last_summarized_msg,
            active_tool_streams_count: chat_state.active_tool_streams_count,
            history_sessions,
        }
    }
}

/// Response wrapper for system messages
#[derive(Serialize)]
pub struct SystemResponse {
    pub res: ChatMessage,
}
