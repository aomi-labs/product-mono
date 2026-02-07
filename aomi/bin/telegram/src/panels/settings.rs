use anyhow::Result;
use async_trait::async_trait;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use aomi_bot_core::{DbWalletConnectService, WalletConnectService};

use super::{Panel, PanelCtx, PanelView, Transition};

pub struct SettingsPanel;

fn make_settings_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("Archive Session", "p:settings:archive"),
            InlineKeyboardButton::callback("Delete Wallet", "p:settings:delete_wallet"),
        ],
        vec![InlineKeyboardButton::callback("Back", "p:start")],
    ])
}

#[async_trait]
impl Panel for SettingsPanel {
    fn prefix(&self) -> &'static str {
        "settings"
    }

    fn commands(&self) -> &'static [&'static str] {
        &["settings"]
    }

    async fn render(&self, ctx: &PanelCtx<'_>) -> Result<PanelView> {
        if !ctx.is_private {
            return Ok(PanelView {
                text: "Settings are available in DMs only.".to_string(),
                keyboard: None,
            });
        }

        Ok(PanelView {
            text: "<b>Settings</b>\n\nChoose an action:".to_string(),
            keyboard: Some(make_settings_keyboard()),
        })
    }

    async fn on_callback(&self, ctx: &PanelCtx<'_>, data: &str) -> Result<Transition> {
        match data {
            "archive" => {
                ctx.session_manager
                    .set_session_archived(&ctx.session_key, true);
                Ok(Transition::Toast("Session archived.".to_string()))
            }
            "delete_wallet" => {
                let wallet_service = DbWalletConnectService::new(ctx.pool.clone());
                match wallet_service.disconnect(&ctx.session_key).await {
                    Ok(()) => Ok(Transition::Toast("Wallet deleted.".to_string())),
                    Err(e) => Ok(Transition::Toast(format!("Error: {}", e))),
                }
            }
            _ => Ok(Transition::None),
        }
    }

    async fn on_command(
        &self,
        ctx: &PanelCtx<'_>,
        _command: &str,
        _args: &str,
    ) -> Result<Transition> {
        if !ctx.is_private {
            return Ok(Transition::Toast(
                "Settings are available in DMs only.".to_string(),
            ));
        }
        Ok(Transition::None) // falls back to render
    }
}
