use anyhow::Result;
use async_trait::async_trait;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use tracing::warn;

use aomi_backend::{AomiModel, Namespace, NamespaceAuth, Selection};

use crate::send::escape_html;

use super::{Panel, PanelCtx, PanelView, Transition};

pub struct ModelPanel;

const MODEL_OPTIONS: [(AomiModel, &str); 3] = [
    (AomiModel::ClaudeOpus4, "Claude Opus 4.1"),
    (AomiModel::ClaudeSonnet4, "Claude Sonnet 4"),
    (AomiModel::Gpt5, "Codex 5.2"),
];

fn make_model_keyboard(current_model: AomiModel) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = MODEL_OPTIONS
        .chunks(2)
        .map(|chunk| {
            chunk
                .iter()
                .map(|(model, label)| {
                    let slug = model.rig_slug();
                    let display = if *model == current_model {
                        format!("  {}", label)
                    } else {
                        (*label).to_string()
                    };
                    InlineKeyboardButton::callback(display, format!("p:model:set:{slug}"))
                })
                .collect::<Vec<_>>()
        })
        .collect();

    rows.push(vec![InlineKeyboardButton::callback("Back", "p:start")]);
    InlineKeyboardMarkup::new(rows)
}

async fn set_model(ctx: &PanelCtx<'_>, slug: &str) -> Result<Transition> {
    let Some(model) = AomiModel::parse_rig(slug) else {
        return Ok(Transition::Toast("Unknown model.".to_string()));
    };

    let pub_key = ctx.bound_wallet().await;
    let (current_namespace, mut selection) = ctx
        .session_manager
        .get_session_config(&ctx.session_key)
        .map(|(namespace, selection)| (namespace, selection))
        .unwrap_or((Namespace::Default, Selection::default()));
    selection.rig = model;

    let api_key = ctx.session_manager.get_session_api_key(&ctx.session_key);
    let mut auth = NamespaceAuth::new(pub_key, api_key, Some(current_namespace.as_str()));
    auth.resolve(ctx.session_manager).await;

    if !auth.is_authorized() {
        return Ok(Transition::Toast(
            "Not authorized for the current namespace. Connect a wallet or ask an admin."
                .to_string(),
        ));
    }

    if let Err(e) = ctx
        .session_manager
        .get_or_create_session(&ctx.session_key, &mut auth, Some(selection))
        .await
    {
        warn!(
            "Failed to switch model for session {}: {}",
            ctx.session_key, e
        );
        return Ok(Transition::Toast("Failed to update model.".to_string()));
    }

    Ok(Transition::ToastHtml(format!(
        "Model set to {} <code>({})</code>\n\nYou can now start chatting with your request.",
        escape_html(model.rig_label()),
        escape_html(model.rig_slug())
    )))
}

#[async_trait]
impl Panel for ModelPanel {
    fn prefix(&self) -> &'static str {
        "model"
    }

    fn commands(&self) -> &'static [&'static str] {
        &["model"]
    }

    async fn render(&self, ctx: &PanelCtx<'_>) -> Result<PanelView> {
        if !ctx.is_private {
            return Ok(PanelView {
                text: "Model selection is available in direct chat with the bot.".to_string(),
                keyboard: None,
            });
        }

        let current_model = ctx
            .session_manager
            .get_session_config(&ctx.session_key)
            .map(|(_, selection)| selection.rig)
            .unwrap_or(Selection::default().rig);

        let msg = format!(
            "<b>Choose Model</b>\n\n\
             Current: {} <code>({})</code>\n\n\
             <b>How to choose</b>\n\
             - Choose stronger models for harder reasoning/planning.\n\
             - Choose lighter models for faster responses.\n\
             - If unsure, keep the current model and change only when speed or quality needs adjustment.",
            escape_html(current_model.rig_label()),
            escape_html(current_model.rig_slug())
        );

        Ok(PanelView {
            text: msg,
            keyboard: Some(make_model_keyboard(current_model)),
        })
    }

    async fn on_callback(&self, ctx: &PanelCtx<'_>, data: &str) -> Result<Transition> {
        if !ctx.is_private {
            return Ok(Transition::Toast(
                "Model selection is available in direct chat.".to_string(),
            ));
        }

        if let Some(slug) = data.strip_prefix("set:") {
            return set_model(ctx, slug).await;
        }

        Ok(Transition::None)
    }

    async fn on_command(
        &self,
        ctx: &PanelCtx<'_>,
        _command: &str,
        args: &str,
    ) -> Result<Transition> {
        if !ctx.is_private {
            return Ok(Transition::Toast(
                "This command is available in DMs only.".to_string(),
            ));
        }

        let arg = args.split_whitespace().next().unwrap_or("");
        match arg {
            "" | "list" | "show" => Ok(Transition::None), // falls back to render
            slug => set_model(ctx, slug).await,
        }
    }
}
