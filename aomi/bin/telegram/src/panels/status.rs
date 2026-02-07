use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use aomi_backend::UserState;

use crate::send::escape_html;
use crate::TelegramBot;

use super::{Panel, PanelCtx, PanelView, Transition};

pub struct StatusPanel {
    keyboard: InlineKeyboardMarkup,
}

impl StatusPanel {
    pub fn new() -> Self {
        Self {
            keyboard: InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::callback("Back", "p:start"),
            ]]),
        }
    }
}

fn backend_base_url(bot: &TelegramBot) -> Option<String> {
    bot.config.backend_url.clone()
}

async fn fetch_backend_user_state(
    bot: &TelegramBot,
    session_key: &str,
) -> Result<Option<UserState>> {
    let Some(base_url) = backend_base_url(bot) else {
        return Ok(None);
    };

    let url = format!("{}/api/state", base_url.trim_end_matches('/'));
    let response = Client::new()
        .get(url)
        .header("X-Session-Id", session_key)
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let payload: Value = response.json().await?;
    let Some(state_value) = payload.get("user_state") else {
        return Ok(None);
    };
    if state_value.is_null() {
        return Ok(None);
    }

    let state: UserState = serde_json::from_value(state_value.clone())?;
    Ok(Some(state))
}

async fn resolve_chain_info(user_state: Option<&UserState>) -> (String, u64, String) {
    let chain_id = user_state.and_then(|state| state.chain_id).unwrap_or(0);
    let manager = match aomi_anvil::provider_manager().await {
        Ok(manager) => manager,
        Err(_) => {
            return (
                "unknown".to_string(),
                chain_id,
                "unknown".to_string(),
            )
        }
    };

    let info = if chain_id > 0 {
        manager.get_instance_info_by_query(Some(chain_id), None)
    } else {
        manager.get_instance_info_by_query(None, None)
    };

    if let Some(info) = info {
        (info.name, info.chain_id, info.endpoint)
    } else {
        (
            "unknown".to_string(),
            chain_id,
            "unknown".to_string(),
        )
    }
}

#[async_trait]
impl Panel for StatusPanel {
    fn prefix(&self) -> &'static str {
        "status"
    }

    fn commands(&self) -> &'static [&'static str] {
        &["status"]
    }

    async fn render(&self, ctx: &PanelCtx<'_>) -> Result<PanelView> {
        if backend_base_url(ctx.bot).is_none() {
            return Ok(PanelView {
                text: "Backend URL is not configured. Set backend_url in bot.toml.".to_string(),
                keyboard: None,
            });
        }

        let user_state = fetch_backend_user_state(ctx.bot, &ctx.session_key).await?;

        let (chain_name, chain_id, rpc_endpoint) = resolve_chain_info(user_state.as_ref()).await;
        let selection = ctx
            .session_manager
            .get_session_config(&ctx.session_key)
            .map(|(_, selection)| selection)
            .unwrap_or_default();
        let title = ctx
            .session_manager
            .get_session_title(&ctx.session_key)
            .unwrap_or_else(|| "New Chat".to_string());

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let (address, is_connected) = match user_state {
            Some(state) => (
                state.address.unwrap_or_else(|| "unknown".to_string()),
                state.is_connected,
            ),
            None => ("not connected".to_string(), false),
        };

        let msg = format!(
            "<b>Status</b>\n\n\
             <b>Wallet:</b> <code>{}</code>\n\
             <b>Connected:</b> {}\n\
             <b>Chain:</b> {} ({})\n\
             <b>RPC:</b> <code>{}</code>\n\
             <b>Model:</b> {}\n\
             <b>Session:</b> {}\n\
             <b>Timestamp:</b> {}",
            escape_html(&address),
            if is_connected { "yes" } else { "no" },
            escape_html(&chain_name),
            chain_id,
            escape_html(&rpc_endpoint),
            escape_html(selection.rig.rig_label()),
            escape_html(&title),
            now,
        );

        Ok(PanelView {
            text: msg,
            keyboard: Some(self.keyboard.clone()),
        })
    }

    async fn on_command(
        &self,
        _ctx: &PanelCtx<'_>,
        _command: &str,
        _args: &str,
    ) -> Result<Transition> {
        Ok(Transition::None) // falls back to render
    }
}
