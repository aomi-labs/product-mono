use anyhow::Result;
use async_trait::async_trait;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, WebAppInfo};

use crate::send::escape_html;

use super::{Panel, PanelCtx, PanelView, Transition};

pub struct SessionsPanel {
    connect_keyboard: InlineKeyboardMarkup,
}

impl SessionsPanel {
    pub fn new(config: &crate::config::TelegramConfig) -> Self {
        let mini_app_url = config.mini_app_url.as_str();
        let connect_keyboard = InlineKeyboardMarkup::new(vec![
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
                        url: format!("{}/create-wallet", mini_app_url)
                            .parse()
                            .unwrap(),
                    },
                ),
            ],
            vec![InlineKeyboardButton::callback("Back", "p:start")],
        ]);

        Self { connect_keyboard }
    }
}

#[async_trait]
impl Panel for SessionsPanel {
    fn prefix(&self) -> &'static str {
        "sess"
    }

    fn commands(&self) -> &'static [&'static str] {
        &["sessions"]
    }

    async fn render(&self, ctx: &PanelCtx<'_>) -> Result<PanelView> {
        let Some(pub_key) = ctx.bound_wallet().await else {
            return Ok(PanelView {
                text: "No wallet connected. Tap below to connect or create one:".to_string(),
                keyboard: Some(self.connect_keyboard.clone()),
            });
        };

        let sessions = ctx
            .session_manager
            .list_sessions(&pub_key, 20)
            .await
            .unwrap_or_default();

        if sessions.is_empty() {
            return Ok(PanelView {
                text: "No sessions found for this wallet.".to_string(),
                keyboard: None,
            });
        }

        let mut summary = String::from("<b>Sessions</b>\n\nSelect a session:\n");
        for (idx, session) in sessions.iter().enumerate() {
            summary.push_str(&format!(
                "\n{}. {}",
                idx + 1,
                escape_html(&session.title)
            ));
        }

        Ok(PanelView {
            text: summary,
            keyboard: Some({
                let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
                let mut current_row: Vec<InlineKeyboardButton> = Vec::new();

                for (idx, session) in sessions.iter().enumerate() {
                    current_row.push(InlineKeyboardButton::callback(
                        format!("Session {}", idx + 1),
                        format!("p:sess:sel:{}", session.session_id),
                    ));
                    if current_row.len() == 2 {
                        rows.push(std::mem::take(&mut current_row));
                    }
                }

                if !current_row.is_empty() {
                    rows.push(current_row);
                }

                rows.push(vec![InlineKeyboardButton::callback("Back", "p:start")]);
                InlineKeyboardMarkup::new(rows)
            }),
        })
    }

    async fn on_callback(&self, _ctx: &PanelCtx<'_>, data: &str) -> Result<Transition> {
        if let Some(session_id) = data.strip_prefix("sel:") {
            return Ok(Transition::ToastHtml(format!(
                "Selected session <code>{}</code>.",
                escape_html(session_id)
            )));
        }

        Ok(Transition::None)
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
