//! Discord bot setup and event handling.

use anyhow::Result;
use serenity::all::{
    Client, Context, EventHandler, GatewayIntents, Message, Ready,
    Interaction,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use sqlx::{Any, Pool};

use crate::{
    config::DiscordConfig,
    handlers::handle_message,
    commands::{register_commands, handle_command},
};
use aomi_backend::SessionManager;

/// Wrapper holding shared state for the bot.
pub struct BotState {
    pub config: DiscordConfig,
    pub session_manager: Arc<SessionManager>,
    pub pool: Pool<Any>,
    pub bot_id: RwLock<Option<u64>>,
}

/// Event handler for Discord events.
struct Handler {
    state: Arc<BotState>,
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Discord bot connected as {}", ready.user.name);
        
        // Store the bot ID
        let mut bot_id = self.state.bot_id.write().await;
        *bot_id = Some(ready.user.id.get());
        info!("Bot user ID: {}", ready.user.id.get());

        // Register slash commands globally
        let commands = register_commands();
        
        match serenity::all::Command::set_global_commands(&ctx.http, commands).await {
            Ok(cmds) => {
                info!("Registered {} global slash commands", cmds.len());
                for cmd in cmds {
                    info!("  /{}", cmd.name);
                }
            }
            Err(e) => {
                error!("Failed to register slash commands: {}", e);
            }
        }
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
            &self.state.pool,
            bot_id,
        )
        .await
        {
            error!("Error handling message: {}", e);
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            info!("Received slash command: /{}", command.data.name);
            
            if let Err(e) = handle_command(&ctx, &command, &self.state.pool).await {
                error!("Error handling slash command: {}", e);
            }
        }
    }
}

pub struct DiscordBot {
    pub config: DiscordConfig,
    pub pool: Pool<Any>,
}

impl DiscordBot {
    pub fn new(config: DiscordConfig, pool: Pool<Any>) -> Result<Self> {
        Ok(Self { config, pool })
    }

    /// Run the Discord bot.
    pub async fn run(self, session_manager: Arc<SessionManager>) -> Result<()> {
        info!("Starting Discord bot...");

        let state = Arc::new(BotState {
            config: self.config.clone(),
            session_manager,
            pool: self.pool,
            bot_id: RwLock::new(None),
        });

        let handler = Handler {
            state: state.clone(),
        };

        // We need message content + guild members intents
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
