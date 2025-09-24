use anyhow::Result;
// Environment variables
static BACKEND_HOST: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("BACKEND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()));
static BACKEND_PORT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("BACKEND_PORT").unwrap_or_else(|_| "8080".to_string()));

use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use crate::{
    agent_pool::AgentPool,
    endpoints::{
        chat_endpoint, chat_stream, health, interrupt_endpoint, mcp_command_endpoint, state_endpoint,
        system_message_endpoint,
    },
    manager::SessionManager,
};
use rig::{agent::Agent, client::completion::CompletionClient, providers::anthropic};

pub(crate) mod agent_pool;
pub(crate) mod endpoints;
pub(crate) mod manager;
pub(crate) mod session;

#[derive(Parser)]
#[command(name = "backend")]
#[command(about = "Web backend for sessioned EVM chatbot")]
struct Cli {
    /// Skip loading Uniswap documentation at startup (reserved for future use)
    #[arg(long)]
    no_docs: bool,

    /// Number of Anthropic agents to keep warm in the pool
    #[arg(long, default_value_t = 3)]
    pool_size: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let agents = create_agent_pool(cli.pool_size.max(1), cli.no_docs).await?;
    let agent_pool = Arc::new(AgentPool::new(agents));

    let session_manager = Arc::new(SessionManager::new(cli.no_docs).with_agent_pool(agent_pool));

    // Start automatic session cleanup
    Arc::clone(&session_manager).start_cleanup_task().await;

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/chat/stream", get(chat_stream))
        .route("/api/interrupt", post(interrupt_endpoint))
        .route("/api/system", post(system_message_endpoint))
        .route("/api/mcp-command", post(mcp_command_endpoint))
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .with_state(session_manager);

    let host = &*BACKEND_HOST;
    let port = &*BACKEND_PORT;
    let bind_addr = format!("{}:{}", host, port);

    println!("🚀 Backend server starting on http://{}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn create_agent_pool(
    pool_size: usize,
    _skip_docs: bool,
) -> Result<Vec<Arc<Agent<anthropic::completion::CompletionModel>>>> {
    let anthropic_api_key =
        std::env::var("ANTHROPIC_API_KEY").map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

    let client = anthropic::Client::new(&anthropic_api_key);

    let mut agents = Vec::with_capacity(pool_size);
    for index in 0..pool_size {
        println!("🤖 Warming agent {}/{}", index + 1, pool_size);
        let agent =
            client.agent("claude-sonnet-4-20250514").preamble("You are an Ethereum operations assistant.").build();
        agents.push(Arc::new(agent));
    }

    Ok(agents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_agent_pool_without_api_key() {
        std::env::remove_var("ANTHROPIC_API_KEY");
        let result = create_agent_pool(1, true).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_manager_initialises() {
        // Provide a dummy API key so we can execute create_agent_pool without calling the network.
        std::env::set_var("ANTHROPIC_API_KEY", "dummy-key");
        let agents = create_agent_pool(1, true).await.expect("failed to create agent pool");
        let pool = Arc::new(AgentPool::new(agents));
        let manager = SessionManager::new(true).with_agent_pool(pool);
        assert_eq!(manager.get_active_session_count().await, 0);
    }
}
