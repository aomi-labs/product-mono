//! Message handlers for routing Telegram updates.

use anyhow::{bail, Result};
use teloxide::types::{ChatKind, Message, MessageEntityKind};
use tracing::{debug, info, warn};

use crate::{
    config::{DmPolicy, GroupPolicy},
    session::{dm_session_key, group_session_key, user_id_from_message},
    TelegramBot,
};

/// Main message handler that routes based on chat type.
///
/// Routes to `handle_dm` for private chats, `handle_group` for groups/supergroups.
pub async fn handle_message(bot: &TelegramBot, message: &Message) -> Result<()> {
    match message.chat.kind {
        ChatKind::Private(_) => handle_dm(bot, message).await,
        ChatKind::Group(_) | ChatKind::Supergroup(_) => handle_group(bot, message).await,
        ChatKind::Channel(_) => {
            debug!("Ignoring channel message");
            Ok(())
        }
    }
}

/// Handle direct message (DM) from a user.
///
/// Checks `DmPolicy` to determine if the message should be processed:
/// - `Open`: Process all DMs
/// - `Allowlist`: Only process DMs from allowlisted users
/// - `Disabled`: Reject all DMs
async fn handle_dm(bot: &TelegramBot, message: &Message) -> Result<()> {
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
            if !bot.config.is_allowlisted(user_id.0) {
                debug!(
                    "User {} not in allowlist, ignoring DM",
                    user_id
                );
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

    // TODO: Process message through SessionManager
    // For now, just log the session key
    debug!("Would process with session key: {}", session_key);

    Ok(())
}

/// Handle group message (group or supergroup).
///
/// Checks `GroupPolicy` to determine if the message should be processed:
/// - `Always`: Process all messages
/// - `Mention`: Only process messages that mention the bot
/// - `Disabled`: Ignore all group messages
async fn handle_group(bot: &TelegramBot, message: &Message) -> Result<()> {
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

    // TODO: Process message through SessionManager
    // For now, just log the session key
    debug!("Would process with session key: {}", session_key);

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
    let bot_username = me.username.as_ref();

    // Check if message is a reply to the bot
    if let Some(reply_to) = &message.reply_to_message() {
        if let Some(from) = reply_to.from() {
            if from.id == me.id {
                return Ok(true);
            }
        }
    }

    // Check for mentions in entities
    if let Some(entities) = message.entities() {
        for entity in entities {
            if let MessageEntityKind::Mention = entity.kind {
                if let Some(text) = message.text() {
                    let start = entity.offset as usize;
                    let end = start + entity.length as usize;
                    if let Some(mention) = text.get(start..end) {
                        // Remove @ prefix and compare
                        let mentioned_username = mention.trim_start_matches('@');
                        if let Some(bot_user) = bot_username {
                            if mentioned_username == bot_user.as_str() {
                                return Ok(true);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}
