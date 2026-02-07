use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{ForceReply, True};

use aomi_backend::AuthorizedKey;

use crate::send::with_thread_id;

use super::{Panel, PanelCtx, PanelView, Transition};

const API_KEY_PROMPT_TEXT: &str =
    "Send us your Aomi API key for exclusive namespace. Reply to this message with your key, or use /apikey <key>.";

pub struct ApiKeyPanel {
    prompt_text: &'static str,
}

impl ApiKeyPanel {
    pub fn new() -> Self {
        Self {
            prompt_text: API_KEY_PROMPT_TEXT,
        }
    }

    pub fn prompt_text(&self) -> &'static str {
        self.prompt_text
    }

    /// Send a ForceReply prompt for the API key.
    async fn send_force_reply(&self, ctx: &PanelCtx<'_>) -> Result<Transition> {
        let reply = ForceReply {
            force_reply: True,
            input_field_placeholder: None,
            selective: false,
        };
        with_thread_id(
            ctx.bot.bot.send_message(ctx.chat_id, self.prompt_text),
            ctx.thread_id,
        )
        .reply_markup(reply)
        .await?;
        Ok(Transition::Render(PanelView {
            text: String::new(),
            keyboard: None,
        }))
    }
}

async fn validate_and_save_key(ctx: &PanelCtx<'_>, key: &str) -> Result<Transition> {
    match AuthorizedKey::new(Arc::new(ctx.pool.clone()), key).await? {
        Some(api_key) => {
            ctx.session_manager
                .set_session_api_key(&ctx.session_key, Some(api_key));
            Ok(Transition::Toast(
                "API key saved. You can switch namespaces now.".to_string(),
            ))
        }
        None => Ok(Transition::Toast(
            "Invalid API key. Try again.".to_string(),
        )),
    }
}

#[async_trait]
impl Panel for ApiKeyPanel {
    fn prefix(&self) -> &'static str {
        "apikey"
    }

    fn commands(&self) -> &'static [&'static str] {
        &["apikey"]
    }

    async fn render(&self, ctx: &PanelCtx<'_>) -> Result<PanelView> {
        if !ctx.is_private {
            return Ok(PanelView {
                text: "API key setup is available in direct chat.".to_string(),
                keyboard: None,
            });
        }

        // For render, we send a ForceReply directly (special case)
        self.send_force_reply(ctx).await?;

        // Return a None-like view since we already sent the message
        Ok(PanelView {
            text: String::new(),
            keyboard: None,
        })
    }

    async fn on_callback(&self, ctx: &PanelCtx<'_>, _data: &str) -> Result<Transition> {
        if !ctx.is_private {
            return Ok(Transition::Toast(
                "API key setup is available in direct chat.".to_string(),
            ));
        }
        self.send_force_reply(ctx).await
    }

    async fn on_text(&self, ctx: &PanelCtx<'_>, text: &str) -> Result<Transition> {
        let candidate = text.trim();
        if candidate.is_empty() {
            return Ok(Transition::Toast(
                "Send a valid Aomi API key.".to_string(),
            ));
        }
        validate_and_save_key(ctx, candidate).await
    }

    async fn on_command(
        &self,
        ctx: &PanelCtx<'_>,
        _command: &str,
        args: &str,
    ) -> Result<Transition> {
        if !ctx.is_private {
            return Ok(Transition::Toast(
                "API key setup is available in DMs only.".to_string(),
            ));
        }

        let key = args.trim();
        if !key.is_empty() {
            return validate_and_save_key(ctx, key).await;
        }

        self.send_force_reply(ctx).await
    }
}
