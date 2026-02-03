//! Slash command handlers for Telegram bot.

use anyhow::Result;
use std::sync::Arc;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, WebAppInfo};
use tracing::{info, warn};

use aomi_backend::{AomiModel, Namespace, NamespaceAuth, Selection, SessionManager};
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
        InlineKeyboardMarkup::new([[InlineKeyboardButton::web_app(
            "🔗 Connect Wallet",
            WebAppInfo {
                url: url.parse().unwrap(),
            },
        )]])
    })
}

/// Create sign transaction keyboard with tx_id parameter
pub fn make_sign_keyboard(tx_id: &str) -> Option<InlineKeyboardMarkup> {
    get_mini_app_url().map(|base_url| {
        // Add /sign path and tx_id as start_param
        let url = format!("{}/sign?tx_id={}", base_url, tx_id);
        InlineKeyboardMarkup::new([[InlineKeyboardButton::web_app(
            "🔐 Sign Transaction",
            WebAppInfo {
                url: url.parse().unwrap(),
            },
        )]])
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
    session_manager: &Arc<SessionManager>,
) -> Result<bool> {
    let text = message.text().unwrap_or("");

    let (cmd, args) = match parse_command(text) {
        Some(c) => c,
        None => return Ok(false),
    };

    match cmd {
        "namespace" => {
            handle_namespace(bot, message, pool, session_manager, args).await?;
            Ok(true)
        }
        "model" => {
            handle_model(bot, message, pool, session_manager, args).await?;
            Ok(true)
        }
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
        "sign" => {
            // /sign <tx_id> - used by agent to prompt user to sign
            handle_sign(bot, message, args).await?;
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

async fn handle_namespace(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
    args: &str,
) -> Result<()> {
    let chat_id = message.chat.id;

    if !message.chat.is_private() {
        bot.bot
            .send_message(chat_id, "This command is available in DMs only.")
            .await?;
        return Ok(());
    }

    let user_id = match message.from.as_ref().map(|u| u.id) {
        Some(id) => id,
        None => {
            bot.bot
                .send_message(chat_id, "Missing user information.")
                .await?;
            return Ok(());
        }
    };

    let session_key = dm_session_key(user_id);
    let wallet_service = DbWalletConnectService::new(pool.clone());
    let pub_key = match wallet_service.get_bound_wallet(&session_key).await {
        Ok(wallet) => wallet,
        Err(e) => {
            warn!(
                "Failed to load bound wallet for session {}: {}",
                session_key, e
            );
            None
        }
    };

    let user_namespaces = if let Some(ref pk) = pub_key {
        session_manager.get_user_namespaces(pk).await.ok()
    } else {
        None
    };
    let mut auth = NamespaceAuth::new(pub_key.clone(), None, None);
    auth.merge_authorization(user_namespaces.clone());
    let mut namespaces = auth.current_authorization;
    namespaces.sort();

    let current_namespace = session_manager
        .get_session_config(&session_key)
        .map(|(namespace, _)| namespace.as_str())
        .unwrap_or(Namespace::Default.as_str());

    let arg = args.split_whitespace().next().unwrap_or("");
    match arg {
        "" => {
            let other_namespaces: Vec<String> = namespaces
                .iter()
                .filter(|ns| !ns.eq_ignore_ascii_case(current_namespace))
                .cloned()
                .collect();

            let mut message = format!("Current namespace: {}", current_namespace);
            if other_namespaces.is_empty() {
                message.push_str("\nOther available namespaces: none");
            } else {
                message.push_str("\nOther available namespaces:\n- ");
                message.push_str(&other_namespaces.join("\n- "));
            }

            bot.bot.send_message(chat_id, message).await?;
        }
        "list" => {
            let message = if namespaces.is_empty() {
                "No namespaces available.".to_string()
            } else {
                format!("Available namespaces:\n- {}", namespaces.join("\n- "))
            };
            bot.bot.send_message(chat_id, message).await?;
        }
        "show" => {
            bot.bot
                .send_message(
                    chat_id,
                    format!("Current namespace: {}", current_namespace),
                )
                .await?;
        }
        _ => {
            let Some(namespace) = Namespace::parse(arg) else {
                bot.bot
                    .send_message(chat_id, "Unknown namespace. Use /namespace list.")
                    .await?;
                return Ok(());
            };

            let current_selection = session_manager
                .get_session_config(&session_key)
                .map(|(_, selection)| selection)
                .unwrap_or_default();

            let mut auth = NamespaceAuth::new(pub_key.clone(), None, Some(namespace.as_str()));
            auth.merge_authorization(user_namespaces);

            if !auth.is_authorized() {
                bot.bot
                    .send_message(
                        chat_id,
                        "Not authorized for that namespace. Connect a wallet or ask an admin.",
                    )
                    .await?;
                return Ok(());
            }

            if let Err(e) = session_manager
                .get_or_create_session(&session_key, &mut auth, Some(current_selection))
                .await
            {
                warn!(
                    "Failed to switch namespace for session {}: {}",
                    session_key, e
                );
                bot.bot
                    .send_message(chat_id, "Failed to switch namespace.")
                    .await?;
                return Ok(());
            }

            bot.bot
                .send_message(chat_id, format!("Namespace set to {}", namespace.as_str()))
                .await?;
        }
    }

    Ok(())
}

async fn handle_model(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
    args: &str,
) -> Result<()> {
    let chat_id = message.chat.id;

    if !message.chat.is_private() {
        bot.bot
            .send_message(chat_id, "This command is available in DMs only.")
            .await?;
        return Ok(());
    }

    let user_id = match message.from.as_ref().map(|u| u.id) {
        Some(id) => id,
        None => {
            bot.bot
                .send_message(chat_id, "Missing user information.")
                .await?;
            return Ok(());
        }
    };

    let session_key = dm_session_key(user_id);
    let wallet_service = DbWalletConnectService::new(pool.clone());
    let pub_key = match wallet_service.get_bound_wallet(&session_key).await {
        Ok(wallet) => wallet,
        Err(e) => {
            warn!(
                "Failed to load bound wallet for session {}: {}",
                session_key, e
            );
            None
        }
    };

    let selection = session_manager
        .get_session_config(&session_key)
        .map(|(_, selection)| selection)
        .unwrap_or_default();
    let current_model = selection.rig;

    let arg = args.split_whitespace().next().unwrap_or("");
    match arg {
        "" => {
            let other_models: Vec<_> = AomiModel::rig_all()
                .iter()
                .copied()
                .filter(|model| *model != current_model)
                .collect();

            let mut message = format!(
                "Current model: {} ({})",
                current_model.rig_label(),
                current_model.rig_slug()
            );

            if other_models.is_empty() {
                message.push_str("\nOther available models: none");
            } else {
                message.push_str("\nOther available models:\n- ");
                let lines: Vec<String> = other_models
                    .into_iter()
                    .map(|model| format!("{} ({})", model.rig_label(), model.rig_slug()))
                    .collect();
                message.push_str(&lines.join("\n- "));
            }

            bot.bot.send_message(chat_id, message).await?;
        }
        "list" => {
            let mut lines = Vec::new();
            lines.push("Available models:".to_string());
            for model in AomiModel::rig_all() {
                lines.push(format!("- {} ({})", model.rig_label(), model.rig_slug()));
            }
            bot.bot.send_message(chat_id, lines.join("\n")).await?;
        }
        "show" => {
            bot.bot
                .send_message(
                    chat_id,
                    format!(
                        "Current model: {} ({})",
                        current_model.rig_label(),
                        current_model.rig_slug()
                    ),
                )
                .await?;
        }
        _ => {
            let Some(model) = AomiModel::parse_rig(arg) else {
                bot.bot
                    .send_message(chat_id, "Unknown model. Use /model list.")
                    .await?;
                return Ok(());
            };

            let (current_namespace, mut selection) = session_manager
                .get_session_config(&session_key)
                .map(|(namespace, selection)| (namespace, selection))
                .unwrap_or((Namespace::Default, Selection::default()));

            selection.rig = model;

            let mut auth =
                NamespaceAuth::new(pub_key.clone(), None, Some(current_namespace.as_str()));
            let user_namespaces = if let Some(ref pk) = pub_key {
                session_manager.get_user_namespaces(pk).await.ok()
            } else {
                None
            };
            auth.merge_authorization(user_namespaces);

            if !auth.is_authorized() {
                bot.bot
                    .send_message(
                        chat_id,
                        "Not authorized for the current namespace. Connect a wallet or ask an admin.",
                    )
                    .await?;
                return Ok(());
            }

            if let Err(e) = session_manager
                .get_or_create_session(&session_key, &mut auth, Some(selection))
                .await
            {
                warn!(
                    "Failed to switch model for session {}: {}",
                    session_key, e
                );
                bot.bot
                    .send_message(chat_id, "Failed to update model.")
                    .await?;
                return Ok(());
            }

            bot.bot
                .send_message(
                    chat_id,
                    format!("Model set to {} ({})", model.rig_label(), model.rig_slug()),
                )
                .await?;
        }
    }

    Ok(())
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
        bot.bot
            .send_message(chat_id, msg)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .reply_markup(keyboard)
            .await?;
    } else {
        bot.bot
            .send_message(chat_id, msg)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
    }

    Ok(())
}

/// Handle /connect command - opens Mini App if available.
async fn handle_connect(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;

    if let Some(keyboard) = make_connect_keyboard() {
        bot.bot
            .send_message(chat_id, "Tap the button below to connect your wallet:")
            .reply_markup(keyboard)
            .await?;
    } else {
        // No HTTPS URL available - show manual instructions
        bot.bot
            .send_message(
                chat_id,
                "⚠️ Wallet connect is not configured\\.\n\n\
            Please ask the admin to set a valid HTTPS URL for `MINI_APP_URL`\\.",
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
    }

    Ok(())
}

/// Handle /sign command - prompts user to sign a pending transaction.
/// Usage: /sign <tx_id>
async fn handle_sign(bot: &TelegramBot, message: &Message, tx_id: &str) -> Result<()> {
    let chat_id = message.chat.id;

    if tx_id.is_empty() {
        bot.bot
            .send_message(chat_id, "❌ Missing transaction ID")
            .await?;
        return Ok(());
    }

    if let Some(keyboard) = make_sign_keyboard(tx_id) {
        bot.bot.send_message(chat_id,
            "🔐 *Transaction requires your signature*\n\nTap the button below to review and sign\\."
        )
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .reply_markup(keyboard)
        .await?;
    } else {
        bot.bot
            .send_message(
                chat_id,
                "⚠️ Signing is not available\\. Please configure MINI\\_APP\\_URL\\.",
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
    }

    Ok(())
}

/// Handle /wallet command.
async fn handle_wallet(bot: &TelegramBot, message: &Message, pool: &Pool<Any>) -> Result<()> {
    let chat_id = message.chat.id;
    let user_id = message.from.as_ref().map(|u| u.id).unwrap();
    let session_key = dm_session_key(user_id);

    let wallet_service = DbWalletConnectService::new(pool.clone());

    match wallet_service.get_bound_wallet(&session_key).await {
        Ok(Some(address)) => {
            let msg = format!("💳 *Connected wallet:*\n\n`{}`", address);

            if get_mini_app_url().is_some() {
                let change_keyboard = InlineKeyboardMarkup::new([[InlineKeyboardButton::web_app(
                    "🔄 Change Wallet",
                    WebAppInfo {
                        url: get_mini_app_url().unwrap().parse().unwrap(),
                    },
                )]]);
                bot.bot
                    .send_message(chat_id, msg)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .reply_markup(change_keyboard)
                    .await?;
            } else {
                bot.bot
                    .send_message(chat_id, msg)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await?;
            }
        }
        Ok(None) => {
            if let Some(keyboard) = make_connect_keyboard() {
                bot.bot
                    .send_message(chat_id, "No wallet connected\\. Tap below to connect:")
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .reply_markup(keyboard)
                    .await?;
            } else {
                bot.bot
                    .send_message(
                        chat_id,
                        "No wallet connected\\. Use /connect to link your wallet\\.",
                    )
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await?;
            }
        }
        Err(e) => {
            bot.bot
                .send_message(chat_id, format!("❌ Error: {}", e))
                .await?;
        }
    }

    Ok(())
}

/// Handle /disconnect command.
async fn handle_disconnect(bot: &TelegramBot, message: &Message, pool: &Pool<Any>) -> Result<()> {
    let chat_id = message.chat.id;
    let user_id = message.from.as_ref().map(|u| u.id).unwrap();
    let session_key = dm_session_key(user_id);

    let wallet_service = DbWalletConnectService::new(pool.clone());

    match wallet_service.disconnect(&session_key).await {
        Ok(()) => {
            bot.bot
                .send_message(chat_id, "✅ Wallet disconnected\\.")
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await?;
            info!("User {} disconnected wallet", user_id);
        }
        Err(e) => {
            bot.bot
                .send_message(chat_id, format!("❌ Error: {}", e))
                .await?;
        }
    }

    Ok(())
}

/// Handle /help command.
async fn handle_help(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;

    bot.bot
        .send_message(
            chat_id,
            "🤖 *Aomi Commands*\n\n\
        /connect \\- Link your Ethereum wallet\n\
        /namespace \\- Show or switch namespace\n\
        /model \\- Show or switch model\n\
        /wallet \\- Show connected wallet\n\
        /disconnect \\- Unlink your wallet\n\
        /help \\- Show this message\n\n\
        Or just chat with me naturally\\!",
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
        assert_eq!(parse_command("/sign tx_123"), Some(("sign", "tx_123")));
        assert_eq!(
            parse_command("/namespace polymarket"),
            Some(("namespace", "polymarket"))
        );
        assert_eq!(parse_command("/namespace list"), Some(("namespace", "list")));
        assert_eq!(parse_command("/model opus-4"), Some(("model", "opus-4")));
        assert_eq!(parse_command("/model list"), Some(("model", "list")));
        assert_eq!(parse_command("hello"), None);
        assert_eq!(parse_command(""), None);
    }
}
