use anyhow::Result;
use async_trait::async_trait;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, WebAppInfo};
use tracing::{info, warn};

use aomi_bot_core::{DbWalletConnectService, WalletConnectService};

use super::{Panel, PanelCtx, PanelView, Transition};

pub struct WalletPanel;

/// Get Mini App URL - returns None if not HTTPS (Telegram requirement).
fn get_mini_app_url() -> Option<String> {
    let url = std::env::var("MINI_APP_URL").ok()?;
    if url.starts_with("https://") {
        Some(url)
    } else {
        warn!("MINI_APP_URL must be HTTPS for Telegram Web Apps: {}", url);
        None
    }
}

fn connect_button() -> InlineKeyboardButton {
    if let Some(connect_url) = get_mini_app_url() {
        InlineKeyboardButton::web_app(
            "Connect Wallet",
            WebAppInfo {
                url: connect_url.parse().unwrap(),
            },
        )
    } else {
        InlineKeyboardButton::callback("Connect Wallet", "p:wallet:unavailable".to_string())
    }
}

fn get_create_wallet_url() -> Option<String> {
    let base = std::env::var("MINI_APP_URL").ok()?;
    if !base.starts_with("https://") {
        warn!("MINI_APP_URL must be HTTPS for Telegram Web Apps: {}", base);
        return None;
    }
    let base = base.trim_end_matches('/');
    Some(format!("{base}/create-wallet"))
}

fn create_wallet_button() -> InlineKeyboardButton {
    if let Some(url) = get_create_wallet_url() {
        InlineKeyboardButton::web_app(
            "Create AA Wallet",
            WebAppInfo {
                url: url.parse().unwrap(),
            },
        )
    } else {
        InlineKeyboardButton::callback("Create AA Wallet", "p:wallet:create".to_string())
    }
}

fn make_connect_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![connect_button(), create_wallet_button()],
        vec![InlineKeyboardButton::callback("Back", "p:start")],
    ])
}

#[async_trait]
impl Panel for WalletPanel {
    fn prefix(&self) -> &'static str {
        "wallet"
    }

    fn commands(&self) -> &'static [&'static str] {
        &["connect", "wallet", "disconnect"]
    }

    async fn render(&self, _ctx: &PanelCtx<'_>) -> Result<PanelView> {
        let msg = if get_mini_app_url().is_some() {
            "Choose how you want to get started with your wallet:"
        } else {
            "Wallet mini-app is not configured. Ask admin to set a valid HTTPS <code>MINI_APP_URL</code>."
        };

        Ok(PanelView {
            text: msg.to_string(),
            keyboard: Some(make_connect_keyboard()),
        })
    }

    async fn on_callback(&self, ctx: &PanelCtx<'_>, data: &str) -> Result<Transition> {
        match data {
            "create" => {
                if !ctx.is_private {
                    return Ok(Transition::Toast(
                        "For security, wallet creation is available only in direct chat with the bot.".to_string(),
                    ));
                }
                // Fallback when MINI_APP_URL is unavailable
                Ok(Transition::Toast(
                    "Create Wallet mini-app is not configured. Ask admin to set a valid HTTPS `MINI_APP_URL`.".to_string(),
                ))
            }
            "unavailable" => Ok(Transition::Toast(
                "Connect Wallet mini-app is not configured. Ask admin to set a valid HTTPS `MINI_APP_URL`.".to_string(),
            )),
            _ => Ok(Transition::None),
        }
    }

    async fn on_command(
        &self,
        ctx: &PanelCtx<'_>,
        command: &str,
        _args: &str,
    ) -> Result<Transition> {
        match command {
            "connect" => Ok(Transition::None), // falls back to render (shows connect keyboard)
            "wallet" => {
                let wallet_service = DbWalletConnectService::new(ctx.pool.clone());
                match wallet_service.get_bound_wallet(&ctx.session_key).await {
                    Ok(Some(address)) => {
                        let view = PanelView {
                            text: format!("<b>Connected wallet:</b>\n\n<code>{}</code>", address),
                            keyboard: Some(make_connect_keyboard()),
                        };
                        Ok(Transition::Render(view))
                    }
                    Ok(None) => {
                        let view = PanelView {
                            text: "No wallet connected. Tap below to connect or create one:".to_string(),
                            keyboard: Some(make_connect_keyboard()),
                        };
                        Ok(Transition::Render(view))
                    }
                    Err(e) => Ok(Transition::Toast(format!("Error: {}", e))),
                }
            }
            "disconnect" => {
                let wallet_service = DbWalletConnectService::new(ctx.pool.clone());
                match wallet_service.disconnect(&ctx.session_key).await {
                    Ok(()) => {
                        info!("User disconnected wallet for session {}", ctx.session_key);
                        Ok(Transition::Toast("Wallet disconnected.".to_string()))
                    }
                    Err(e) => Ok(Transition::Toast(format!("Error: {}", e))),
                }
            }
            _ => Ok(Transition::None),
        }
    }
}
