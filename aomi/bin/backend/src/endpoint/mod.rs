mod chat;
mod db;
mod history;
mod sessions;
mod system;
mod types;
pub mod wallet;

use crate::endpoint::chat::{
    chat_endpoint, health, interrupt_endpoint, state_endpoint, SharedSessionManager,
};
use axum::{
    routing::{get, post},
    Router,
};

pub fn create_router(session_manager: SharedSessionManager) -> Router {
    // Wallet router uses Extension for pool, so we give it empty state
    // and merge it before applying session_manager state
    let wallet_routes = wallet::create_wallet_router();
    
    Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/interrupt", post(interrupt_endpoint))
        .nest("/api/sessions", sessions::create_sessions_router())
        .nest("/api", system::create_system_router())
        .nest("/api/db", db::create_db_router())
        .with_state(session_manager)
        .merge(Router::new().nest("/api/wallet", wallet_routes))
}
