use anyhow::Result;
use teloxide::prelude::Requester;
use teloxide::types::{ChatId, InlineKeyboardButton, WebAppInfo};
use tracing::warn;

use crate::TelegramBot;

pub const CREATE_WALLET_CALLBACK: &str = "create_aa_wallet";

fn get_create_wallet_url() -> Option<String> {
    let base = std::env::var("MINI_APP_URL").ok()?;
    if !base.starts_with("https://") {
        warn!("MINI_APP_URL must be HTTPS for Telegram Web Apps: {}", base);
        return None;
    }

    let base = base.trim_end_matches('/');
    Some(format!("{base}/create-wallet"))
}

pub fn create_wallet_button() -> InlineKeyboardButton {
    if let Some(url) = get_create_wallet_url() {
        InlineKeyboardButton::web_app(
            "🆕 Create AA Wallet",
            WebAppInfo {
                url: url.parse().unwrap(),
            },
        )
    } else {
        InlineKeyboardButton::callback("🆕 Create AA Wallet", CREATE_WALLET_CALLBACK.to_string())
    }
}

pub async fn handle_create_wallet_callback(bot: &TelegramBot, chat_id: ChatId) -> Result<()> {
    let msg = "⚠️ Create Wallet mini-app is not configured. Ask admin to set a valid HTTPS `MINI_APP_URL`.";
    bot.bot.send_message(chat_id, msg).await?;
    Ok(())
}
