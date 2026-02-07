use anyhow::Result;
use sqlx::{Any, Pool};
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::{error, info};

use crate::{
    commands::{handle_help, handle_sign, parse_command},
    config::TelegramConfig,
    handlers::handle_message,
    panels::{self, PanelCtx, PanelRouter},
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
        let router = Arc::new(panels::build_router());

        let handler = dptree::entry()
            .branch(
                Update::filter_message().endpoint(
                    |msg: Message,
                     bot_ref: Arc<TelegramBot>,
                     session_mgr: Arc<SessionManager>,
                     router: Arc<PanelRouter>| async move {
                        let text = msg.text().unwrap_or("");
                        if let Some((cmd, args)) = parse_command(text) {
                            // Try panel router first
                            let ctx = PanelCtx::from_message(
                                &bot_ref,
                                &bot_ref.pool,
                                &session_mgr,
                                &msg,
                            );
                            match router.handle_command(&ctx, cmd, args).await {
                                Ok(true) => return respond(()),
                                Ok(false) => {}
                                Err(e) => {
                                    error!("Error handling panel command: {}", e);
                                    return respond(());
                                }
                            }

                            // Fall back to non-panel commands
                            let result = match cmd {
                                "sign" => handle_sign(&bot_ref, &msg, args).await,
                                "help" => handle_help(&bot_ref, &msg).await,
                                _ => Ok(()), // unknown command, fall through to message handler
                            };
                            if let Err(e) = result {
                                error!("Error handling command: {}", e);
                            }
                            if matches!(cmd, "sign" | "help") {
                                return respond(());
                            }
                        }

                        // Handle as normal message
                        if let Err(e) =
                            handle_message(&bot_ref, &msg, &session_mgr, &router).await
                        {
                            error!("Error handling message: {}", e);
                        }
                        respond(())
                    },
                ),
            )
            .branch(Update::filter_callback_query().endpoint(
                |query: CallbackQuery,
                 bot_ref: Arc<TelegramBot>,
                 session_mgr: Arc<SessionManager>,
                 router: Arc<PanelRouter>| async move {
                    let Some(data) = query.data.as_deref() else {
                        return respond(());
                    };

                    // Answer the callback query first
                    let _ = bot_ref.bot.answer_callback_query(query.id.clone()).await;

                    let ctx = PanelCtx::from_callback(
                        &bot_ref,
                        &bot_ref.pool,
                        &session_mgr,
                        &query,
                    );
                    match router.handle_callback(&ctx, data).await {
                        Ok(true) => {}
                        Ok(false) => {
                            // Unrecognized callback
                        }
                        Err(e) => {
                            error!("Error handling callback: {}", e);
                        }
                    }
                    respond(())
                },
            ));

        // Build and run dispatcher with long-polling
        Dispatcher::builder(bot.bot.clone(), handler)
            .dependencies(dptree::deps![bot.clone(), session_manager, router])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        info!("Telegram bot stopped");
        Ok(())
    }
}
