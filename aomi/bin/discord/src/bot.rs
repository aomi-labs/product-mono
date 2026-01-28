//! Discord bot setup and event handling.

use anyhow::Result;
use serenity::all::{
    Client, Context, EventHandler, GatewayIntents, Message, Ready,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::{config::DiscordConfig, handlers::handle_message};
use aomi_backend::SessionManager;

/// Wrapper holding shared state for the bot.
pub struct BotState {
    pub config: DiscordConfig,
    pub session_manager: Arc<SessionManager>,
    pub bot_id: RwLock<Option<u64>>,
}

/// Event handler for Discord events.
struct Handler {
    state: Arc<BotState>,
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!("Discord bot connected as {}", ready.user.name);
        
        // Store the bot's user ID
        let mut bot_id = self.state.bot_id.write().await;
        *bot_id = Some(ready.user.id.get());
        
        info!("Bot user ID: {}", ready.user.id.get());
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // Get bot ID
        let bot_id = {
            let guard = self.state.bot_id.read().await;
            match *guard {
                Some(id) => id,
                None => {
                    error!("Bot ID not yet available, ignoring message");
                    return;
                }
            }
        };

        if let Err(e) = handle_message(
            &ctx,
            &msg,
            &self.state.config,
            &self.state.session_manager,
            bot_id,
        )
        .await
        {
            error!("Error handling message: {}", e);
        }
    }
}

pub struct DiscordBot {
    pub config: DiscordConfig,
}

impl DiscordBot {
    pub fn new(config: DiscordConfig) -> Result<Self> {
        Ok(Self { config })
    }

    /// Run the Discord bot.
    ///
    /// Sets up the serenity client with message intents and starts
    /// listening for events. This method blocks until the bot is stopped.
    pub async fn run(self, session_manager: Arc<SessionManager>) -> Result<()> {
        info!("Starting Discord bot...");

        let state = Arc::new(BotState {
            config: self.config.clone(),
            session_manager,
            bot_id: RwLock::new(None),
        });

        let handler = Handler {
            state: state.clone(),
        };

        // We need message content intent to read message text
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        let mut client = Client::builder(&self.config.bot_token, intents)
            .event_handler(handler)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Discord client: {}", e))?;

        // Start the client
        client
            .start()
            .await
            .map_err(|e| anyhow::anyhow!("Discord client error: {}", e))?;

        info!("Discord bot stopped");
        Ok(())
    }
}
