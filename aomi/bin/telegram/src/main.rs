use anyhow::Result;
use aomi_anvil::provider_manager;
use aomi_backend::{PersistentHistoryBackend, SessionManager};
use clap::{ArgAction, Parser};
use sqlx::any::AnyPoolOptions;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod bot;
mod commands;
mod config;
mod handlers;
mod send;
mod session;

use bot::TelegramBot;
use config::TelegramConfig;

#[cfg(test)]
mod tests;

static DATABASE_URL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://aomi@localhost:5432/chatbot".to_string())
});

#[derive(Parser)]
#[command(name = "telegram")]
#[command(about = "Telegram bot for AOMI EVM agent")]
struct Cli {
    /// Skip loading Uniswap documentation at startup
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    no_docs: bool,

    /// Skip MCP server connection (for testing)
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    skip_mcp: bool,

    /// Path to bot config TOML (overrides BOT_CONFIG_PATH)
    #[arg(long)]
    bot_config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    let manager = provider_manager().await?;
    tracing::info!(
        instances = manager.instance_count(),
        "ProviderManager initialized"
    );

    // Initialize database
    sqlx::any::install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(10)
        .connect(&DATABASE_URL)
        .await?;

    // Create history backend
    let history_backend = Arc::new(PersistentHistoryBackend::new(pool.clone()).await);

    // Initialize session manager
    let session_manager =
        Arc::new(SessionManager::initialize(cli.no_docs, cli.skip_mcp, history_backend).await?);

    // Load telegram config from bot.toml
    let config_path = cli
        .bot_config
        .or_else(|| std::env::var("BOT_CONFIG_PATH").ok())
        .ok_or_else(|| anyhow::anyhow!("Missing bot config path. Set --bot-config or BOT_CONFIG_PATH"))?;
    let config = TelegramConfig::from_path(&config_path)?;

    // Create and run the bot
    let bot = TelegramBot::new(config, pool)?;
    bot.run(session_manager).await
}
