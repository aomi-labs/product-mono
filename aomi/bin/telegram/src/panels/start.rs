use anyhow::Result;
use async_trait::async_trait;
use teloxide::types::InlineKeyboardButton;
use teloxide::types::InlineKeyboardMarkup;

use super::{Panel, PanelCtx, PanelView, Transition};

pub struct StartPanel {
    keyboard: InlineKeyboardMarkup,
}

impl StartPanel {
    pub fn new() -> Self {
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::callback("Namespace", "p:ns"),
                InlineKeyboardButton::callback("Models", "p:model"),
            ],
            vec![
                InlineKeyboardButton::callback("Sessions", "p:sess"),
                InlineKeyboardButton::callback("Status", "p:status"),
            ],
            vec![
                InlineKeyboardButton::callback("Wallet", "p:wallet"),
                InlineKeyboardButton::callback("API Key", "p:apikey"),
            ],
            vec![InlineKeyboardButton::callback("Settings", "p:settings")],
        ]);
        Self { keyboard }
    }
}

fn start_message() -> &'static str {
    "<b>Welcome to Aomi!</b>\n\n\
     I'm your DeFi assistant.\n\n\
     Use the panels below to manage sessions, models, and wallet settings."
}

#[async_trait]
impl Panel for StartPanel {
    fn prefix(&self) -> &'static str {
        "start"
    }

    fn commands(&self) -> &'static [&'static str] {
        &["start"]
    }

    async fn render(&self, _ctx: &PanelCtx<'_>) -> Result<PanelView> {
        Ok(PanelView {
            text: start_message().to_string(),
            keyboard: Some(self.keyboard.clone()),
        })
    }

    async fn on_command(
        &self,
        _ctx: &PanelCtx<'_>,
        _command: &str,
        _args: &str,
    ) -> Result<Transition> {
        // /start always renders
        Ok(Transition::None) // falls back to render
    }
}
