//! Slash command handlers for Telegram bot.

use anyhow::Result;
use std::sync::Arc;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{
    CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, Message, ParseMode, WebAppInfo,
};
use tracing::{info, warn};

use crate::send::escape_html;
use crate::wallet_create::{
    CREATE_WALLET_CALLBACK, create_wallet_button, handle_create_wallet_callback,
};

use aomi_backend::{AomiModel, Namespace, NamespaceAuth, Selection, SessionManager};
use aomi_bot_core::{DbWalletConnectService, WalletConnectService};
use sqlx::{Any, Pool};

use crate::TelegramBot;
use crate::session::dm_session_key;

const MENU_HOME_CALLBACK: &str = "menu_home";
const MENU_NAMESPACE_CALLBACK: &str = "menu_namespace";
const MENU_MODEL_CALLBACK: &str = "menu_model";
const CONNECT_UNAVAILABLE_CALLBACK: &str = "connect_unavailable";
const NAMESPACE_SET_PREFIX: &str = "namespace_set:";
const MODEL_SET_PREFIX: &str = "model_set:";

/// Get Mini App URL - returns None if not HTTPS (Telegram requirement)
fn get_mini_app_url() -> Option<String> {
    let url = std::env::var("MINI_APP_URL").ok()?;
    if url.starts_with("https://") {
        Some(url)
    } else {
        warn!("MINI_APP_URL must be HTTPS for Telegram Web Apps: {}", url);
        None
    }
}

fn start_message(has_wallet: bool) -> &'static str {
    if has_wallet {
        "<b>👋 Welcome to Aomi!</b>\n\n\
         I'm your DeFi assistant.\n\n\
         Use the buttons below to manage wallet, namespace, and model."
    } else {
        "<b>👋 Welcome to Aomi!</b>\n\n\
         I'm your DeFi assistant.\n\n\
         Start by connecting a wallet or creating a new wallet."
    }
}

fn connect_button() -> InlineKeyboardButton {
    if let Some(connect_url) = get_mini_app_url() {
        InlineKeyboardButton::web_app(
            "🔗 Connect Wallet",
            WebAppInfo {
                url: connect_url.parse().unwrap(),
            },
        )
    } else {
        InlineKeyboardButton::callback(
            "🔗 Connect Wallet",
            CONNECT_UNAVAILABLE_CALLBACK.to_string(),
        )
    }
}

/// 2x2 start menu keyboard: Namespace / Model / Connect / Create.
fn make_start_keyboard(has_wallet: bool) -> InlineKeyboardMarkup {
    let mut rows = vec![vec![connect_button(), create_wallet_button()]];

    if has_wallet {
        rows.push(vec![
            InlineKeyboardButton::callback("📦 Namespace", MENU_NAMESPACE_CALLBACK.to_string()),
            InlineKeyboardButton::callback("🤖 Model", MENU_MODEL_CALLBACK.to_string()),
        ]);
    }

    InlineKeyboardMarkup::new(rows)
}

/// Wallet-only keyboard used by /connect and /wallet flows.
fn make_connect_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![connect_button(), create_wallet_button()]])
}

fn make_namespace_keyboard(namespaces: &[String], current_namespace: &str) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = namespaces
        .chunks(2)
        .map(|chunk| {
            chunk
                .iter()
                .map(|namespace| {
                    let label = if namespace.eq_ignore_ascii_case(current_namespace) {
                        format!("✅ {}", namespace)
                    } else {
                        namespace.clone()
                    };
                    InlineKeyboardButton::callback(
                        label,
                        format!("{NAMESPACE_SET_PREFIX}{namespace}"),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();

    rows.push(vec![InlineKeyboardButton::callback(
        "⬅️ Back",
        MENU_HOME_CALLBACK.to_string(),
    )]);

    InlineKeyboardMarkup::new(rows)
}

fn make_model_keyboard(current_model: AomiModel) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = AomiModel::rig_all()
        .chunks(2)
        .map(|chunk| {
            chunk
                .iter()
                .map(|model| {
                    let slug = model.rig_slug();
                    let label = if *model == current_model {
                        format!("✅ {}", model.rig_label())
                    } else {
                        model.rig_label().to_string()
                    };
                    InlineKeyboardButton::callback(label, format!("{MODEL_SET_PREFIX}{slug}"))
                })
                .collect::<Vec<_>>()
        })
        .collect();

    rows.push(vec![InlineKeyboardButton::callback(
        "⬅️ Back",
        MENU_HOME_CALLBACK.to_string(),
    )]);

    InlineKeyboardMarkup::new(rows)
}

/// Create sign transaction keyboard with tx_id parameter
pub fn make_sign_keyboard(tx_id: &str) -> Option<InlineKeyboardMarkup> {
    get_mini_app_url().map(|base_url| {
        // Add /sign path and tx_id as start_param
        let url = format!("{}/sign?tx_id={}", base_url, tx_id);
        InlineKeyboardMarkup::new([[InlineKeyboardButton::web_app(
            "🔐 Sign Transaction",
            WebAppInfo {
                url: url.parse().unwrap(),
            },
        )]])
    })
}

/// Check if a message is a command and return the command name and args.
pub fn parse_command(text: &str) -> Option<(&str, &str)> {
    if !text.starts_with('/') {
        return None;
    }

    let text = text.trim();
    let mut parts = text.splitn(2, |c: char| c.is_whitespace());
    let cmd = parts.next()?.trim_start_matches('/');
    let args = parts.next().unwrap_or("").trim();

    // Remove @botname suffix if present
    let cmd = cmd.split('@').next()?;

    Some((cmd, args))
}

/// Handle wallet-related commands.
/// Returns Ok(true) if command was handled, Ok(false) if not a command.
pub async fn handle_command(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
) -> Result<bool> {
    let text = message.text().unwrap_or("");

    let (cmd, args) = match parse_command(text) {
        Some(c) => c,
        None => return Ok(false),
    };

    match cmd {
        "namespace" => {
            handle_namespace(bot, message, pool, session_manager, args).await?;
            Ok(true)
        }
        "model" => {
            handle_model(bot, message, pool, session_manager, args).await?;
            Ok(true)
        }
        "connect" => {
            handle_connect(bot, message).await?;
            Ok(true)
        }
        "wallet" => {
            handle_wallet(bot, message, pool).await?;
            Ok(true)
        }
        "disconnect" => {
            handle_disconnect(bot, message, pool).await?;
            Ok(true)
        }
        "sign" => {
            // /sign <tx_id> - used by agent to prompt user to sign
            handle_sign(bot, message, args).await?;
            Ok(true)
        }
        "start" => {
            handle_start(bot, message, pool).await?;
            Ok(true)
        }
        "help" => {
            handle_help(bot, message).await?;
            Ok(true)
        }
        _ => Ok(false), // Unknown command, let it fall through to normal handling
    }
}

/// Handle callback button interactions.
/// Returns Ok(true) if callback was handled, Ok(false) if not recognized.
pub async fn handle_callback(
    bot: &TelegramBot,
    query: &CallbackQuery,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
) -> Result<bool> {
    let Some(data) = query.data.as_deref() else {
        return Ok(false);
    };

    let handled = data == CREATE_WALLET_CALLBACK
        || data == MENU_HOME_CALLBACK
        || data == MENU_NAMESPACE_CALLBACK
        || data == MENU_MODEL_CALLBACK
        || data == CONNECT_UNAVAILABLE_CALLBACK
        || data.starts_with(NAMESPACE_SET_PREFIX)
        || data.starts_with(MODEL_SET_PREFIX);

    if !handled {
        return Ok(false);
    }
    bot.bot.answer_callback_query(query.id.clone()).await?;
    let chat_id = callback_chat_id(query);

    if data == MENU_HOME_CALLBACK {
        send_start_menu(bot, chat_id, Some(query.from.id), pool).await?;
        return Ok(true);
    }

    if data == CONNECT_UNAVAILABLE_CALLBACK {
        bot.bot
            .send_message(
                chat_id,
                "⚠️ Connect Wallet mini-app is not configured. Ask admin to set a valid HTTPS `MINI_APP_URL`.",
            )
            .await?;
        return Ok(true);
    }

    if data == MENU_NAMESPACE_CALLBACK {
        if !callback_is_private(query) {
            bot.bot
                .send_message(
                    chat_id,
                    "Namespace selection is available in direct chat with the bot.",
                )
                .await?;
            return Ok(true);
        }
        send_namespace_menu(bot, chat_id, query.from.id, pool, session_manager).await?;
        return Ok(true);
    }

    if data == MENU_MODEL_CALLBACK {
        if !callback_is_private(query) {
            bot.bot
                .send_message(
                    chat_id,
                    "Model selection is available in direct chat with the bot.",
                )
                .await?;
            return Ok(true);
        }
        send_model_menu(bot, chat_id, query.from.id, pool, session_manager).await?;
        return Ok(true);
    }

    if let Some(namespace_slug) = data.strip_prefix(NAMESPACE_SET_PREFIX) {
        if !callback_is_private(query) {
            bot.bot
                .send_message(chat_id, "Namespace selection is available in direct chat.")
                .await?;
            return Ok(true);
        }

        let Some(namespace) = Namespace::parse(namespace_slug) else {
            bot.bot.send_message(chat_id, "Unknown namespace.").await?;
            return Ok(true);
        };

        let session_key = dm_session_key(query.from.id);
        let (pub_key, user_namespaces) =
            load_auth_context(pool, session_manager, &session_key).await;

        let current_selection = session_manager
            .get_session_config(&session_key)
            .map(|(_, selection)| selection)
            .unwrap_or_default();

        let mut auth = NamespaceAuth::new(pub_key.clone(), None, Some(namespace.as_str()));
        auth.merge_authorization(user_namespaces);

        if !auth.is_authorized() {
            bot.bot
                .send_message(
                    chat_id,
                    "Not authorized for that namespace. Connect a wallet or ask an admin.",
                )
                .await?;
            return Ok(true);
        }

        if let Err(e) = session_manager
            .get_or_create_session(&session_key, &mut auth, Some(current_selection))
            .await
        {
            warn!(
                "Failed to switch namespace via callback for session {}: {}",
                session_key, e
            );
            bot.bot
                .send_message(chat_id, "Failed to switch namespace.")
                .await?;
            return Ok(true);
        }

        bot.bot
            .send_message(
                chat_id,
                format!(
                    "✅ Namespace set to <code>{}</code>\n\nYou can now start chatting with your request.",
                    escape_html(namespace.as_str())
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        return Ok(true);
    }

    if let Some(model_slug) = data.strip_prefix(MODEL_SET_PREFIX) {
        if !callback_is_private(query) {
            bot.bot
                .send_message(chat_id, "Model selection is available in direct chat.")
                .await?;
            return Ok(true);
        }

        let Some(model) = AomiModel::parse_rig(model_slug) else {
            bot.bot.send_message(chat_id, "Unknown model.").await?;
            return Ok(true);
        };

        let session_key = dm_session_key(query.from.id);
        let (pub_key, user_namespaces) =
            load_auth_context(pool, session_manager, &session_key).await;
        let (current_namespace, mut selection) = session_manager
            .get_session_config(&session_key)
            .unwrap_or((Namespace::Default, Selection::default()));
        selection.rig = model;

        let mut auth = NamespaceAuth::new(pub_key, None, Some(current_namespace.as_str()));
        auth.merge_authorization(user_namespaces);

        if !auth.is_authorized() {
            bot.bot
                .send_message(
                    chat_id,
                    "Not authorized for the current namespace. Connect a wallet or ask an admin.",
                )
                .await?;
            return Ok(true);
        }

        if let Err(e) = session_manager
            .get_or_create_session(&session_key, &mut auth, Some(selection))
            .await
        {
            warn!(
                "Failed to switch model via callback for session {}: {}",
                session_key, e
            );
            bot.bot
                .send_message(chat_id, "Failed to update model.")
                .await?;
            return Ok(true);
        }

        bot.bot
            .send_message(
                chat_id,
                format!(
                    "✅ Model set to {} <code>({})</code>\n\nYou can now start chatting with your request.",
                    escape_html(model.rig_label()),
                    escape_html(model.rig_slug())
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        return Ok(true);
    }

    // CREATE_WALLET_CALLBACK fallback only happens when MINI_APP_URL is unavailable.
    if !callback_is_private(query) {
        bot.bot
            .send_message(
                chat_id,
                "For security, wallet creation is available only in direct chat with the bot.",
            )
            .await?;
        return Ok(true);
    }

    handle_create_wallet_callback(bot, chat_id).await?;
    Ok(true)
}

fn callback_chat_id(query: &CallbackQuery) -> teloxide::types::ChatId {
    query
        .message
        .as_ref()
        .map(|msg| msg.chat().id)
        .unwrap_or(teloxide::types::ChatId(query.from.id.0 as i64))
}

fn callback_is_private(query: &CallbackQuery) -> bool {
    query
        .message
        .as_ref()
        .is_some_and(|msg| msg.chat().is_private())
}

async fn get_bound_wallet_for_user(
    pool: &Pool<Any>,
    user_id: teloxide::types::UserId,
) -> Option<String> {
    let session_key = dm_session_key(user_id);
    let wallet_service = DbWalletConnectService::new(pool.clone());
    match wallet_service.get_bound_wallet(&session_key).await {
        Ok(wallet) => wallet,
        Err(e) => {
            warn!(
                "Failed to load bound wallet for session {}: {}",
                session_key, e
            );
            None
        }
    }
}

async fn send_start_menu(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    user_id: Option<teloxide::types::UserId>,
    pool: &Pool<Any>,
) -> Result<()> {
    let has_wallet = if let Some(uid) = user_id {
        get_bound_wallet_for_user(pool, uid).await.is_some()
    } else {
        false
    };

    bot.bot
        .send_message(chat_id, start_message(has_wallet))
        .parse_mode(ParseMode::Html)
        .reply_markup(make_start_keyboard(has_wallet))
        .await?;
    Ok(())
}

async fn load_auth_context(
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
    session_key: &str,
) -> (Option<String>, Option<Vec<String>>) {
    let wallet_service = DbWalletConnectService::new(pool.clone());
    let pub_key = match wallet_service.get_bound_wallet(session_key).await {
        Ok(wallet) => wallet,
        Err(e) => {
            warn!(
                "Failed to load bound wallet for session {}: {}",
                session_key, e
            );
            None
        }
    };

    let user_namespaces = if let Some(ref pk) = pub_key {
        match session_manager.get_user_namespaces(pk).await {
            Ok(namespaces) => Some(namespaces),
            Err(e) => {
                warn!("Failed to load namespaces for {}: {}", pk, e);
                None
            }
        }
    } else {
        None
    };

    (pub_key, user_namespaces)
}

async fn send_namespace_menu(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    user_id: teloxide::types::UserId,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let session_key = dm_session_key(user_id);
    let (pub_key, user_namespaces) = load_auth_context(pool, session_manager, &session_key).await;

    let mut auth = NamespaceAuth::new(pub_key, None, None);
    auth.merge_authorization(user_namespaces);
    let mut namespaces = auth.current_authorization;
    namespaces.sort();

    let current_namespace = session_manager
        .get_session_config(&session_key)
        .map(|(namespace, _)| namespace.as_str().to_string())
        .unwrap_or_else(|| Namespace::Default.as_str().to_string());

    let msg = format!(
        "<b>📦 Choose Namespace</b>\n\n\
         Current: <code>{}</code>\n\n\
         <b>How to choose</b>\n\
         • Pick the app/domain you want to work in.\n\
         • Use <code>default</code> for general DeFi and broad tasks.\n\
         • Switch to specialized namespaces (for example <code>polymarket</code> or <code>x</code>) when your task is specific to that domain.",
        escape_html(&current_namespace)
    );

    bot.bot
        .send_message(chat_id, msg)
        .parse_mode(ParseMode::Html)
        .reply_markup(make_namespace_keyboard(&namespaces, &current_namespace))
        .await?;
    Ok(())
}

async fn send_model_menu(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    user_id: teloxide::types::UserId,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let session_key = dm_session_key(user_id);
    let _ = load_auth_context(pool, session_manager, &session_key).await;
    let current_model = session_manager
        .get_session_config(&session_key)
        .map(|(_, selection)| selection.rig)
        .unwrap_or(Selection::default().rig);

    let msg = format!(
        "<b>🤖 Choose Model</b>\n\n\
         Current: {} <code>({})</code>\n\n\
         <b>How to choose</b>\n\
         • Choose stronger models for harder reasoning/planning.\n\
         • Choose lighter models for faster responses.\n\
         • If unsure, keep the current model and change only when speed or quality needs adjustment.",
        escape_html(current_model.rig_label()),
        escape_html(current_model.rig_slug())
    );

    bot.bot
        .send_message(chat_id, msg)
        .parse_mode(ParseMode::Html)
        .reply_markup(make_model_keyboard(current_model))
        .await?;
    Ok(())
}

async fn handle_namespace(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
    args: &str,
) -> Result<()> {
    let chat_id = message.chat.id;

    if !message.chat.is_private() {
        bot.bot
            .send_message(chat_id, "This command is available in DMs only.")
            .await?;
        return Ok(());
    }

    let user_id = match message.from.as_ref().map(|u| u.id) {
        Some(id) => id,
        None => {
            bot.bot
                .send_message(chat_id, "Missing user information.")
                .await?;
            return Ok(());
        }
    };

    let session_key = dm_session_key(user_id);
    let (pub_key, user_namespaces) = load_auth_context(pool, session_manager, &session_key).await;

    let arg = args.split_whitespace().next().unwrap_or("");
    match arg {
        "" | "list" | "show" => {
            send_namespace_menu(bot, chat_id, user_id, pool, session_manager).await?;
        }
        _ => {
            let Some(namespace) = Namespace::parse(arg) else {
                bot.bot
                    .send_message(chat_id, "Unknown namespace. Tap /namespace to choose.")
                    .await?;
                return Ok(());
            };

            let current_selection = session_manager
                .get_session_config(&session_key)
                .map(|(_, selection)| selection)
                .unwrap_or_default();

            let mut auth = NamespaceAuth::new(pub_key.clone(), None, Some(namespace.as_str()));
            auth.merge_authorization(user_namespaces);

            if !auth.is_authorized() {
                bot.bot
                    .send_message(
                        chat_id,
                        "Not authorized for that namespace. Connect a wallet or ask an admin.",
                    )
                    .await?;
                return Ok(());
            }

            if let Err(e) = session_manager
                .get_or_create_session(&session_key, &mut auth, Some(current_selection))
                .await
            {
                warn!(
                    "Failed to switch namespace for session {}: {}",
                    session_key, e
                );
                bot.bot
                    .send_message(chat_id, "Failed to switch namespace.")
                    .await?;
                return Ok(());
            }

            let msg = format!(
                "✅ Namespace set to <code>{}</code>\n\nYou can now start chatting with your request.",
                escape_html(namespace.as_str())
            );
            bot.bot
                .send_message(chat_id, msg)
                .parse_mode(ParseMode::Html)
                .await?;
        }
    }

    Ok(())
}

async fn handle_model(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
    args: &str,
) -> Result<()> {
    let chat_id = message.chat.id;

    if !message.chat.is_private() {
        bot.bot
            .send_message(chat_id, "This command is available in DMs only.")
            .await?;
        return Ok(());
    }

    let user_id = match message.from.as_ref().map(|u| u.id) {
        Some(id) => id,
        None => {
            bot.bot
                .send_message(chat_id, "Missing user information.")
                .await?;
            return Ok(());
        }
    };

    let session_key = dm_session_key(user_id);
    let (pub_key, user_namespaces) = load_auth_context(pool, session_manager, &session_key).await;

    let arg = args.split_whitespace().next().unwrap_or("");
    match arg {
        "" | "list" | "show" => {
            send_model_menu(bot, chat_id, user_id, pool, session_manager).await?;
        }
        _ => {
            let Some(model) = AomiModel::parse_rig(arg) else {
                bot.bot
                    .send_message(chat_id, "Unknown model. Tap /model to choose.")
                    .await?;
                return Ok(());
            };

            let (current_namespace, mut selection) = session_manager
                .get_session_config(&session_key)
                .unwrap_or((Namespace::Default, Selection::default()));

            selection.rig = model;

            let mut auth = NamespaceAuth::new(pub_key, None, Some(current_namespace.as_str()));
            auth.merge_authorization(user_namespaces);

            if !auth.is_authorized() {
                bot.bot
                    .send_message(
                        chat_id,
                        "Not authorized for the current namespace. Connect a wallet or ask an admin.",
                    )
                    .await?;
                return Ok(());
            }

            if let Err(e) = session_manager
                .get_or_create_session(&session_key, &mut auth, Some(selection))
                .await
            {
                warn!("Failed to switch model for session {}: {}", session_key, e);
                bot.bot
                    .send_message(chat_id, "Failed to update model.")
                    .await?;
                return Ok(());
            }

            let msg = format!(
                "✅ Model set to {} <code>({})</code>\n\nYou can now start chatting with your request.",
                escape_html(model.rig_label()),
                escape_html(model.rig_slug())
            );
            bot.bot
                .send_message(chat_id, msg)
                .parse_mode(ParseMode::Html)
                .await?;
        }
    }

    Ok(())
}

/// Handle /start command.
async fn handle_start(bot: &TelegramBot, message: &Message, pool: &Pool<Any>) -> Result<()> {
    let chat_id = message.chat.id;
    let user_id = message.from.as_ref().map(|u| u.id);
    send_start_menu(bot, chat_id, user_id, pool).await
}

/// Handle /connect command - opens Mini App if available.
async fn handle_connect(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;

    let keyboard = make_connect_keyboard();
    let msg = if get_mini_app_url().is_some() {
        "Choose how you want to get started with your wallet:"
    } else {
        "⚠️ Wallet mini-app is not configured. Ask admin to set a valid HTTPS `MINI_APP_URL`."
    };
    bot.bot
        .send_message(chat_id, msg)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

/// Handle /sign command - prompts user to sign a pending transaction.
/// Usage: /sign <tx_id>
async fn handle_sign(bot: &TelegramBot, message: &Message, tx_id: &str) -> Result<()> {
    let chat_id = message.chat.id;

    if tx_id.is_empty() {
        bot.bot
            .send_message(chat_id, "❌ Missing transaction ID")
            .await?;
        return Ok(());
    }

    if let Some(keyboard) = make_sign_keyboard(tx_id) {
        bot.bot.send_message(chat_id,
            "🔐 *Transaction requires your signature*\n\nTap the button below to review and sign\\."
        )
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .reply_markup(keyboard)
        .await?;
    } else {
        bot.bot
            .send_message(
                chat_id,
                "⚠️ Signing is not available\\. Please configure MINI\\_APP\\_URL\\.",
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
    }

    Ok(())
}

/// Handle /wallet command.
async fn handle_wallet(bot: &TelegramBot, message: &Message, pool: &Pool<Any>) -> Result<()> {
    let chat_id = message.chat.id;
    let user_id = message.from.as_ref().map(|u| u.id).unwrap();
    let session_key = dm_session_key(user_id);

    let wallet_service = DbWalletConnectService::new(pool.clone());

    match wallet_service.get_bound_wallet(&session_key).await {
        Ok(Some(address)) => {
            let msg = format!("💳 *Connected wallet:*\n\n`{}`", address);

            bot.bot
                .send_message(chat_id, msg)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .reply_markup(make_connect_keyboard())
                .await?;
        }
        Ok(None) => {
            bot.bot
                .send_message(
                    chat_id,
                    "No wallet connected\\. Tap below to connect or create one:",
                )
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .reply_markup(make_connect_keyboard())
                .await?;
        }
        Err(e) => {
            bot.bot
                .send_message(chat_id, format!("❌ Error: {}", e))
                .await?;
        }
    }

    Ok(())
}

/// Handle /disconnect command.
async fn handle_disconnect(bot: &TelegramBot, message: &Message, pool: &Pool<Any>) -> Result<()> {
    let chat_id = message.chat.id;
    let user_id = message.from.as_ref().map(|u| u.id).unwrap();
    let session_key = dm_session_key(user_id);

    let wallet_service = DbWalletConnectService::new(pool.clone());

    match wallet_service.disconnect(&session_key).await {
        Ok(()) => {
            bot.bot
                .send_message(chat_id, "✅ Wallet disconnected\\.")
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await?;
            info!("User {} disconnected wallet", user_id);
        }
        Err(e) => {
            bot.bot
                .send_message(chat_id, format!("❌ Error: {}", e))
                .await?;
        }
    }

    Ok(())
}

/// Handle /help command.
async fn handle_help(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;

    bot.bot
        .send_message(
            chat_id,
            "🤖 *Aomi Commands*\n\n\
        /start \\- Show main action buttons\n\
        /connect \\- Connect or create a wallet\n\
        /namespace \\- Open namespace picker\n\
        /model \\- Open model picker\n\
        /wallet \\- Show connected wallet\n\
        /disconnect \\- Unlink your wallet\n\
        /help \\- Show this message\n\n\
        Tip: use /start and tap buttons instead of typing selections\\.",
        )
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command() {
        assert_eq!(parse_command("/connect"), Some(("connect", "")));
        assert_eq!(parse_command("/connect 0x123"), Some(("connect", "0x123")));
        assert_eq!(parse_command("/wallet@mybot"), Some(("wallet", "")));
        assert_eq!(parse_command("/sign tx_123"), Some(("sign", "tx_123")));
        assert_eq!(
            parse_command("/namespace polymarket"),
            Some(("namespace", "polymarket"))
        );
        assert_eq!(
            parse_command("/namespace list"),
            Some(("namespace", "list"))
        );
        assert_eq!(parse_command("/model opus-4"), Some(("model", "opus-4")));
        assert_eq!(parse_command("/model list"), Some(("model", "list")));
        assert_eq!(parse_command("hello"), None);
        assert_eq!(parse_command(""), None);
    }
}
