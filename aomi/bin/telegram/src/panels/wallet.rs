use anyhow::Result;
use async_trait::async_trait;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, WebAppInfo};
use tracing::info;

use aomi_bot_core::WalletConnectService;

use super::{Panel, PanelCtx, PanelView, Transition};

pub struct WalletPanel {
    keyboard: InlineKeyboardMarkup,
}

impl WalletPanel {
    pub fn new(config: &crate::config::TelegramConfig) -> Self {
        let mini_app_url = config.mini_app_url.as_str();
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::web_app(
                    "Connect Wallet",
                    WebAppInfo {
                        url: mini_app_url.parse().unwrap(),
                    },
                ),
                InlineKeyboardButton::web_app(
                    "Create AA Wallet",
                    WebAppInfo {
                        url: format!("{}/create-wallet", mini_app_url).parse().unwrap(),
                    },
                ),
            ],
            vec![InlineKeyboardButton::callback("Back", "p:start")],
        ]);
        Self { keyboard }
    }
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
        Ok(PanelView {
            text: "Choose how you want to get started with your wallet:".to_string(),
            keyboard: Some(self.keyboard.clone()),
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
                // Fallback when mini_app_url is unavailable
                Ok(Transition::Toast(
                    "Create Wallet mini-app is not configured. Set mini_app_url in bot.toml.".to_string(),
                ))
            }
            "unavailable" => Ok(Transition::Toast(
                "Connect Wallet mini-app is not configured. Set mini_app_url in bot.toml.".to_string(),
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
                match ctx.get_bound_wallet(&ctx.session_key).await {
                    Ok(Some(address)) => {
                        let view = PanelView {
                            text: format!("<b>Connected wallet:</b>\n\n<code>{}</code>", address),
                            keyboard: Some(self.keyboard.clone()),
                        };
                        Ok(Transition::Render(view))
                    }
                    Ok(None) => {
                        let view = PanelView {
                            text: "No wallet connected. Tap below to connect or create one:".to_string(),
                            keyboard: Some(self.keyboard.clone()),
                        };
                        Ok(Transition::Render(view))
                    }
                    Err(e) => Ok(Transition::Toast(format!("Error: {}", e))),
                }
            }
            "disconnect" => {
                match ctx.disconnect(&ctx.session_key).await {
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
