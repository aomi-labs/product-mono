use anyhow::Result;
use aomi_backend::SessionManager;
use aomi_chat::ChatApp;
use clap::Parser;
use sqlx::any::AnyPoolOptions;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod endpoint;
use endpoint::create_router;

// Environment variables
static BACKEND_HOST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
});
static BACKEND_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_PORT").unwrap_or_else(|_| "8080".to_string())
});

#[derive(Parser)]
#[command(name = "backend")]
#[command(about = "Web backend for AOMI EVM agent")]
struct Cli {
    /// Skip loading Uniswap documentation at startup
    #[arg(long)]
    no_docs: bool,

    /// Skip MCP server connection (for testing)
    #[arg(long)]
    skip_mcp: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    let chat_app = Arc::new(
        ChatApp::new_with_options(cli.no_docs, cli.skip_mcp)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );

    // Initialize session manager with optional database
    let session_manager = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        println!("🗄️  Connecting to database...");
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await?;
        println!(
            "✅ Database connected: {}",
            database_url.split('@').last().unwrap_or("database")
        );
        Arc::new(SessionManager::with_database(chat_app, pool))
    } else {
        println!(
            "💾 Running with in-memory session storage (set DATABASE_URL to enable persistence)"
        );
        Arc::new(SessionManager::new(chat_app))
    };

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

// TODO(@Han): Verify this works with Nginx
fn build_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}
