use anyhow::Result;
use sqlx::{Any, Pool};
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::{error, info};

use crate::{
    commands::{handle_callback, handle_command},
    config::TelegramConfig,
    handlers::handle_message,
};
use aomi_backend::SessionManager;

pub struct TelegramBot {
    pub bot: Bot,
    pub config: TelegramConfig,
    pub pool: Pool<Any>,
}

impl TelegramBot {
    pub fn new(config: TelegramConfig, pool: Pool<Any>) -> Result<Self> {
        let bot = Bot::new(config.bot_token.clone());
        Ok(Self { bot, config, pool })
    }

    /// Run the Telegram bot with long-polling.
    pub async fn run(self, session_manager: Arc<SessionManager>) -> Result<()> {
        info!("Starting Telegram bot...");

        let bot = Arc::new(self);

        // Create message + callback handlers
        let handler =
            dptree::entry()
                .branch(
                    Update::filter_message().endpoint(
                        |msg: Message,
                         bot_ref: Arc<TelegramBot>,
                         session_mgr: Arc<SessionManager>| async move {
                            // First try to handle as a command
                            match handle_command(&bot_ref, &msg, &bot_ref.pool, &session_mgr).await
                            {
                                Ok(true) => {
                                    // Command was handled
                                    return respond(());
                                }
                                Ok(false) => {
                                    // Not a command, continue to normal handling
                                }
                                Err(e) => {
                                    error!("Error handling command: {}", e);
                                    return respond(());
                                }
                            }

                            // Handle as normal message
                            if let Err(e) = handle_message(&bot_ref, &msg, &session_mgr).await {
                                error!("Error handling message: {}", e);
                            }
                            respond(())
                        },
                    ),
                )
                .branch(Update::filter_callback_query().endpoint(
                    |query: CallbackQuery,
                     bot_ref: Arc<TelegramBot>,
                     session_mgr: Arc<SessionManager>| async move {
                        match handle_callback(&bot_ref, &query, &bot_ref.pool, &session_mgr).await {
                            Ok(true) => respond(()),
                            Ok(false) => respond(()),
                            Err(e) => {
                                error!("Error handling callback: {}", e);
                                respond(())
                            }
                        }
                    },
                ));

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
