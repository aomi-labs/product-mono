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
    CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any)
}

#[cfg(test)]
mod tests;
