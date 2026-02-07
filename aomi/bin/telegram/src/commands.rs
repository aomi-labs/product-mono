//! Slash command handlers that don't fit into the panel system.

use anyhow::Result;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, WebAppInfo};
use tracing::warn;

use crate::send::with_thread_id;
use crate::TelegramBot;

/// Get Mini App URL - returns None if not HTTPS (Telegram requirement).
pub(crate) fn get_mini_app_url() -> Option<String> {
    let url = std::env::var("MINI_APP_URL").ok()?;
    if url.starts_with("https://") {
        Some(url)
    } else {
        warn!("MINI_APP_URL must be HTTPS for Telegram Web Apps: {}", url);
        None
    }
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

/// Create sign transaction keyboard with tx_id parameter.
pub fn make_sign_keyboard(tx_id: &str) -> Option<InlineKeyboardMarkup> {
    get_mini_app_url().map(|base_url| {
        let url = format!("{}/sign?tx_id={}", base_url, tx_id);
        InlineKeyboardMarkup::new([[InlineKeyboardButton::web_app(
            "Sign Transaction",
            WebAppInfo {
                url: url.parse().unwrap(),
            },
        )]])
    })
}

/// Handle /sign command - prompts user to sign a pending transaction.
pub async fn handle_sign(bot: &TelegramBot, message: &Message, tx_id: &str) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;

    if tx_id.is_empty() {
        with_thread_id(
            bot.bot.send_message(chat_id, "Missing transaction ID"),
            thread_id,
        )
        .await?;
        return Ok(());
    }

    if let Some(keyboard) = make_sign_keyboard(tx_id) {
        with_thread_id(
            bot.bot.send_message(
                chat_id,
                "🔐 *Transaction requires your signature*\n\nTap the button below to review and sign\\.",
            ),
            thread_id,
        )
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .reply_markup(keyboard)
        .await?;
    } else {
        with_thread_id(
            bot.bot.send_message(
                chat_id,
                "⚠️ Signing is not available\\. Please configure MINI\\_APP\\_URL\\.",
            ),
            thread_id,
        )
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;
    }

    Ok(())
}

/// Handle /help command.
pub async fn handle_help(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;

    with_thread_id(
        bot.bot.send_message(
            chat_id,
            "🤖 *Aomi Commands*\n\n\
        /start \\- Show main action buttons\n\
        /connect \\- Connect or create a wallet\n\
        /namespace \\- Open namespace picker\n\
        /model \\- Open model picker\n\
        /sessions \\- List your sessions\n\
        /status \\- Show wallet + session status\n\
        /apikey \\- Set API key for namespaces\n\
        /settings \\- Settings panel\n\
        /wallet \\- Show connected wallet\n\
        /disconnect \\- Unlink your wallet\n\
        /help \\- Show this message\n\n\
        Tip: use /start and tap buttons instead of typing selections\\.",
        ),
        thread_id,
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
        assert_eq!(
            parse_command("/namespace list"),
            Some(("namespace", "list"))
        );
        assert_eq!(parse_command("/model opus-4"), Some(("model", "opus-4")));
        assert_eq!(parse_command("/model list"), Some(("model", "list")));
        assert_eq!(parse_command("hello"), None);
        assert_eq!(parse_command(""), None);
    }
}
