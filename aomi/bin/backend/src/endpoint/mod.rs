mod db;
mod history;
mod sessions;
mod system;
mod types;
mod chat;

use axum::{
    routing::{get, post},
    Router,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use chat::{chat_endpoint, state_endpoint, interrupt_endpoint};
use aomi_backend::{generate_session_id, Namespace, SessionManager, SessionResponse};

use crate::endpoint::chat::health;


pub fn create_router(session_manager: Arc<SessionManager>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/interrupt", post(interrupt_endpoint))
        .nest("/api/sessions", sessions::create_sessions_router())
        .nest("/api", system::create_system_router())
        .nest("/api/db", db::create_db_router())
        .with_state(session_manager)
}
