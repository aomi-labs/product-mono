use anyhow::Result;
// Environment variables
static BACKEND_HOST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string())
});
static BACKEND_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_PORT").unwrap_or_else(|_| "8080".to_string())
});

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Json, Sse},
    routing::{get, post},
    Router,
};
use chrono::Local;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::{Duration, Instant}};
use tokio::{sync::{mpsc, Mutex, RwLock}, time::interval};
use tokio_stream::{wrappers::IntervalStream, StreamExt};
use tower_http::cors::{CorsLayer, Any};
use uuid::Uuid;

use agent::{AgentMessage, LoadingProgress};

use crate::manager::SessionManager;
use crate::endpoints::*;

pub(crate) mod manager;
pub(crate) mod session;
pub(crate) mod endpoints;


#[derive(Parser)]
#[command(name = "backend")]
#[command(about = "Web backend for EVM chatbot")]
struct Cli {
    /// Skip loading Uniswap documentation at startup
    #[arg(long)]
    no_docs: bool,
}


#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();

    // Initialize session manager
    let session_manager = Arc::new(SessionManager::new());

    // Start cleanup task
    let cleanup_manager = Arc::clone(&session_manager);
    cleanup_manager.start_cleanup_task().await;

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/chat/stream", get(chat_stream))
        .route("/api/interrupt", post(interrupt_endpoint))
        .route("/api/system", post(system_message_endpoint))
        .route("/api/mcp-command", post(mcp_command_endpoint))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        )
        .with_state(session_manager);

    // Get host and port from environment variables or use defaults
    let host = &*BACKEND_HOST;
    let port = &*BACKEND_PORT;
    let bind_addr = format!("{}:{}", host, port);

    println!("🚀 Backend server starting on http://{}", bind_addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::manager::generate_session_id;

    use super::*;

    #[tokio::test]
    async fn test_session_manager_create_session() {
        let session_manager = SessionManager::new();

        let session_id = "test-session-1";
        let session_state = session_manager
            .get_or_create_session(session_id, true)
            .await
            .expect("Failed to create session");

        // Verify we got a session state
        let state = session_state.lock().await;
        assert_eq!(state.messages.len(), 0);
        assert!(state.is_loading); // Should start loading
    }

    #[tokio::test]
    async fn test_session_manager_multiple_sessions() {
        let session_manager = SessionManager::new();

        // Create two different sessions
        let session1_id = "test-session-1";
        let session2_id = "test-session-2";

        let session1_state = session_manager
            .get_or_create_session(session1_id, true)
            .await
            .expect("Failed to create session 1");

        let session2_state = session_manager
            .get_or_create_session(session2_id, true)
            .await
            .expect("Failed to create session 2");

        // Verify they are different instances
        assert_ne!(
            Arc::as_ptr(&session1_state),
            Arc::as_ptr(&session2_state),
            "Sessions should be different instances"
        );

        // Verify session count
        assert_eq!(session_manager.get_active_session_count().await, 2);
    }

    #[tokio::test]
    async fn test_session_manager_reuse_session() {
        let session_manager = SessionManager::new();

        let session_id = "test-session-reuse";

        // Create session first time
        let session_state_1 = session_manager
            .get_or_create_session(session_id, true)
            .await
            .expect("Failed to create session first time");

        // Get session second time
        let session_state_2 = session_manager
            .get_or_create_session(session_id, true)
            .await
            .expect("Failed to get session second time");

        // Should be the same instance
        assert_eq!(
            Arc::as_ptr(&session_state_1),
            Arc::as_ptr(&session_state_2),
            "Should reuse existing session"
        );

        // Verify session count is still 1
        assert_eq!(session_manager.get_active_session_count().await, 1);
    }

    #[tokio::test]
    async fn test_session_manager_remove_session() {
        let session_manager = SessionManager::new();

        let session_id = "test-session-remove";

        // Create session
        let _session_state = session_manager
            .get_or_create_session(session_id, true)
            .await
            .expect("Failed to create session");

        assert_eq!(session_manager.get_active_session_count().await, 1);

        // Remove session
        session_manager.remove_session(session_id).await;

        // Verify session is removed
        assert_eq!(session_manager.get_active_session_count().await, 0);
    }

    #[tokio::test]
    async fn test_generate_session_id_uniqueness() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();

        assert_ne!(id1, id2, "Session IDs should be unique");
        assert!(!id1.is_empty(), "Session ID should not be empty");
        assert!(!id2.is_empty(), "Session ID should not be empty");
    }
}