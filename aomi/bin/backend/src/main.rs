use anyhow::Result;
use aomi_anvil::default_manager;
use aomi_backend::{PersistentHistoryBackend, SessionManager};
use clap::Parser;
use sqlx::any::AnyPoolOptions;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod endpoint;
use endpoint::create_router;

// Environment variables
static BACKEND_HOST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
});
static BACKEND_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_PORT").unwrap_or_else(|_| "8080".to_string())
});
static DATABASE_URL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://aomi@localhost:5432/chatbot".to_string())
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

    let manager = default_manager().await?;
    tracing::info!(
        instances = manager.instance_count(),
        "ProviderManager initialized"
    );

    // Initialize database and run migrations
    sqlx::any::install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(10)
        .connect(&DATABASE_URL)
        .await?;

    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("Database migrations completed successfully");

    let api_auth = auth::ApiAuth::from_db(pool.clone()).await?;

    // Create history backend (reuse existing pool)
    let history_backend = Arc::new(PersistentHistoryBackend::new(pool).await);

    // Initialize session manager with all backends
    let session_manager =
        Arc::new(SessionManager::initialize(cli.no_docs, cli.skip_mcp, history_backend).await?);

    // Start cleanup task
    let cleanup_manager = Arc::clone(&session_manager);
    cleanup_manager.start_cleanup_task();

    // Start background tasks (title generation + async notification broadcasting)
    let background_manager = Arc::clone(&session_manager);
    background_manager.start_background_tasks();

    // Build router
    let app = create_router(session_manager)
        .layer(axum::middleware::from_fn_with_state(
            api_auth,
            auth::api_key_middleware,
        ))
        .layer(build_cors_layer());

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
