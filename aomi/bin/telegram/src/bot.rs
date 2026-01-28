use anyhow::Result;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::{error, info};

use crate::{config::TelegramConfig, handlers::handle_message};
use aomi_backend::SessionManager;

pub struct TelegramBot {
    pub bot: Bot,
    pub config: TelegramConfig,
}

impl TelegramBot {
    pub fn new(config: TelegramConfig) -> Result<Self> {
        let bot = Bot::new(config.bot_token.clone());
        Ok(Self { bot, config })
    }

    /// Run the Telegram bot with long-polling.
    ///
    /// Sets up a dispatcher to handle incoming messages and routes them
    /// through the handler pipeline. This method blocks until the bot is stopped.
    ///
    /// # Arguments
    /// * `session_manager` - Shared session manager for processing messages
    ///
    /// # Example
    /// ```ignore
    /// let bot = TelegramBot::new(config)?;
    /// let session_manager = Arc::new(SessionManager::new(...));
    /// bot.run(session_manager).await?;
    /// ```
    pub async fn run(self, session_manager: Arc<SessionManager>) -> Result<()> {
        info!("Starting Telegram bot...");

        let bot = Arc::new(self);

        // Create message handler
        let handler = Update::filter_message().endpoint(
            |msg: Message, bot_ref: Arc<TelegramBot>, session_mgr: Arc<SessionManager>| async move {
                if let Err(e) = handle_message(&bot_ref, &msg, &session_mgr).await {
                    error!("Error handling message: {}", e);
                }
                respond(())
            },
        );

        // Build and run dispatcher with long-polling
        Dispatcher::builder(bot.bot.clone(), handler)
            .dependencies(dptree::deps![bot.clone(), session_manager])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        info!("Telegram bot stopped");
        Ok(())
    }
}
