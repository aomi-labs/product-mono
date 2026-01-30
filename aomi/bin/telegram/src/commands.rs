//! Slash command handlers for Telegram bot.

use anyhow::Result;
use std::sync::Arc;
use teloxide::prelude::Requester;
use teloxide::payloads::SendMessageSetters;
use teloxide::types::{Message, InlineKeyboardButton, InlineKeyboardMarkup, WebAppInfo};
use tracing::info;

use aomi_bot_core::{DbWalletConnectService, WalletConnectService};
use sqlx::{Any, Pool};

use crate::TelegramBot;
use crate::session::dm_session_key;

/// Mini App URL for wallet connection
fn mini_app_url() -> String {
    std::env::var("MINI_APP_URL").unwrap_or_else(|_| "https://app.aomi.ai".to_string())
}

/// Check if a message is a command and return the command name and args.
pub fn parse_command(text: &str) -> Option<(&str, &str)> {
    if !text.starts_with('/') {
        return None;
    }
    
    let text = text.trim();
    let mut parts = text.splitn(2, |c: char| c.is_whitespace());
    let cmd = parts.next()?.trim_start_matches('/');
    let args = parts.next().unwrap_or("").trim();
    
    // Remove @botname suffix if present
    let cmd = cmd.split('@').next()?;
    
    Some((cmd, args))
}

/// Handle wallet-related commands.
/// Returns Ok(true) if command was handled, Ok(false) if not a command.
pub async fn handle_command(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
) -> Result<bool> {
    let text = message.text().unwrap_or("");
    
    let (cmd, args) = match parse_command(text) {
        Some(c) => c,
        None => return Ok(false),
    };

    let chat_id = message.chat.id;
    
    match cmd {
        "connect" => {
            handle_connect(bot, message).await?;
            Ok(true)
        }
        "wallet" => {
            handle_wallet(bot, message, pool).await?;
            Ok(true)
        }
        "disconnect" => {
            handle_disconnect(bot, message, pool).await?;
            Ok(true)
        }
        "start" => {
            handle_start(bot, message).await?;
            Ok(true)
        }
        "help" => {
            handle_help(bot, message).await?;
            Ok(true)
        }
        _ => Ok(false), // Unknown command, let it fall through to normal handling
    }
}

/// Handle /start command.
async fn handle_start(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;
    
    let keyboard = InlineKeyboardMarkup::new([[
        InlineKeyboardButton::web_app(
            "🔗 Connect Wallet",
            WebAppInfo { url: mini_app_url().parse()? }
        )
    ]]);
    
    bot.bot.send_message(chat_id, 
        "👋 *Welcome to Aomi\\!*\n\n\
        I'm your DeFi assistant\\. I can help you:\n\
        • Swap tokens\n\
        • Check balances\n\
        • Interact with DeFi protocols\n\n\
        Connect your wallet to get started\\!"
    )
    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
    .reply_markup(keyboard)
    .await?;
    
    Ok(())
}

/// Handle /connect command - opens Mini App.
async fn handle_connect(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;
    
    let keyboard = InlineKeyboardMarkup::new([[
        InlineKeyboardButton::web_app(
            "🔗 Connect Wallet",
            WebAppInfo { url: mini_app_url().parse()? }
        )
    ]]);
    
    bot.bot.send_message(chat_id, "Tap the button below to connect your wallet:")
        .reply_markup(keyboard)
        .await?;
    
    Ok(())
}

/// Handle /wallet command.
async fn handle_wallet(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
) -> Result<()> {
    let chat_id = message.chat.id;
    let user_id = message.from.as_ref().map(|u| u.id).unwrap();
    let session_key = dm_session_key(user_id);
    
    let wallet_service = DbWalletConnectService::new(pool.clone());
    
    match wallet_service.get_bound_wallet(&session_key).await {
        Ok(Some(address)) => {
            let short_addr = if address.len() > 10 {
                format!("{}...{}", &address[..6], &address[address.len()-4..])
            } else {
                address.clone()
            };
            
            let keyboard = InlineKeyboardMarkup::new([[
                InlineKeyboardButton::web_app(
                    "🔄 Change Wallet",
                    WebAppInfo { url: mini_app_url().parse()? }
                )
            ]]);
            
            bot.bot.send_message(chat_id,
                format!("💳 *Connected wallet:*\n\n`{}`", address)
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .reply_markup(keyboard)
            .await?;
        }
        Ok(None) => {
            let keyboard = InlineKeyboardMarkup::new([[
                InlineKeyboardButton::web_app(
                    "🔗 Connect Wallet",
                    WebAppInfo { url: mini_app_url().parse()? }
                )
            ]]);
            
            bot.bot.send_message(chat_id, "No wallet connected\\.")
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .reply_markup(keyboard)
                .await?;
        }
        Err(e) => {
            bot.bot.send_message(chat_id, format!("❌ Error: {}", e)).await?;
        }
    }
    
    Ok(())
}

/// Handle /disconnect command.
async fn handle_disconnect(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
) -> Result<()> {
    let chat_id = message.chat.id;
    let user_id = message.from.as_ref().map(|u| u.id).unwrap();
    let session_key = dm_session_key(user_id);
    
    let wallet_service = DbWalletConnectService::new(pool.clone());
    
    match wallet_service.disconnect(&session_key).await {
        Ok(()) => {
            bot.bot.send_message(chat_id, "✅ Wallet disconnected\\.").parse_mode(teloxide::types::ParseMode::MarkdownV2).await?;
            info!("User {} disconnected wallet", user_id);
        }
        Err(e) => {
            bot.bot.send_message(chat_id, format!("❌ Error: {}", e)).await?;
        }
    }
    
    Ok(())
}

/// Handle /help command.
async fn handle_help(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;
    
    bot.bot.send_message(chat_id,
        "🤖 *Aomi Commands*\n\n\
        /connect \\- Link your Ethereum wallet\n\
        /wallet \\- Show connected wallet\n\
        /disconnect \\- Unlink your wallet\n\
        /help \\- Show this message\n\n\
        Or just chat with me naturally\\!"
    )
    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
    .await?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command() {
        assert_eq!(parse_command("/connect"), Some(("connect", "")));
        assert_eq!(parse_command("/connect 0x123"), Some(("connect", "0x123")));
        assert_eq!(parse_command("/wallet@mybot"), Some(("wallet", "")));
        assert_eq!(parse_command("hello"), None);
        assert_eq!(parse_command(""), None);
    }
}
