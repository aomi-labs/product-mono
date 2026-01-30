//! Slash command handlers for Telegram bot.

use anyhow::Result;
use teloxide::prelude::Requester;
use teloxide::payloads::SendMessageSetters;
use teloxide::types::{Message, InlineKeyboardButton, InlineKeyboardMarkup, WebAppInfo};
use tracing::{info, warn};

use aomi_bot_core::{DbWalletConnectService, WalletConnectService};
use sqlx::{Any, Pool};

use crate::TelegramBot;
use crate::session::dm_session_key;

/// Get Mini App URL - returns None if not HTTPS (Telegram requirement)
fn get_mini_app_url() -> Option<String> {
    let url = std::env::var("MINI_APP_URL").ok()?;
    if url.starts_with("https://") {
        Some(url)
    } else {
        warn!("MINI_APP_URL must be HTTPS for Telegram Web Apps: {}", url);
        None
    }
}

/// Create connect wallet keyboard - uses WebApp if HTTPS, otherwise shows instructions
fn make_connect_keyboard() -> Option<InlineKeyboardMarkup> {
    get_mini_app_url().map(|url| {
        InlineKeyboardMarkup::new([[
            InlineKeyboardButton::web_app(
                "🔗 Connect Wallet",
                WebAppInfo { url: url.parse().unwrap() }
            )
        ]])
    })
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
    
    let (cmd, _args) = match parse_command(text) {
        Some(c) => c,
        None => return Ok(false),
    };
    
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
    
    let msg = "👋 *Welcome to Aomi\\!*\n\n\
        I'm your DeFi assistant\\. I can help you:\n\
        • Swap tokens\n\
        • Check balances\n\
        • Interact with DeFi protocols\n\n\
        Use /connect to link your wallet, or just ask me anything\\!";
    
    if let Some(keyboard) = make_connect_keyboard() {
        bot.bot.send_message(chat_id, msg)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .reply_markup(keyboard)
            .await?;
    } else {
        bot.bot.send_message(chat_id, msg)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
    }
    
    Ok(())
}

/// Handle /connect command - opens Mini App if available.
async fn handle_connect(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;
    
    if let Some(keyboard) = make_connect_keyboard() {
        bot.bot.send_message(chat_id, "Tap the button below to connect your wallet:")
            .reply_markup(keyboard)
            .await?;
    } else {
        // No HTTPS URL available - show manual instructions
        bot.bot.send_message(chat_id, 
            "⚠️ Wallet connect is not configured\\.\n\n\
            Please ask the admin to set a valid HTTPS URL for `MINI_APP_URL`\\."
        ).parse_mode(teloxide::types::ParseMode::MarkdownV2).await?;
    }
    
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
            let msg = format!("💳 *Connected wallet:*\n\n`{}`", address);
            
            if let Some(keyboard) = make_connect_keyboard() {
                let change_keyboard = InlineKeyboardMarkup::new([[
                    InlineKeyboardButton::web_app(
                        "🔄 Change Wallet",
                        WebAppInfo { url: get_mini_app_url().unwrap().parse().unwrap() }
                    )
                ]]);
                bot.bot.send_message(chat_id, msg)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .reply_markup(change_keyboard)
                    .await?;
            } else {
                bot.bot.send_message(chat_id, msg)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await?;
            }
        }
        Ok(None) => {
            if let Some(keyboard) = make_connect_keyboard() {
                bot.bot.send_message(chat_id, "No wallet connected\\. Tap below to connect:")
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .reply_markup(keyboard)
                    .await?;
            } else {
                bot.bot.send_message(chat_id, "No wallet connected\\. Use /connect to link your wallet\\.")
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await?;
            }
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
