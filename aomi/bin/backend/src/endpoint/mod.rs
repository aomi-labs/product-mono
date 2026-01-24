mod db;
mod history;
mod sessions;
mod system;
mod types;
mod chat;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use crate::endpoint::chat::{SharedSessionManager, chat_endpoint, health, interrupt_endpoint, state_endpoint};

pub fn create_router(session_manager: SharedSessionManager) -> Router {
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
