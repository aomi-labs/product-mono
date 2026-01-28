//! Message handlers for routing Telegram updates.

use anyhow::Result;
use std::sync::Arc;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{ChatAction, Message, MessageEntityKind, ParseMode};
use tracing::{debug, info, warn};

use crate::{
    TelegramBot,
    config::{DmPolicy, GroupPolicy},
    send::format_for_telegram,
    session::{dm_session_key, group_session_key, user_id_from_message},
};
use aomi_backend::{MessageSender, SessionManager, SessionResponse};

fn extract_assistant_text(response: &SessionResponse) -> String {
    // Get only the LAST assistant message (delta), not the full history
    response
        .messages
        .iter()
        .filter(|m| matches!(m.sender, MessageSender::Assistant))
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default()
}

/// Main message handler that routes based on chat type.
///
/// Routes to `handle_dm` for private chats, `handle_group` for groups/supergroups.
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
///
/// Checks `DmPolicy` to determine if the message should be processed:
/// - `Open`: Process all DMs
/// - `Allowlist`: Only process DMs from allowlisted users
/// - `Disabled`: Reject all DMs
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
            // Convert u64 to i64 for allowlist check
            let user_id_i64 = user_id.0 as i64;
            if !bot.config.is_allowlisted(user_id_i64) {
                debug!("User {} not in allowlist, ignoring DM", user_id);
                return Ok(());
            }
        }
        DmPolicy::Open => {
            // Process all DMs
        }
    }

    let session_key = dm_session_key(user_id);
    let text = message.text().unwrap_or("");

    info!(
        "Processing DM from user {} (session: {}): {}",
        user_id, session_key, text
    );

    // Get or create session
    let session = session_manager
        .get_or_create_session(&session_key, None)
        .await?;

    // Show typing indicator while processing
    bot.bot.send_chat_action(message.chat.id, ChatAction::Typing).await?;

    let mut state = session.lock().await;
    
    debug!("Sending user input to session: {:?}", text);
    state.send_user_input(text.to_string()).await?;
    
    // Poll until processing is complete (like the HTTP endpoint pattern)
    let max_wait = std::time::Duration::from_secs(60);
    let start = std::time::Instant::now();
    let mut last_typing = std::time::Instant::now();
    
    loop {
        state.sync_state().await;
        let response = state.format_session_response(None);
        
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
        
        // Refresh typing indicator every 4 seconds (Telegram typing lasts ~5s)
        if last_typing.elapsed() > std::time::Duration::from_secs(4) {
            let _ = bot.bot.send_chat_action(message.chat.id, ChatAction::Typing).await;
            last_typing = std::time::Instant::now();
        }
        
        // Release lock briefly to allow processing
        drop(state);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state = session.lock().await;
    }
    
    let response = state.format_session_response(None);
    debug!("Session response has {} messages", response.messages.len());
    
    for (i, msg) in response.messages.iter().enumerate() {
        debug!("  Message {}: sender={:?}, content_len={}", i, msg.sender, msg.content.len());
    }

    let assistant_text = extract_assistant_text(&response);
    debug!("Extracted assistant text (len={})", assistant_text.len());
    
    if assistant_text.is_empty() {
        warn!("No assistant response to send!");
        bot.bot
            .send_message(message.chat.id, "🤔 I didn't generate a response. Please try again.")
            .await?;
        return Ok(());
    }
    
    let chunks = format_for_telegram(&assistant_text);
    debug!("Formatted into {} chunks", chunks.len());
    
    if chunks.is_empty() {
        warn!("No chunks to send after formatting!");
        bot.bot
            .send_message(message.chat.id, "🤔 I didn't generate a response. Please try again.")
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

/// Handle group message (group or supergroup).
///
/// Checks `GroupPolicy` to determine if the message should be processed:
/// - `Always`: Process all messages
/// - `Mention`: Only process messages that mention the bot
/// - `Disabled`: Ignore all group messages
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
        GroupPolicy::Mention => {
            // Check if bot is mentioned
            is_bot_mentioned(&bot.bot, message).await?
        }
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

    // Get or create session
    let session = session_manager
        .get_or_create_session(&session_key, None)
        .await?;

    // Show typing indicator while processing
    bot.bot.send_chat_action(message.chat.id, ChatAction::Typing).await?;

    let mut state = session.lock().await;
    
    debug!("[GROUP] Sending user input to session: {:?}", text);
    state.send_user_input(text.to_string()).await?;
    
    // Poll until processing is complete
    let max_wait = std::time::Duration::from_secs(60);
    let start = std::time::Instant::now();
    let mut last_typing = std::time::Instant::now();
    
    loop {
        state.sync_state().await;
        let response = state.format_session_response(None);
        
        if !response.is_processing {
            debug!("[GROUP] Processing complete after {:?}", start.elapsed());
            break;
        }
        
        if start.elapsed() > max_wait {
            warn!("[GROUP] Timeout waiting for response");
            bot.bot
                .send_message(message.chat.id, "⏱️ Response timed out. Please try again.")
                .await?;
            return Ok(());
        }
        
        // Refresh typing indicator every 4 seconds
        if last_typing.elapsed() > std::time::Duration::from_secs(4) {
            let _ = bot.bot.send_chat_action(message.chat.id, ChatAction::Typing).await;
            last_typing = std::time::Instant::now();
        }
        
        drop(state);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state = session.lock().await;
    }
    
    let response = state.format_session_response(None);
    debug!("[GROUP] Session response has {} messages", response.messages.len());

    let assistant_text = extract_assistant_text(&response);
    debug!("[GROUP] Extracted assistant text (len={})", assistant_text.len());
    
    if assistant_text.is_empty() {
        warn!("[GROUP] No assistant response to send!");
        bot.bot
            .send_message(message.chat.id, "🤔 I didn't generate a response. Please try again.")
            .await?;
        return Ok(());
    }
    
    let chunks = format_for_telegram(&assistant_text);
    for chunk in chunks {
        if chunk.trim().is_empty() {
            continue;
        }
        bot.bot
            .send_message(message.chat.id, &chunk)
            .parse_mode(ParseMode::Html)
            .await?;
    }

    Ok(())
}

/// Check if the bot is mentioned in a message.
///
/// Returns `true` if:
/// - Message contains a mention entity with the bot's username
/// - Message is a reply to a message from the bot
async fn is_bot_mentioned(bot: &teloxide::Bot, message: &Message) -> Result<bool> {
    // Get bot username
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
                    // Remove @ prefix and compare
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
