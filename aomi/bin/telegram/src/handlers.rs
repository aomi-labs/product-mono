//! Message handlers for routing Telegram updates.

use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{ChatAction, Message, MessageEntityKind, ParseMode};
use tracing::{debug, info, warn};

use aomi_backend::{NamespaceAuth, SessionManager, types::UserState};
use aomi_bot_core::handler::extract_assistant_text;
use aomi_bot_core::{DbWalletConnectService, WalletConnectService};
use aomi_core::SystemEvent;

use crate::{
    TelegramBot,
    commands::make_sign_keyboard,
    config::{DmPolicy, GroupPolicy},
    send::format_for_telegram,
    session::{dm_session_key, group_session_key, user_id_from_message},
};

/// Main message handler that routes based on chat type.
pub async fn handle_message(
    bot: &TelegramBot,
    message: &Message,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let chat = &message.chat;

    if chat.is_private() {
        handle_dm(bot, message, session_manager).await
    } else if chat.is_group() || chat.is_supergroup() {
        handle_group(bot, message, session_manager).await
    } else if chat.is_channel() {
        debug!("Ignoring channel message");
        Ok(())
    } else {
        debug!("Unknown chat type, ignoring");
        Ok(())
    }
}

/// Handle direct message (DM) from a user.
async fn handle_dm(
    bot: &TelegramBot,
    message: &Message,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let user_id = match user_id_from_message(message) {
        Some(uid) => uid,
        None => {
            warn!("DM message has no sender, ignoring");
            return Ok(());
        }
    };

    // Check DM policy
    match bot.config.dm_policy {
        DmPolicy::Disabled => {
            debug!("DM policy is disabled, ignoring message from {}", user_id);
            return Ok(());
        }
        DmPolicy::Allowlist => {
            let user_id_i64 = user_id.0 as i64;
            if !bot.config.is_allowlisted(user_id_i64) {
                debug!("User {} not in allowlist, ignoring DM", user_id);
                return Ok(());
            }
        }
        DmPolicy::Open => {}
    }

    let session_key = dm_session_key(user_id);
    let text = message.text().unwrap_or("");

    info!(
        "Processing DM from user {} (session: {}): {}",
        user_id, session_key, text
    );

    process_and_respond(bot, message, session_manager, &session_key, text).await
}

/// Handle group message (group or supergroup).
async fn handle_group(
    bot: &TelegramBot,
    message: &Message,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let user_id = match user_id_from_message(message) {
        Some(uid) => uid,
        None => {
            debug!("Group message has no sender, ignoring");
            return Ok(());
        }
    };

    // Check group policy
    let should_process = match bot.config.group_policy {
        GroupPolicy::Disabled => {
            debug!("Group policy is disabled, ignoring message");
            return Ok(());
        }
        GroupPolicy::Always => true,
        GroupPolicy::Mention => is_bot_mentioned(&bot.bot, message).await?,
    };

    if !should_process {
        debug!("Bot not mentioned in group message, ignoring");
        return Ok(());
    }

    let session_key = group_session_key(message.chat.id);
    let text = message.text().unwrap_or("");

    info!(
        "Processing group message from user {} in chat {} (session: {}): {}",
        user_id, message.chat.id, session_key, text
    );

    process_and_respond(bot, message, session_manager, &session_key, text).await
}

/// Create a pending transaction via the Mini App API
async fn create_pending_tx(session_key: &str, tx: &Value) -> Result<String> {
    let mini_app_url = std::env::var("MINI_APP_URL").ok();
    let base_url = mini_app_url.unwrap_or_else(|| "http://localhost:3001".to_string());

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/wallet/tx", base_url))
        .json(&serde_json::json!({
            "session_key": session_key,
            "tx": {
                "to": tx.get("to").and_then(|v| v.as_str()).unwrap_or(""),
                "value": tx.get("value").and_then(|v| v.as_str()).unwrap_or("0"),
                "data": tx.get("data").and_then(|v| v.as_str()).unwrap_or("0x"),
                "chainId": tx.get("chainId").and_then(|v| v.as_u64()).unwrap_or(1),
            }
        }))
        .send()
        .await?;

    let result: Value = response.json().await?;
    let tx_id = result
        .get("txId")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(tx_id)
}

/// Handle wallet_tx_request event - create pending tx and show sign button
async fn handle_wallet_tx_request(
    bot: &TelegramBot,
    message: &Message,
    session_key: &str,
    payload: &Value,
) -> Result<bool> {
    info!("Found wallet_tx_request, creating pending tx");

    // Create pending transaction
    match create_pending_tx(session_key, payload).await {
        Ok(tx_id) => {
            debug!("Created pending tx: {}", tx_id);

            // Get transaction details for display
            let to = payload
                .get("to")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let value_wei = payload.get("value").and_then(|v| v.as_str()).unwrap_or("0");
            let description = payload
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Format value as ETH
            let value_eth = value_wei
                .parse::<u128>()
                .map(|v| format!("{:.6} ETH", v as f64 / 1e18))
                .unwrap_or_else(|_| value_wei.to_string());

            // Send sign button
            if let Some(keyboard) = make_sign_keyboard(&tx_id) {
                let msg = format!(
                    "🔐 <b>Transaction requires your signature</b>\n\n\
                    <b>To:</b> <code>{}</code>\n\
                    <b>Amount:</b> {}\n\
                    <b>Description:</b> {}\n\n\
                    Tap the button below to review and sign.",
                    to, value_eth, description
                );
                bot.bot
                    .send_message(message.chat.id, msg)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(keyboard)
                    .await?;
                Ok(true)
            } else {
                bot.bot
                    .send_message(
                        message.chat.id,
                        "⚠️ Transaction signing is not configured. Please set MINI_APP_URL.",
                    )
                    .await?;
                Ok(false)
            }
        }
        Err(e) => {
            warn!("Failed to create pending tx: {}", e);
            bot.bot
                .send_message(
                    message.chat.id,
                    format!("❌ Failed to prepare transaction: {}", e),
                )
                .await?;
            Ok(false)
        }
    }
}

/// Common message processing logic for both DMs and groups.
async fn process_and_respond(
    bot: &TelegramBot,
    message: &Message,
    session_manager: &Arc<SessionManager>,
    session_key: &str,
    text: &str,
) -> Result<()> {
    // Get or create session with default namespace authorization
    let mut auth = NamespaceAuth::new(None, None, None);
    let session = session_manager
        .get_or_create_session(session_key, &mut auth, None)
        .await?;

    // Show typing indicator while processing
    bot.bot
        .send_chat_action(message.chat.id, ChatAction::Typing)
        .await?;

    let mut state = session.lock().await;

    // Check for bound wallet and inject into session
    let wallet_service = DbWalletConnectService::new(bot.pool.clone());
    if let Ok(Some(wallet_address)) = wallet_service.get_bound_wallet(session_key).await {
        debug!(
            "Found bound wallet for session {}: {}",
            session_key, wallet_address
        );
        let user_state = UserState {
            address: Some(wallet_address),
            chain_id: Some(1),
            is_connected: true,
            ens_name: None,
        };
        state.sync_user_state(user_state).await;
    }

    debug!("Sending user input to session: {:?}", text);
    state.send_user_input(text.to_string()).await?;

    // Poll until processing is complete
    // Collect all system events during polling
    let mut all_system_events: Vec<SystemEvent> = Vec::new();
    let mut had_wallet_tx_request = false;

    let max_wait = std::time::Duration::from_secs(60);
    let start = std::time::Instant::now();
    let mut last_typing = std::time::Instant::now();

    loop {
        state.sync_state().await;

        // Collect events from this iteration (advance_http_events consumes them)
        let response = state.format_session_response(None);

        // Check for wallet_tx_request events immediately
        for event in &response.system_events {
            if let SystemEvent::InlineCall(value) = event
                && value.get("type").and_then(|v| v.as_str()) == Some("wallet_tx_request")
                && let Some(payload) = value.get("payload")
            {
                // Handle wallet tx request immediately while we have the lock
                drop(state); // Release lock before async call
                had_wallet_tx_request =
                    handle_wallet_tx_request(bot, message, session_key, payload).await?;
                state = session.lock().await; // Re-acquire lock
            }
            all_system_events.push(event.clone());
        }

        if !response.is_processing {
            debug!("Processing complete after {:?}", start.elapsed());
            break;
        }

        if start.elapsed() > max_wait {
            warn!("Timeout waiting for response after {:?}", start.elapsed());
            bot.bot
                .send_message(message.chat.id, "⏱️ Response timed out. Please try again.")
                .await?;
            return Ok(());
        }

        // Refresh typing indicator every 4 seconds
        if last_typing.elapsed() > std::time::Duration::from_secs(4) {
            let _ = bot
                .bot
                .send_chat_action(message.chat.id, ChatAction::Typing)
                .await;
            last_typing = std::time::Instant::now();
        }

        // Release lock briefly to allow processing
        drop(state);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state = session.lock().await;
    }

    // Get final response (events already collected)
    let response = state.format_session_response(None);
    debug!(
        "Session response has {} messages, collected {} system events",
        response.messages.len(),
        all_system_events.len()
    );

    let assistant_text = extract_assistant_text(&response);
    debug!("Extracted assistant text (len={})", assistant_text.len());

    if assistant_text.is_empty() {
        // If we had a wallet_tx_request, we already sent a message
        if had_wallet_tx_request {
            return Ok(());
        }

        warn!("No assistant response to send!");
        bot.bot
            .send_message(
                message.chat.id,
                "🤔 I didn't generate a response. Please try again.",
            )
            .await?;
        return Ok(());
    }

    let chunks = format_for_telegram(&assistant_text);
    debug!("Formatted into {} chunks", chunks.len());

    if chunks.is_empty() {
        warn!("No chunks to send after formatting!");
        bot.bot
            .send_message(
                message.chat.id,
                "🤔 I didn't generate a response. Please try again.",
            )
            .await?;
        return Ok(());
    }

    for (i, chunk) in chunks.iter().enumerate() {
        if chunk.trim().is_empty() {
            debug!("Skipping empty chunk {}", i);
            continue;
        }
        debug!("Sending chunk {} (len={})", i, chunk.len());
        bot.bot
            .send_message(message.chat.id, chunk)
            .parse_mode(ParseMode::Html)
            .await?;
    }

    Ok(())
}

/// Check if the bot is mentioned in a message.
async fn is_bot_mentioned(bot: &teloxide::Bot, message: &Message) -> Result<bool> {
    let me = bot.get_me().await?;
    let bot_username: Option<&str> = me.username.as_deref();

    // Check if message is a reply to the bot
    if let Some(reply_to) = &message.reply_to_message()
        && let Some(ref from) = reply_to.from
        && from.id == me.id
    {
        return Ok(true);
    }

    // Check for mentions in entities
    if let Some(entities) = message.entities() {
        for entity in entities {
            if let MessageEntityKind::Mention = entity.kind
                && let Some(text) = message.text()
            {
                let start = entity.offset;
                let end = start + entity.length;
                if let Some(mention) = text.get(start..end) {
                    let mentioned_username = mention.trim_start_matches('@');
                    if let Some(bot_user) = bot_username
                        && mentioned_username == bot_user
                    {
                        return Ok(true);
                    }
                }
            }
        }
    }

    Ok(false)
}
