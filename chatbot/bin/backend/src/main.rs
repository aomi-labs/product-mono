use anyhow::Result;
use aomi_agent::ChatApp;
use clap::Parser;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

mod endpoint;
mod history;
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

    let chat_app = Arc::new(
        ChatApp::new(cli.no_docs)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );

    // Initialize session manager
    let session_manager = Arc::new(SessionManager::new(chat_app));

    // Start cleanup task
    let cleanup_manager = Arc::clone(&session_manager);
    cleanup_manager.start_cleanup_task();

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
    let cors_base = || {
        CorsLayer::new()
            .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
            .allow_headers([
                HeaderName::from_static("accept"),
                HeaderName::from_static("last-event-id"),
                HeaderName::from_static("content-type"),
                HeaderName::from_static("authorization"),
                HeaderName::from_static("x-requested-with"),
            ])
            .allow_credentials(true)
    };

    match determine_allowed_origins() {
        AllowedOrigins::List { headers, display } => {
            println!("🔓 Allowing CORS origins: {}", display.join(", "));
            cors_base().allow_origin(AllowOrigin::list(headers))
        }
        AllowedOrigins::Mirror => {
            println!("🔓 Allowing CORS origin: mirror request origin");
            cors_base().allow_origin(AllowOrigin::mirror_request())
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
    CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any)
}

#[cfg(test)]
mod tests;
