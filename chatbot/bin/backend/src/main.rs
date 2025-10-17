use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

mod endpoint;
mod manager;
mod session;
use endpoint::create_router;
use manager::SessionManager;

// Environment variables
static BACKEND_HOST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
});
static BACKEND_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_PORT").unwrap_or_else(|_| "8080".to_string())
});

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
    let cli = Cli::parse();

    // Initialize session manager
    let session_manager = Arc::new(SessionManager::new(cli.no_docs));

    // Start cleanup task
    let cleanup_manager = Arc::clone(&session_manager);
    cleanup_manager.start_cleanup_task().await;

    // Build router
    let app = create_router(session_manager).layer(build_cors_layer());

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

fn build_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{manager::generate_session_id, session::SetupPhase};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_session_manager_create_session() {
        let session_manager = SessionManager::new(true);

        let session_id = "test-session-1";
        let session_state = session_manager
            .get_or_create_session(session_id)
            .await
            .expect("Failed to create session");

        let state = session_state.lock().await;
        assert_eq!(state.messages.len(), 0);
        assert!(matches!(state.readiness.phase, SetupPhase::ConnectingMcp));
    }

    #[tokio::test]
    async fn test_session_manager_multiple_sessions() {
        let session_manager = SessionManager::new(true);

        let session1_id = "test-session-1";
        let session2_id = "test-session-2";

        let session1_state = session_manager
            .get_or_create_session(session1_id)
            .await
            .expect("Failed to create session 1");

        let session2_state = session_manager
            .get_or_create_session(session2_id)
            .await
            .expect("Failed to create session 2");

        assert_ne!(
            Arc::as_ptr(&session1_state),
            Arc::as_ptr(&session2_state),
            "Sessions should be different instances"
        );
        assert_eq!(session_manager.get_active_session_count().await, 2);
    }

    #[tokio::test]
    async fn test_session_manager_reuse_session() {
        let session_manager = SessionManager::new(true);
        let session_id = "test-session-reuse";

        let session_state_1 = session_manager
            .get_or_create_session(session_id)
            .await
            .expect("Failed to create session first time");

        let session_state_2 = session_manager
            .get_or_create_session(session_id)
            .await
            .expect("Failed to get session second time");

        assert_eq!(
            Arc::as_ptr(&session_state_1),
            Arc::as_ptr(&session_state_2),
            "Should reuse existing session"
        );
        assert_eq!(session_manager.get_active_session_count().await, 1);
    }

    #[tokio::test]
    async fn test_session_manager_remove_session() {
        let session_manager = SessionManager::new(true);
        let session_id = "test-session-remove";

        let _session_state = session_manager
            .get_or_create_session(session_id)
            .await
            .expect("Failed to create session");

        assert_eq!(session_manager.get_active_session_count().await, 1);

        session_manager.remove_session(session_id).await;

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
