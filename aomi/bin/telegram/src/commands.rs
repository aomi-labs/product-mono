//! Slash command handlers that don't fit into the panel system.

use anyhow::Result;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::Message;
use crate::send::with_thread_id;
use crate::TelegramBot;

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
