//! Slash command handlers for Telegram bot.

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{
    CallbackQuery, ForceReply, InlineKeyboardButton, InlineKeyboardMarkup, Message, ParseMode,
    ThreadId, True, WebAppInfo,
};
use tracing::{info, warn};

use crate::send::{escape_html, with_thread_id};
use crate::wallet_create::{
    CREATE_WALLET_CALLBACK, create_wallet_button, handle_create_wallet_callback,
};

use aomi_backend::{
    AomiModel, AuthorizedKey, Namespace, NamespaceAuth, Selection, SessionManager, SessionRecord,
    UserState,
};
use aomi_bot_core::{DbWalletConnectService, WalletConnectService};
use sqlx::{Any, Pool};

use crate::TelegramBot;
use crate::session::{dm_session_key, session_key_from_message};

const PANEL_START_CALLBACK: &str = "panel:start";
const PANEL_NAMESPACE_CALLBACK: &str = "panel:namespace";
const PANEL_MODEL_CALLBACK: &str = "panel:model";
const PANEL_SESSIONS_CALLBACK: &str = "panel:sessions";
const PANEL_STATUS_CALLBACK: &str = "panel:status";
const PANEL_WALLET_CALLBACK: &str = "panel:wallet";
const PANEL_APIKEY_CALLBACK: &str = "panel:apikey";
const PANEL_SETTINGS_CALLBACK: &str = "panel:settings";
const CONNECT_UNAVAILABLE_CALLBACK: &str = "connect_unavailable";
const NAMESPACE_SET_PREFIX: &str = "namespace_set:";
const MODEL_SET_PREFIX: &str = "model_set:";
const SESSION_SELECT_PREFIX: &str = "session_select:";
const SETTINGS_ARCHIVE_CALLBACK: &str = "settings_archive";
const SETTINGS_DELETE_WALLET_CALLBACK: &str = "settings_delete_wallet";

#[derive(Clone, Copy, Debug)]
enum PanelId {
    Start,
    Namespace,
    Model,
    Sessions,
    Status,
    Wallet,
    ApiKey,
    Settings,
}

impl PanelId {
    fn callback(self) -> &'static str {
        match self {
            PanelId::Start => PANEL_START_CALLBACK,
            PanelId::Namespace => PANEL_NAMESPACE_CALLBACK,
            PanelId::Model => PANEL_MODEL_CALLBACK,
            PanelId::Sessions => PANEL_SESSIONS_CALLBACK,
            PanelId::Status => PANEL_STATUS_CALLBACK,
            PanelId::Wallet => PANEL_WALLET_CALLBACK,
            PanelId::ApiKey => PANEL_APIKEY_CALLBACK,
            PanelId::Settings => PANEL_SETTINGS_CALLBACK,
        }
    }
}

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

fn start_message() -> &'static str {
    "<b>👋 Welcome to Aomi!</b>\n\n\
     I'm your DeFi assistant.\n\n\
     Use the panels below to manage sessions, models, and wallet settings."
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
fn make_start_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("📦 Namespace", PanelId::Namespace.callback().to_string()),
            InlineKeyboardButton::callback("🤖 Models", PanelId::Model.callback().to_string()),
        ],
        vec![
            InlineKeyboardButton::callback("🧵 Sessions", PanelId::Sessions.callback().to_string()),
            InlineKeyboardButton::callback("📊 Status", PanelId::Status.callback().to_string()),
        ],
        vec![
            InlineKeyboardButton::callback("👛 Wallet", PanelId::Wallet.callback().to_string()),
            InlineKeyboardButton::callback("🔑 API Key", PanelId::ApiKey.callback().to_string()),
        ],
        vec![InlineKeyboardButton::callback(
            "⚙️ Settings",
            PanelId::Settings.callback().to_string(),
        )],
    ])
}

/// Wallet-only keyboard used by /connect and /wallet flows.
fn make_connect_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![connect_button(), create_wallet_button()],
        vec![InlineKeyboardButton::callback(
            "⬅️ Back",
            PanelId::Start.callback().to_string(),
        )],
    ])
}

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
                        format!("✅ {}", label)
                    } else {
                        (*label).to_string()
                    };
                    InlineKeyboardButton::callback(
                        display,
                        format!("{NAMESPACE_SET_PREFIX}{}", namespace.as_str()),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();

    rows.push(vec![InlineKeyboardButton::callback(
        "⬅️ Back",
        PanelId::Start.callback().to_string(),
    )]);

    InlineKeyboardMarkup::new(rows)
}

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
                        format!("✅ {}", label)
                    } else {
                        (*label).to_string()
                    };
                    InlineKeyboardButton::callback(display, format!("{MODEL_SET_PREFIX}{slug}"))
                })
                .collect::<Vec<_>>()
        })
        .collect();

    rows.push(vec![InlineKeyboardButton::callback(
        "⬅️ Back",
        PanelId::Start.callback().to_string(),
    )]);

    InlineKeyboardMarkup::new(rows)
}

fn make_settings_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                "🗄️ Archive Session",
                SETTINGS_ARCHIVE_CALLBACK.to_string(),
            ),
            InlineKeyboardButton::callback(
                "🧹 Delete Wallet",
                SETTINGS_DELETE_WALLET_CALLBACK.to_string(),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            "⬅️ Back",
            PanelId::Start.callback().to_string(),
        )],
    ])
}

fn make_sessions_keyboard(sessions: &[SessionRecord]) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    let mut current_row: Vec<InlineKeyboardButton> = Vec::new();

    for (idx, session) in sessions.iter().enumerate() {
        current_row.push(InlineKeyboardButton::callback(
            format!("Session {}", idx + 1),
            format!("{SESSION_SELECT_PREFIX}{}", session.session_id),
        ));
        if current_row.len() == 2 {
            rows.push(std::mem::take(&mut current_row));
        }
    }

    if !current_row.is_empty() {
        rows.push(current_row);
    }

    rows.push(vec![InlineKeyboardButton::callback(
        "⬅️ Back",
        PanelId::Start.callback().to_string(),
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
        "apikey" => {
            handle_api_key(bot, message, session_manager, args).await?;
            Ok(true)
        }
        "sessions" => {
            handle_sessions(bot, message, pool, session_manager).await?;
            Ok(true)
        }
        "settings" => {
            handle_settings(bot, message).await?;
            Ok(true)
        }
        "status" => {
            handle_status(bot, message, session_manager).await?;
            Ok(true)
        }
        "sign" => {
            // /sign <tx_id> - used by agent to prompt user to sign
            handle_sign(bot, message, args).await?;
            Ok(true)
        }
        "start" => {
            handle_start(bot, message).await?;
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

    let is_known = data == CREATE_WALLET_CALLBACK
        || data == CONNECT_UNAVAILABLE_CALLBACK
        || data == SETTINGS_ARCHIVE_CALLBACK
        || data == SETTINGS_DELETE_WALLET_CALLBACK
        || data == PANEL_START_CALLBACK
        || data == PANEL_NAMESPACE_CALLBACK
        || data == PANEL_MODEL_CALLBACK
        || data == PANEL_SESSIONS_CALLBACK
        || data == PANEL_STATUS_CALLBACK
        || data == PANEL_WALLET_CALLBACK
        || data == PANEL_APIKEY_CALLBACK
        || data == PANEL_SETTINGS_CALLBACK
        || data.starts_with(NAMESPACE_SET_PREFIX)
        || data.starts_with(MODEL_SET_PREFIX)
        || data.starts_with(SESSION_SELECT_PREFIX);

    if !is_known {
        return Ok(false);
    }

    bot.bot.answer_callback_query(query.id.clone()).await?;
    let chat_id = callback_chat_id(query);
    let thread_id = callback_thread_id(query);

    if data == PANEL_START_CALLBACK {
        send_start_menu(bot, chat_id, thread_id).await?;
        return Ok(true);
    }

    if data == CONNECT_UNAVAILABLE_CALLBACK {
        with_thread_id(
            bot.bot.send_message(
                chat_id,
                "⚠️ Connect Wallet mini-app is not configured. Ask admin to set a valid HTTPS `MINI_APP_URL`.",
            ),
            thread_id,
        )
        .await?;
        return Ok(true);
    }

    if data == PANEL_NAMESPACE_CALLBACK {
        if !callback_is_private(query) {
            with_thread_id(
                bot.bot.send_message(
                    chat_id,
                    "Namespace selection is available in direct chat with the bot.",
                ),
                thread_id,
            )
            .await?;
            return Ok(true);
        }
        send_namespace_menu(bot, chat_id, thread_id, query.from.id, pool, session_manager).await?;
        return Ok(true);
    }

    if data == PANEL_MODEL_CALLBACK {
        if !callback_is_private(query) {
            with_thread_id(
                bot.bot
                    .send_message(chat_id, "Model selection is available in direct chat with the bot."),
                thread_id,
            )
            .await?;
            return Ok(true);
        }
        send_model_menu(bot, chat_id, thread_id, query.from.id, session_manager).await?;
        return Ok(true);
    }

    if data == PANEL_WALLET_CALLBACK {
        if let Some(message) = callback_message(query) {
            handle_connect(bot, message).await?;
        } else {
            with_thread_id(
                bot.bot.send_message(chat_id, "Wallet panel is unavailable here."),
                thread_id,
            )
            .await?;
        }
        return Ok(true);
    }

    if data == PANEL_APIKEY_CALLBACK {
        handle_api_key_prompt(bot, chat_id, thread_id, session_manager, query).await?;
        return Ok(true);
    }

    if data == PANEL_SETTINGS_CALLBACK {
        send_settings_menu(bot, chat_id, thread_id).await?;
        return Ok(true);
    }

    if data == PANEL_SESSIONS_CALLBACK {
        handle_sessions_callback(bot, chat_id, thread_id, pool, session_manager, query).await?;
        return Ok(true);
    }

    if data == PANEL_STATUS_CALLBACK {
        handle_status_callback(bot, chat_id, thread_id, session_manager, query).await?;
        return Ok(true);
    }

    if data == SETTINGS_ARCHIVE_CALLBACK {
        let session_key = callback_session_key(query);
        session_manager.set_session_archived(&session_key, true);
        with_thread_id(
            bot.bot.send_message(chat_id, "✅ Session archived."),
            thread_id,
        )
        .await?;
        return Ok(true);
    }

    if data == SETTINGS_DELETE_WALLET_CALLBACK {
        let session_key = callback_session_key(query);
        let wallet_service = DbWalletConnectService::new(pool.clone());
        match wallet_service.disconnect(&session_key).await {
            Ok(()) => {
                with_thread_id(bot.bot.send_message(chat_id, "✅ Wallet deleted."), thread_id)
                    .await?;
            }
            Err(e) => {
                with_thread_id(
                    bot.bot.send_message(chat_id, format!("❌ Error: {}", e)),
                    thread_id,
                )
                .await?;
            }
        }
        return Ok(true);
    }

    if let Some(namespace_slug) = data.strip_prefix(NAMESPACE_SET_PREFIX) {
        if !callback_is_private(query) {
            with_thread_id(
                bot.bot.send_message(chat_id, "Namespace selection is available in direct chat."),
                thread_id,
            )
            .await?;
            return Ok(true);
        }

        let Some(namespace) = Namespace::parse(namespace_slug) else {
            with_thread_id(bot.bot.send_message(chat_id, "Unknown namespace."), thread_id).await?;
            return Ok(true);
        };

        let session_key = callback_session_key(query);
        let pub_key = get_bound_wallet_for_session(pool, &session_key).await;
        let current_selection = session_manager
            .get_session_config(&session_key)
            .map(|(_, selection)| selection)
            .unwrap_or_default();

        let api_key = session_manager.get_session_api_key(&session_key);
        let mut auth = NamespaceAuth::new(pub_key.clone(), api_key, Some(namespace.as_str()));
        auth.resolve(session_manager).await;

        if !auth.is_authorized() {
            with_thread_id(
                bot.bot.send_message(
                    chat_id,
                    "Not authorized for that namespace. Connect a wallet or ask an admin.",
                ),
                thread_id,
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
            with_thread_id(
                bot.bot.send_message(chat_id, "Failed to switch namespace."),
                thread_id,
            )
            .await?;
            return Ok(true);
        }

        with_thread_id(
            bot.bot.send_message(
                chat_id,
                format!(
                    "✅ Namespace set to <code>{}</code>\n\nYou can now start chatting with your request.",
                    escape_html(namespace.as_str())
                ),
            ),
            thread_id,
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(true);
    }

    if let Some(model_slug) = data.strip_prefix(MODEL_SET_PREFIX) {
        if !callback_is_private(query) {
            with_thread_id(
                bot.bot.send_message(chat_id, "Model selection is available in direct chat."),
                thread_id,
            )
            .await?;
            return Ok(true);
        }

        let Some(model) = AomiModel::parse_rig(model_slug) else {
            with_thread_id(bot.bot.send_message(chat_id, "Unknown model."), thread_id).await?;
            return Ok(true);
        };

        let session_key = callback_session_key(query);
        let pub_key = get_bound_wallet_for_session(pool, &session_key).await;
        let (current_namespace, mut selection) = session_manager
            .get_session_config(&session_key)
            .map(|(namespace, selection)| (namespace, selection))
            .unwrap_or((Namespace::Default, Selection::default()));
        selection.rig = model;

        let api_key = session_manager.get_session_api_key(&session_key);
        let mut auth = NamespaceAuth::new(pub_key, api_key, Some(current_namespace.as_str()));
        auth.resolve(session_manager).await;

        if !auth.is_authorized() {
            with_thread_id(
                bot.bot.send_message(
                    chat_id,
                    "Not authorized for the current namespace. Connect a wallet or ask an admin.",
                ),
                thread_id,
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
            with_thread_id(bot.bot.send_message(chat_id, "Failed to update model."), thread_id)
                .await?;
            return Ok(true);
        }

        with_thread_id(
            bot.bot.send_message(
                chat_id,
                format!(
                    "✅ Model set to {} <code>({})</code>\n\nYou can now start chatting with your request.",
                    escape_html(model.rig_label()),
                    escape_html(model.rig_slug())
                ),
            ),
            thread_id,
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(true);
    }

    if let Some(session_id) = data.strip_prefix(SESSION_SELECT_PREFIX) {
        with_thread_id(
            bot.bot.send_message(
                chat_id,
                format!("Selected session <code>{}</code>.", escape_html(session_id)),
            ),
            thread_id,
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(true);
    }

    // CREATE_WALLET_CALLBACK fallback only happens when MINI_APP_URL is unavailable.
    if !callback_is_private(query) {
        with_thread_id(
            bot.bot.send_message(
                chat_id,
                "For security, wallet creation is available only in direct chat with the bot.",
            ),
            thread_id,
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

fn callback_thread_id(query: &CallbackQuery) -> Option<ThreadId> {
    callback_message(query).and_then(|msg| msg.thread_id)
}

fn callback_session_key(query: &CallbackQuery) -> String {
    callback_message(query)
        .and_then(session_key_from_message)
        .unwrap_or_else(|| dm_session_key(query.from.id))
}

fn callback_message(query: &CallbackQuery) -> Option<&Message> {
    query.message.as_ref().and_then(|msg| msg.regular_message())
}

fn callback_is_private(query: &CallbackQuery) -> bool {
    query
        .message
        .as_ref()
        .is_some_and(|msg| msg.chat().is_private())
}

async fn send_start_menu(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    thread_id: Option<ThreadId>,
) -> Result<()> {
    with_thread_id(bot.bot.send_message(chat_id, start_message()), thread_id)
        .parse_mode(ParseMode::Html)
        .reply_markup(make_start_keyboard())
        .await?;
    Ok(())
}

async fn get_bound_wallet_for_session(pool: &Pool<Any>, session_key: &str) -> Option<String> {
    let wallet_service = DbWalletConnectService::new(pool.clone());
    match wallet_service.get_bound_wallet(session_key).await {
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

async fn send_namespace_menu(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    thread_id: Option<ThreadId>,
    user_id: teloxide::types::UserId,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let session_key = dm_session_key(user_id);
    let pub_key = get_bound_wallet_for_session(pool, &session_key).await;
    let api_key = session_manager.get_session_api_key(&session_key);

    let mut auth = NamespaceAuth::new(pub_key, api_key, None);
    auth.resolve(session_manager).await;
    let current_namespace = session_manager
        .get_session_config(&session_key)
        .map(|(namespace, _)| namespace)
        .unwrap_or(Namespace::Default);

    let msg = format!(
        "<b>📦 Choose Namespace</b>\n\n\
         Current: <code>{}</code>\n\n\
         <b>How to choose</b>\n\
         • Pick the app/domain you want to work in.\n\
         • Use <code>default</code> for general DeFi and broad tasks.\n\
         • Switch to specialized namespaces (for example <code>polymarket</code> or <code>x</code>) when your task is specific to that domain.",
        escape_html(current_namespace.as_str())
    );

    with_thread_id(bot.bot.send_message(chat_id, msg), thread_id)
        .parse_mode(ParseMode::Html)
        .reply_markup(make_namespace_keyboard(current_namespace))
        .await?;
    Ok(())
}

async fn send_model_menu(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    thread_id: Option<ThreadId>,
    user_id: teloxide::types::UserId,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let session_key = dm_session_key(user_id);
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

    with_thread_id(bot.bot.send_message(chat_id, msg), thread_id)
        .parse_mode(ParseMode::Html)
        .reply_markup(make_model_keyboard(current_model))
        .await?;
    Ok(())
}

async fn send_settings_menu(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    thread_id: Option<ThreadId>,
) -> Result<()> {
    let msg = "<b>⚙️ Settings</b>\n\nChoose an action:";
    with_thread_id(bot.bot.send_message(chat_id, msg), thread_id)
        .parse_mode(ParseMode::Html)
        .reply_markup(make_settings_keyboard())
        .await?;
    Ok(())
}

async fn send_sessions_menu(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    thread_id: Option<ThreadId>,
    session_key: &str,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let Some(pub_key) = get_bound_wallet_for_session(pool, session_key).await else {
        with_thread_id(
            bot.bot.send_message(
                chat_id,
                "No wallet connected. Tap below to connect or create one:",
            ),
            thread_id,
        )
        .reply_markup(make_connect_keyboard())
        .await?;
        return Ok(());
    };

    let sessions = session_manager
        .list_sessions(&pub_key, 20)
        .await
        .unwrap_or_default();

    if sessions.is_empty() {
        with_thread_id(
            bot.bot.send_message(chat_id, "No sessions found for this wallet."),
            thread_id,
        )
        .await?;
        return Ok(());
    }

    let mut summary = String::from("<b>🧵 Sessions</b>\n\nSelect a session:\n");
    for (idx, session) in sessions.iter().enumerate() {
        summary.push_str(&format!(
            "\n{}. {}",
            idx + 1,
            escape_html(&session.title)
        ));
    }

    with_thread_id(bot.bot.send_message(chat_id, summary), thread_id)
        .parse_mode(ParseMode::Html)
        .reply_markup(make_sessions_keyboard(&sessions))
        .await?;
    Ok(())
}

async fn send_status_menu(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    thread_id: Option<ThreadId>,
    session_key: &str,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    if backend_base_url(bot).is_none() {
        with_thread_id(
            bot.bot.send_message(
                chat_id,
                "Backend URL is not configured. Set backend_url in bot.toml or AOMI_BACKEND_URL.",
            ),
            thread_id,
        )
        .await?;
        return Ok(());
    }

    let user_state = fetch_backend_user_state(bot, session_key).await?;

    let (chain_name, chain_id, rpc_endpoint) = resolve_chain_info(user_state.as_ref()).await;
    let selection = session_manager
        .get_session_config(session_key)
        .map(|(_, selection)| selection)
        .unwrap_or_default();
    let title = session_manager
        .get_session_title(session_key)
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
        "<b>📊 Status</b>\n\n\
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

    with_thread_id(bot.bot.send_message(chat_id, msg), thread_id)
        .parse_mode(ParseMode::Html)
        .reply_markup(InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
            "⬅️ Back",
            PanelId::Start.callback().to_string(),
        )]]))
        .await?;
    Ok(())
}

fn backend_base_url(bot: &TelegramBot) -> Option<String> {
    bot.config
        .backend_url
        .clone()
        .or_else(|| std::env::var("AOMI_BACKEND_URL").ok())
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

pub(crate) fn api_key_prompt_text() -> &'static str {
    "Send us your Aomi API key for exclusive namespace. Reply to this message with your key, or use /apikey <key>."
}

async fn handle_api_key(
    bot: &TelegramBot,
    message: &Message,
    session_manager: &Arc<SessionManager>,
    args: &str,
) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;

    if !message.chat.is_private() {
        with_thread_id(
            bot.bot
                .send_message(chat_id, "API key setup is available in DMs only."),
            thread_id,
        )
        .await?;
        return Ok(());
    }

    let session_key = match session_key_from_message(message) {
        Some(key) => key,
        None => {
            with_thread_id(
                bot.bot.send_message(chat_id, "Missing session context."),
                thread_id,
            )
            .await?;
            return Ok(());
        }
    };

    let api_key = args.trim();
    if !api_key.is_empty() {
        match AuthorizedKey::new(Arc::new(bot.pool.clone()), api_key).await? {
            Some(key) => {
                session_manager.set_session_api_key(&session_key, Some(key));
                with_thread_id(
                    bot.bot.send_message(chat_id, "✅ API key saved. You can switch namespaces now."),
                    thread_id,
                )
                .await?;
            }
            None => {
                with_thread_id(
                    bot.bot.send_message(chat_id, "❌ Invalid API key. Try again."),
                    thread_id,
                )
                .await?;
            }
        }
        return Ok(());
    }

    let reply = ForceReply {
        force_reply: True,
        input_field_placeholder: None,
        selective: false,
    };
    with_thread_id(bot.bot.send_message(chat_id, api_key_prompt_text()), thread_id)
        .reply_markup(reply)
        .await?;
    Ok(())
}

async fn handle_api_key_prompt(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    thread_id: Option<ThreadId>,
    session_manager: &Arc<SessionManager>,
    query: &CallbackQuery,
) -> Result<()> {
    let _ = session_manager;
    if !callback_is_private(query) {
        with_thread_id(
            bot.bot
                .send_message(chat_id, "API key setup is available in direct chat."),
            thread_id,
        )
        .await?;
        return Ok(());
    }

    let session_key = callback_session_key(query);
    let _ = session_key;
    let reply = ForceReply {
        force_reply: True,
        input_field_placeholder: None,
        selective: false,
    };
    with_thread_id(bot.bot.send_message(chat_id, api_key_prompt_text()), thread_id)
        .reply_markup(reply)
        .await?;
    Ok(())
}

async fn handle_sessions(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;
    let session_key = match session_key_from_message(message) {
        Some(key) => key,
        None => {
            with_thread_id(
                bot.bot.send_message(chat_id, "Missing session context."),
                thread_id,
            )
            .await?;
            return Ok(());
        }
    };

    send_sessions_menu(
        bot,
        chat_id,
        thread_id,
        &session_key,
        pool,
        session_manager,
    )
    .await
}

async fn handle_sessions_callback(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    thread_id: Option<ThreadId>,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
    query: &CallbackQuery,
) -> Result<()> {
    let session_key = callback_session_key(query);
    send_sessions_menu(
        bot,
        chat_id,
        thread_id,
        &session_key,
        pool,
        session_manager,
    )
    .await
}

async fn handle_settings(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;

    if !message.chat.is_private() {
        with_thread_id(
            bot.bot
                .send_message(chat_id, "Settings are available in DMs only."),
            thread_id,
        )
        .await?;
        return Ok(());
    }

    send_settings_menu(bot, chat_id, thread_id).await
}

async fn handle_status(
    bot: &TelegramBot,
    message: &Message,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;
    let session_key = match session_key_from_message(message) {
        Some(key) => key,
        None => {
            with_thread_id(
                bot.bot.send_message(chat_id, "Missing session context."),
                thread_id,
            )
            .await?;
            return Ok(());
        }
    };

    send_status_menu(bot, chat_id, thread_id, &session_key, session_manager).await
}

async fn handle_status_callback(
    bot: &TelegramBot,
    chat_id: teloxide::types::ChatId,
    thread_id: Option<ThreadId>,
    session_manager: &Arc<SessionManager>,
    query: &CallbackQuery,
) -> Result<()> {
    let session_key = callback_session_key(query);
    send_status_menu(bot, chat_id, thread_id, &session_key, session_manager).await
}

async fn handle_namespace(
    bot: &TelegramBot,
    message: &Message,
    pool: &Pool<Any>,
    session_manager: &Arc<SessionManager>,
    args: &str,
) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;

    if !message.chat.is_private() {
        with_thread_id(
            bot.bot
                .send_message(chat_id, "This command is available in DMs only."),
            thread_id,
        )
        .await?;
        return Ok(());
    }

    let user_id = match message.from.as_ref().map(|u| u.id) {
        Some(id) => id,
        None => {
            with_thread_id(bot.bot.send_message(chat_id, "Missing user information."), thread_id)
                .await?;
            return Ok(());
        }
    };

    let session_key = session_key_from_message(message).unwrap_or_else(|| dm_session_key(user_id));

    let arg = args.split_whitespace().next().unwrap_or("");
    match arg {
        "" | "list" | "show" => {
            send_namespace_menu(bot, chat_id, thread_id, user_id, pool, session_manager).await?;
        }
        _ => {
            let Some(namespace) = Namespace::parse(arg) else {
                with_thread_id(
                    bot.bot
                        .send_message(chat_id, "Unknown namespace. Tap /namespace to choose."),
                    thread_id,
                )
                .await?;
                return Ok(());
            };

            let pub_key = get_bound_wallet_for_session(pool, &session_key).await;
            let current_selection = session_manager
                .get_session_config(&session_key)
                .map(|(_, selection)| selection)
                .unwrap_or_default();

            let api_key = session_manager.get_session_api_key(&session_key);
            let mut auth =
                NamespaceAuth::new(pub_key.clone(), api_key, Some(namespace.as_str()));
            auth.resolve(session_manager).await;

            if !auth.is_authorized() {
                with_thread_id(
                    bot.bot.send_message(
                        chat_id,
                        "Not authorized for that namespace. Connect a wallet or ask an admin.",
                    ),
                    thread_id,
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
                with_thread_id(
                    bot.bot.send_message(chat_id, "Failed to switch namespace."),
                    thread_id,
                )
                .await?;
                return Ok(());
            }

            let msg = format!(
                "✅ Namespace set to <code>{}</code>\n\nYou can now start chatting with your request.",
                escape_html(namespace.as_str())
            );
            with_thread_id(bot.bot.send_message(chat_id, msg), thread_id)
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
    let thread_id = message.thread_id;

    if !message.chat.is_private() {
        with_thread_id(
            bot.bot
                .send_message(chat_id, "This command is available in DMs only."),
            thread_id,
        )
        .await?;
        return Ok(());
    }

    let user_id = match message.from.as_ref().map(|u| u.id) {
        Some(id) => id,
        None => {
            with_thread_id(bot.bot.send_message(chat_id, "Missing user information."), thread_id)
                .await?;
            return Ok(());
        }
    };

    let session_key = session_key_from_message(message).unwrap_or_else(|| dm_session_key(user_id));

    let arg = args.split_whitespace().next().unwrap_or("");
    match arg {
        "" | "list" | "show" => {
            send_model_menu(bot, chat_id, thread_id, user_id, session_manager).await?;
        }
        _ => {
            let Some(model) = AomiModel::parse_rig(arg) else {
                with_thread_id(
                    bot.bot
                        .send_message(chat_id, "Unknown model. Tap /model to choose."),
                    thread_id,
                )
                .await?;
                return Ok(());
            };

            let pub_key = get_bound_wallet_for_session(pool, &session_key).await;
            let (current_namespace, mut selection) = session_manager
                .get_session_config(&session_key)
                .map(|(namespace, selection)| (namespace, selection))
                .unwrap_or((Namespace::Default, Selection::default()));

            selection.rig = model;

            let api_key = session_manager.get_session_api_key(&session_key);
            let mut auth = NamespaceAuth::new(pub_key, api_key, Some(current_namespace.as_str()));
            auth.resolve(session_manager).await;

            if !auth.is_authorized() {
                with_thread_id(
                    bot.bot.send_message(
                        chat_id,
                        "Not authorized for the current namespace. Connect a wallet or ask an admin.",
                    ),
                    thread_id,
                )
                .await?;
                return Ok(());
            }

            if let Err(e) = session_manager
                .get_or_create_session(&session_key, &mut auth, Some(selection))
                .await
            {
                warn!("Failed to switch model for session {}: {}", session_key, e);
                with_thread_id(
                    bot.bot.send_message(chat_id, "Failed to update model."),
                    thread_id,
                )
                .await?;
                return Ok(());
            }

            let msg = format!(
                "✅ Model set to {} <code>({})</code>\n\nYou can now start chatting with your request.",
                escape_html(model.rig_label()),
                escape_html(model.rig_slug())
            );
            with_thread_id(bot.bot.send_message(chat_id, msg), thread_id)
                .parse_mode(ParseMode::Html)
                .await?;
        }
    }

    Ok(())
}

/// Handle /start command.
async fn handle_start(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;
    send_start_menu(bot, chat_id, thread_id).await
}

/// Handle /connect command - opens Mini App if available.
async fn handle_connect(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;

    let keyboard = make_connect_keyboard();
    let msg = if get_mini_app_url().is_some() {
        "Choose how you want to get started with your wallet:"
    } else {
        "⚠️ Wallet mini-app is not configured. Ask admin to set a valid HTTPS `MINI_APP_URL`."
    };
    with_thread_id(bot.bot.send_message(chat_id, msg), thread_id)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

/// Handle /sign command - prompts user to sign a pending transaction.
/// Usage: /sign <tx_id>
async fn handle_sign(bot: &TelegramBot, message: &Message, tx_id: &str) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;

    if tx_id.is_empty() {
        with_thread_id(
            bot.bot.send_message(chat_id, "❌ Missing transaction ID"),
            thread_id,
        )
        .await?;
        return Ok(());
    }

    if let Some(keyboard) = make_sign_keyboard(tx_id) {
        with_thread_id(
            bot.bot.send_message(
                chat_id,
                "🔐 *Transaction requires your signature*\n\nTap the button below to review and sign\\.",
            ),
            thread_id,
        )
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .reply_markup(keyboard)
        .await?;
    } else {
        with_thread_id(
            bot.bot.send_message(
                chat_id,
                "⚠️ Signing is not available\\. Please configure MINI\\_APP\\_URL\\.",
            ),
            thread_id,
        )
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;
    }

    Ok(())
}

/// Handle /wallet command.
async fn handle_wallet(bot: &TelegramBot, message: &Message, pool: &Pool<Any>) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;
    let user_id = message.from.as_ref().map(|u| u.id).unwrap();
    let session_key = session_key_from_message(message).unwrap_or_else(|| dm_session_key(user_id));

    let wallet_service = DbWalletConnectService::new(pool.clone());

    match wallet_service.get_bound_wallet(&session_key).await {
        Ok(Some(address)) => {
            let msg = format!("💳 *Connected wallet:*\n\n`{}`", address);

            with_thread_id(bot.bot.send_message(chat_id, msg), thread_id)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .reply_markup(make_connect_keyboard())
                .await?;
        }
        Ok(None) => {
            with_thread_id(
                bot.bot.send_message(
                    chat_id,
                    "No wallet connected\\. Tap below to connect or create one:",
                ),
                thread_id,
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .reply_markup(make_connect_keyboard())
            .await?;
        }
        Err(e) => {
            with_thread_id(
                bot.bot.send_message(chat_id, format!("❌ Error: {}", e)),
                thread_id,
            )
            .await?;
        }
    }

    Ok(())
}

/// Handle /disconnect command.
async fn handle_disconnect(bot: &TelegramBot, message: &Message, pool: &Pool<Any>) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;
    let user_id = message.from.as_ref().map(|u| u.id).unwrap();
    let session_key = session_key_from_message(message).unwrap_or_else(|| dm_session_key(user_id));

    let wallet_service = DbWalletConnectService::new(pool.clone());

    match wallet_service.disconnect(&session_key).await {
        Ok(()) => {
            with_thread_id(
                bot.bot.send_message(chat_id, "✅ Wallet disconnected\\."),
                thread_id,
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
            info!("User {} disconnected wallet", user_id);
        }
        Err(e) => {
            with_thread_id(
                bot.bot.send_message(chat_id, format!("❌ Error: {}", e)),
                thread_id,
            )
            .await?;
        }
    }

    Ok(())
}

/// Handle /help command.
async fn handle_help(bot: &TelegramBot, message: &Message) -> Result<()> {
    let chat_id = message.chat.id;
    let thread_id = message.thread_id;

    with_thread_id(
        bot.bot.send_message(
            chat_id,
            "🤖 *Aomi Commands*\n\n\
        /start \\- Show main action buttons\n\
        /connect \\- Connect or create a wallet\n\
        /namespace \\- Open namespace picker\n\
        /model \\- Open model picker\n\
        /sessions \\- List your sessions\n\
        /status \\- Show wallet + session status\n\
        /apikey \\- Set API key for namespaces\n\
        /settings \\- Settings panel\n\
        /wallet \\- Show connected wallet\n\
        /disconnect \\- Unlink your wallet\n\
        /help \\- Show this message\n\n\
        Tip: use /start and tap buttons instead of typing selections\\.",
        ),
        thread_id,
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
