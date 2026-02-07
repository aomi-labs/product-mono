//! Panel state machine for Telegram bot UI.
//!
//! Each panel owns its keyboard, render logic, and callback handling.
//! The `PanelRouter` dispatches callbacks/commands to the correct panel.

pub mod apikey;
pub mod model;
pub mod namespace;
pub mod sessions;
pub mod settings;
pub mod start;
pub mod status;
pub mod wallet;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sqlx::{Any, Pool};
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{
    CallbackQuery, ChatId, InlineKeyboardMarkup, Message, ParseMode, ThreadId,
};
use tracing::warn;

use aomi_backend::SessionManager;
use aomi_bot_core::{DbWalletConnectService, WalletConnectService};

use crate::send::with_thread_id;
use crate::session::{dm_session_key, session_key_from_message};
use crate::TelegramBot;

/// Shared context for a single panel request.
pub struct PanelCtx<'a> {
    pub bot: &'a TelegramBot,
    pub pool: &'a Pool<Any>,
    pub session_manager: &'a Arc<SessionManager>,
    pub chat_id: ChatId,
    pub thread_id: Option<ThreadId>,
    pub session_key: String,
    pub is_private: bool,
}

impl<'a> PanelCtx<'a> {
    /// Build context from a callback query.
    pub fn from_callback(
        bot: &'a TelegramBot,
        pool: &'a Pool<Any>,
        session_manager: &'a Arc<SessionManager>,
        query: &CallbackQuery,
    ) -> Self {
        let chat_id = query
            .message
            .as_ref()
            .map(|msg| msg.chat().id)
            .unwrap_or(ChatId(query.from.id.0 as i64));

        let thread_id = query
            .message
            .as_ref()
            .and_then(|msg| msg.regular_message())
            .and_then(|msg| msg.thread_id);

        let session_key = query
            .message
            .as_ref()
            .and_then(|msg| msg.regular_message())
            .and_then(session_key_from_message)
            .unwrap_or_else(|| dm_session_key(query.from.id));

        let is_private = query
            .message
            .as_ref()
            .is_some_and(|msg| msg.chat().is_private());

        Self {
            bot,
            pool,
            session_manager,
            chat_id,
            thread_id,
            session_key,
            is_private,
        }
    }

    /// Build context from a message.
    pub fn from_message(
        bot: &'a TelegramBot,
        pool: &'a Pool<Any>,
        session_manager: &'a Arc<SessionManager>,
        message: &Message,
    ) -> Self {
        let chat_id = message.chat.id;
        let thread_id = message.thread_id;
        let session_key = session_key_from_message(message)
            .or_else(|| message.from.as_ref().map(|u| dm_session_key(u.id)))
            .unwrap_or_default();
        let is_private = message.chat.is_private();

        Self {
            bot,
            pool,
            session_manager,
            chat_id,
            thread_id,
            session_key,
            is_private,
        }
    }

    /// Get the wallet bound to this session (if any).
    pub async fn bound_wallet(&self) -> Option<String> {
        let wallet_service = DbWalletConnectService::new(self.pool.clone());
        match wallet_service.get_bound_wallet(&self.session_key).await {
            Ok(wallet) => wallet,
            Err(e) => {
                warn!(
                    "Failed to load bound wallet for session {}: {}",
                    self.session_key, e
                );
                None
            }
        }
    }

    /// Send a plain text message.
    pub async fn send(&self, text: &str) -> Result<()> {
        with_thread_id(
            self.bot.bot.send_message(self.chat_id, text),
            self.thread_id,
        )
        .await?;
        Ok(())
    }

    /// Send an HTML message with optional inline keyboard.
    pub async fn send_html(
        &self,
        text: &str,
        keyboard: Option<InlineKeyboardMarkup>,
    ) -> Result<()> {
        let mut req = with_thread_id(
            self.bot.bot.send_message(self.chat_id, text),
            self.thread_id,
        )
        .parse_mode(ParseMode::Html);

        if let Some(kb) = keyboard {
            req = req.reply_markup(kb);
        }

        req.await?;
        Ok(())
    }
}

/// What a panel renders.
pub struct PanelView {
    pub text: String,
    pub keyboard: Option<InlineKeyboardMarkup>,
}

/// What a panel handler returns.
pub enum Transition {
    /// Navigate to another panel by prefix (calls its render()).
    #[allow(dead_code)]
    Navigate(String),
    /// Send this view directly.
    Render(PanelView),
    /// Send a plain text toast.
    Toast(String),
    /// Send an HTML toast.
    ToastHtml(String),
    /// Do nothing.
    None,
}

/// A self-contained UI panel.
#[async_trait]
pub trait Panel: Send + Sync {
    /// Unique prefix used in callback data (e.g. "start", "ns", "model").
    fn prefix(&self) -> &'static str;

    /// Slash commands this panel handles (e.g. &["start"]).
    fn commands(&self) -> &'static [&'static str] {
        &[]
    }

    /// Render the panel view.
    async fn render(&self, ctx: &PanelCtx<'_>) -> Result<PanelView>;

    /// Handle a callback action (the part after `p:{prefix}:`).
    /// Empty string means the panel itself was clicked (navigate to it).
    async fn on_callback(&self, _ctx: &PanelCtx<'_>, _data: &str) -> Result<Transition> {
        Ok(Transition::None)
    }

    /// Handle a slash command. `args` is the text after the command name.
    async fn on_command(&self, _ctx: &PanelCtx<'_>, _command: &str, _args: &str) -> Result<Transition> {
        Ok(Transition::None)
    }

    /// Handle text input (e.g. ForceReply responses).
    async fn on_text(&self, _ctx: &PanelCtx<'_>, _text: &str) -> Result<Transition> {
        Ok(Transition::None)
    }
}

/// Routes callbacks and commands to the correct panel.
pub struct PanelRouter {
    panels: HashMap<String, Box<dyn Panel>>,
    commands: HashMap<String, String>, // slash cmd -> prefix
}

impl PanelRouter {
    fn new() -> Self {
        Self {
            panels: HashMap::new(),
            commands: HashMap::new(),
        }
    }

    fn register(&mut self, panel: Box<dyn Panel>) {
        let prefix = panel.prefix().to_string();
        for cmd in panel.commands() {
            self.commands.insert(cmd.to_string(), prefix.clone());
        }
        self.panels.insert(prefix, panel);
    }

    /// Handle a callback query. Returns true if handled.
    pub async fn handle_callback(&self, ctx: &PanelCtx<'_>, raw_data: &str) -> Result<bool> {
        // All panel callbacks start with "p:"
        let Some(rest) = raw_data.strip_prefix("p:") else {
            return Ok(false);
        };

        // Split into prefix and action: "ns:set:polymarket" -> ("ns", "set:polymarket")
        let (prefix, action) = match rest.find(':') {
            Some(pos) => (&rest[..pos], &rest[pos + 1..]),
            None => (rest, ""),
        };

        let Some(panel) = self.panels.get(prefix) else {
            return Ok(false);
        };

        let transition = if action.is_empty() {
            // Navigate to panel (render it)
            let view = panel.render(ctx).await?;
            Transition::Render(view)
        } else {
            panel.on_callback(ctx, action).await?
        };

        self.apply_transition(ctx, transition).await?;
        Ok(true)
    }

    /// Handle a slash command. Returns true if handled.
    pub async fn handle_command(
        &self,
        ctx: &PanelCtx<'_>,
        cmd_name: &str,
        args: &str,
    ) -> Result<bool> {
        let Some(prefix) = self.commands.get(cmd_name) else {
            return Ok(false);
        };

        let Some(panel) = self.panels.get(prefix.as_str()) else {
            return Ok(false);
        };

        let transition = panel.on_command(ctx, cmd_name, args).await?;

        // If the panel returned None for the command, fall back to render
        let transition = match transition {
            Transition::None => {
                let view = panel.render(ctx).await?;
                Transition::Render(view)
            }
            other => other,
        };

        self.apply_transition(ctx, transition).await?;
        Ok(true)
    }

    /// Handle text input for a specific panel prefix. Returns true if handled.
    pub async fn handle_text(
        &self,
        prefix: &str,
        ctx: &PanelCtx<'_>,
        text: &str,
    ) -> Result<bool> {
        let Some(panel) = self.panels.get(prefix) else {
            return Ok(false);
        };

        let transition = panel.on_text(ctx, text).await?;
        self.apply_transition(ctx, transition).await?;
        Ok(true)
    }

    /// Apply a transition by sending the appropriate response.
    async fn apply_transition(&self, ctx: &PanelCtx<'_>, transition: Transition) -> Result<()> {
        match transition {
            Transition::Navigate(prefix) => {
                if let Some(panel) = self.panels.get(&prefix) {
                    let view = panel.render(ctx).await?;
                    ctx.send_html(&view.text, view.keyboard).await?;
                }
            }
            Transition::Render(view) => {
                if !view.text.is_empty() {
                    ctx.send_html(&view.text, view.keyboard).await?;
                }
            }
            Transition::Toast(text) => {
                ctx.send(&text).await?;
            }
            Transition::ToastHtml(text) => {
                ctx.send_html(&text, None).await?;
            }
            Transition::None => {}
        }
        Ok(())
    }
}

/// Build the panel router with all panels registered.
pub fn build_router() -> PanelRouter {
    let mut router = PanelRouter::new();
    router.register(Box::new(start::StartPanel));
    router.register(Box::new(settings::SettingsPanel));
    router.register(Box::new(namespace::NamespacePanel));
    router.register(Box::new(model::ModelPanel));
    router.register(Box::new(wallet::WalletPanel));
    router.register(Box::new(apikey::ApiKeyPanel));
    router.register(Box::new(sessions::SessionsPanel));
    router.register(Box::new(status::StatusPanel));
    router
}
