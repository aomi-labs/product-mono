use anyhow::Result;
use axum::http::HeaderValue;
use clap::Parser;
use std::{env, sync::Arc};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

mod endpoint;
mod manager;
mod session;



mod threads;
mod manager2;
mod session2;

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

enum AllowedOrigins {
    List {
        headers: Vec<HeaderValue>,
        display: Vec<String>,
    },
    Mirror,
}

fn build_cors_layer() -> CorsLayer {
    match determine_allowed_origins() {
        AllowedOrigins::List { headers, display } => {
            println!("🔓 Allowing CORS origins: {}", display.join(", "));
            CorsLayer::new()
                .allow_methods(Any)
                .allow_headers(Any)
                .allow_origin(AllowOrigin::list(headers))
                .allow_credentials(true)
        }
        AllowedOrigins::Mirror => {
            println!("🔓 Allowing CORS origin: mirror request origin");
            CorsLayer::new()
                .allow_methods(Any)
                .allow_headers(Any)
                .allow_origin(AllowOrigin::mirror_request())
                .allow_credentials(true)
        }
    }
}

fn determine_allowed_origins() -> AllowedOrigins {
    let candidate_values = if let Ok(raw) = env::var("BACKEND_ALLOWED_ORIGINS") {
        raw.split(',')
            .map(|entry| entry.trim().to_owned())
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>()
    } else {
        default_origin_candidates()
    };

    let mut normalized: Vec<String> = candidate_values
        .into_iter()
        .filter_map(|value| normalize_origin(&value))
        .collect();

    if normalized.iter().any(|value| value == "*") {
        return AllowedOrigins::Mirror;
    }

    normalized.sort();
    normalized.dedup();

    let mut headers = Vec::new();
    let mut display = Vec::new();

    for origin in normalized.into_iter() {
        match HeaderValue::from_str(&origin) {
            Ok(header) => {
                headers.push(header);
                display.push(origin);
            }
            Err(err) => {
                eprintln!("⚠️  Ignoring invalid CORS origin '{}': {}", origin, err);
            }
        }
    }

    if headers.is_empty() {
        AllowedOrigins::Mirror
    } else {
        AllowedOrigins::List { headers, display }
    }
}

fn default_origin_candidates() -> Vec<String> {
    let mut origins = vec![
        "http://localhost:3000".to_string(),
        "http://127.0.0.1:3000".to_string(),
    ];

    if let Ok(domain) = env::var("AOMI_DOMAIN") {
        if let Some(origin) = normalize_origin(domain.trim()) {
            origins.push(origin);
        }
    }

    if let Ok(extra) = env::var("BACKEND_EXTRA_ALLOWED_ORIGINS") {
        for value in extra.split(',') {
            if let Some(origin) = normalize_origin(value) {
                origins.push(origin);
            }
        }
    }

    origins
}

fn normalize_origin(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    if trimmed == "*" {
        return Some("*".to_string());
    }

    if trimmed.contains("://") {
        Some(trimmed.to_string())
    } else if trimmed.starts_with("localhost") || trimmed.starts_with("127.") {
        Some(format!("http://{}", trimmed))
    } else {
        Some(format!("https://{}", trimmed))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        sync::{LazyLock, Mutex},
    };

    use crate::{manager::generate_session_id, session::SetupPhase};

    use super::*;

    static ENV_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct EnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let original = env::var(key).ok();
            env::set_var(key, value);
            Self { key, original }
        }

        fn clear(key: &'static str) -> Self {
            let original = env::var(key).ok();
            env::remove_var(key);
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                env::set_var(self.key, value);
            } else {
                env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn test_determine_allowed_origins_from_env_list() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let _guard_allowed = EnvGuard::set(
            "BACKEND_ALLOWED_ORIGINS",
            "https://foo.example, http://bar.localhost:4000",
        );
        let _guard_domain = EnvGuard::clear("AOMI_DOMAIN");
        let _guard_extra = EnvGuard::clear("BACKEND_EXTRA_ALLOWED_ORIGINS");

        match determine_allowed_origins() {
            AllowedOrigins::List { display, .. } => {
                assert_eq!(
                    display,
                    vec![
                        "http://bar.localhost:4000".to_string(),
                        "https://foo.example".to_string()
                    ]
                );
            }
            _ => panic!("expected explicit origin list"),
        }
    }

    #[test]
    fn test_determine_allowed_origins_wildcard() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let _guard_allowed = EnvGuard::set("BACKEND_ALLOWED_ORIGINS", "*, https://foo.example");
        let _guard_extra = EnvGuard::clear("BACKEND_EXTRA_ALLOWED_ORIGINS");
        let _guard_domain = EnvGuard::clear("AOMI_DOMAIN");

        match determine_allowed_origins() {
            AllowedOrigins::Mirror => {}
            _ => panic!("expected mirror when wildcard present"),
        }
    }

    #[test]
    fn test_default_origins_include_domain_and_localhost() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let _guard_allowed = EnvGuard::clear("BACKEND_ALLOWED_ORIGINS");
        let _guard_extra = EnvGuard::clear("BACKEND_EXTRA_ALLOWED_ORIGINS");
        let _guard_domain = EnvGuard::set("AOMI_DOMAIN", "app.foameo.ai");

        match determine_allowed_origins() {
            AllowedOrigins::List { display, .. } => {
                assert!(display.contains(&"https://app.foameo.ai".to_string()));
                assert!(display.contains(&"http://localhost:3000".to_string()));
                assert!(display.contains(&"http://127.0.0.1:3000".to_string()));
            }
            _ => panic!("expected explicit origin list"),
        }
    }

    #[tokio::test]
    async fn test_session_manager_create_session() {
        let session_manager = SessionManager::new(true);

        let session_id = "test-session-1";
        let session_state = session_manager
            .get_or_create_session(session_id)
            .await
            .expect("Failed to create session");

        // Verify we got a session state
        let state = session_state.lock().await;
        assert_eq!(state.messages.len(), 0);
        assert!(matches!(state.readiness.phase, SetupPhase::ConnectingMcp));
    }

    #[tokio::test]
    async fn test_session_manager_multiple_sessions() {
        let session_manager = SessionManager::new(true);

        // Create two different sessions
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
        let session_manager = SessionManager::new(true);

        let session_id = "test-session-reuse";

        // Create session first time
        let session_state_1 = session_manager
            .get_or_create_session(session_id)
            .await
            .expect("Failed to create session first time");

        // Get session second time
        let session_state_2 = session_manager
            .get_or_create_session(session_id)
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
        let session_manager = SessionManager::new(true);

        let session_id = "test-session-remove";

        // Create session
        let _session_state = session_manager
            .get_or_create_session(session_id)
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
