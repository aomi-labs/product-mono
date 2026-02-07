use anyhow::Result;
use async_trait::async_trait;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use tracing::warn;

use aomi_backend::{Namespace, NamespaceAuth};

use crate::send::escape_html;

use super::{Panel, PanelCtx, PanelView, Transition};

pub struct NamespacePanel;

const NAMESPACE_OPTIONS: [(Namespace, &str); 4] = [
    (Namespace::Default, "Just SendIt"),
    (Namespace::Polymarket, "Prediction Wizzard"),
    (Namespace::L2b, "DeFi Master"),
    (Namespace::X, "Social Jam"),
];

fn make_namespace_keyboard(current_namespace: Namespace) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = NAMESPACE_OPTIONS
        .chunks(2)
        .map(|chunk| {
            chunk
                .iter()
                .map(|(namespace, label)| {
                    let display = if *namespace == current_namespace {
                        format!("  {}", label)
                    } else {
                        (*label).to_string()
                    };
                    InlineKeyboardButton::callback(
                        display,
                        format!("p:ns:set:{}", namespace.as_str()),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();

    rows.push(vec![InlineKeyboardButton::callback("Back", "p:start")]);
    InlineKeyboardMarkup::new(rows)
}

async fn set_namespace(ctx: &PanelCtx<'_>, slug: &str) -> Result<Transition> {
    let Some(namespace) = Namespace::parse(slug) else {
        return Ok(Transition::Toast("Unknown namespace.".to_string()));
    };

    let pub_key = ctx.bound_wallet().await;
    let current_selection = ctx
        .session_manager
        .get_session_config(&ctx.session_key)
        .map(|(_, selection)| selection)
        .unwrap_or_default();

    let api_key = ctx.session_manager.get_session_api_key(&ctx.session_key);
    let mut auth = NamespaceAuth::new(pub_key, api_key, Some(namespace.as_str()));
    auth.resolve(ctx.session_manager).await;

    if !auth.is_authorized() {
        return Ok(Transition::Toast(
            "Not authorized for that namespace. Connect a wallet or ask an admin.".to_string(),
        ));
    }

    if let Err(e) = ctx
        .session_manager
        .get_or_create_session(&ctx.session_key, &mut auth, Some(current_selection))
        .await
    {
        warn!(
            "Failed to switch namespace for session {}: {}",
            ctx.session_key, e
        );
        return Ok(Transition::Toast(
            "Failed to switch namespace.".to_string(),
        ));
    }

    Ok(Transition::ToastHtml(format!(
        "Namespace set to <code>{}</code>\n\nYou can now start chatting with your request.",
        escape_html(namespace.as_str())
    )))
}

#[async_trait]
impl Panel for NamespacePanel {
    fn prefix(&self) -> &'static str {
        "ns"
    }

    fn commands(&self) -> &'static [&'static str] {
        &["namespace"]
    }

    async fn render(&self, ctx: &PanelCtx<'_>) -> Result<PanelView> {
        if !ctx.is_private {
            return Ok(PanelView {
                text: "Namespace selection is available in direct chat with the bot.".to_string(),
                keyboard: None,
            });
        }

        let current_namespace = ctx
            .session_manager
            .get_session_config(&ctx.session_key)
            .map(|(namespace, _)| namespace)
            .unwrap_or(Namespace::Default);

        let msg = format!(
            "<b>Choose Namespace</b>\n\n\
             Current: <code>{}</code>\n\n\
             <b>How to choose</b>\n\
             - Pick the app/domain you want to work in.\n\
             - Use <code>default</code> for general DeFi and broad tasks.\n\
             - Switch to specialized namespaces (for example <code>polymarket</code> or <code>x</code>) when your task is specific to that domain.",
            escape_html(current_namespace.as_str())
        );

        Ok(PanelView {
            text: msg,
            keyboard: Some(make_namespace_keyboard(current_namespace)),
        })
    }

    async fn on_callback(&self, ctx: &PanelCtx<'_>, data: &str) -> Result<Transition> {
        if !ctx.is_private {
            return Ok(Transition::Toast(
                "Namespace selection is available in direct chat.".to_string(),
            ));
        }

        if let Some(slug) = data.strip_prefix("set:") {
            return set_namespace(ctx, slug).await;
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
            slug => set_namespace(ctx, slug).await,
        }
    }
}
