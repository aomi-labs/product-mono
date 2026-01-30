//! Message handlers for routing Discord events.

use anyhow::Result;
use serenity::all::{Context, Message};
use std::sync::Arc;
use tracing::{debug, info, warn};

use aomi_backend::SessionManager;
use aomi_bot_core::handler::extract_assistant_text;

use crate::{
    config::{DiscordConfig, DmPolicy, GuildPolicy},
    send::format_for_discord,
    session::{channel_session_key, dm_session_key},
};

/// Main message handler that routes based on channel type.
pub async fn handle_message(
    ctx: &Context,
    msg: &Message,
    config: &DiscordConfig,
    session_manager: &Arc<SessionManager>,
    bot_id: u64,
) -> Result<()> {
    // Ignore messages from bots (including self)
    if msg.author.bot {
        return Ok(());
    }

    // Check if this is a DM or guild message
    if msg.guild_id.is_none() {
        handle_dm(ctx, msg, config, session_manager).await
    } else {
        handle_guild(ctx, msg, config, session_manager, bot_id).await
    }
}

/// Handle direct message (DM) from a user.
async fn handle_dm(
    ctx: &Context,
    msg: &Message,
    config: &DiscordConfig,
    session_manager: &Arc<SessionManager>,
) -> Result<()> {
    let user_id = msg.author.id.get();

    // Check DM policy
    match config.dm_policy {
        DmPolicy::Disabled => {
            debug!("DM policy is disabled, ignoring message from {}", user_id);
            return Ok(());
        }
        DmPolicy::Allowlist => {
            if !config.is_allowlisted(user_id) {
                debug!("User {} not in allowlist, ignoring DM", user_id);
                return Ok(());
            }
        }
        DmPolicy::Open => {
            // Process all DMs
        }
    }

    let session_key = dm_session_key(msg.author.id);
    let text = &msg.content;

    info!(
        "Processing DM from user {} (session: {}): {}",
        user_id, session_key, text
    );

    process_and_respond(ctx, msg, session_manager, &session_key, text).await
}

/// Handle guild (server) message.
async fn handle_guild(
    ctx: &Context,
    msg: &Message,
    config: &DiscordConfig,
    session_manager: &Arc<SessionManager>,
    bot_id: u64,
) -> Result<()> {
    let user_id = msg.author.id.get();

    // Check guild policy
    let should_process = match config.guild_policy {
        GuildPolicy::Disabled => {
            debug!("Guild policy is disabled, ignoring message");
            return Ok(());
        }
        GuildPolicy::Always => true,
        GuildPolicy::Mention => {
            // Check if bot is mentioned
            is_bot_mentioned(msg, bot_id)
        }
    };

    if !should_process {
        debug!("Bot not mentioned in guild message, ignoring");
        return Ok(());
    }

    // Also check user allowlist if configured
    if config.dm_policy == DmPolicy::Allowlist && !config.is_allowlisted(user_id) {
        debug!("User {} not in allowlist, ignoring guild message", user_id);
        return Ok(());
    }

    let session_key = channel_session_key(msg.channel_id);
    
    // Remove bot mention from the message text
    let text = remove_bot_mention(&msg.content, bot_id);
    
    if text.trim().is_empty() {
        debug!("Message is empty after removing mention, ignoring");
        return Ok(());
    }

    info!(
        "Processing guild message from user {} in channel {} (session: {}): {}",
        user_id,
        msg.channel_id.get(),
        session_key,
        text
    );

    process_and_respond(ctx, msg, session_manager, &session_key, &text).await
}

/// Common message processing logic for both DMs and groups.
async fn process_and_respond(
    ctx: &Context,
    msg: &Message,
    session_manager: &Arc<SessionManager>,
    session_key: &str,
    text: &str,
) -> Result<()> {
    // Show typing indicator
    let typing = msg.channel_id.start_typing(&ctx.http);

    // Get or create session
    let session = session_manager
        .get_or_create_session(session_key, None)
        .await?;

    let mut state = session.lock().await;

    debug!("Sending user input to session: {:?}", text);
    state.send_user_input(text.to_string()).await?;

    // Poll until processing is complete
    let max_wait = std::time::Duration::from_secs(60);
    let start = std::time::Instant::now();

    loop {
        state.sync_state().await;
        let response = state.format_session_response(None);

        if !response.is_processing {
            debug!("Processing complete after {:?}", start.elapsed());
            break;
        }

        if start.elapsed() > max_wait {
            warn!("Timeout waiting for response after {:?}", start.elapsed());
            msg.channel_id
                .say(&ctx.http, "⏱️ Response timed out. Please try again.")
                .await?;
            return Ok(());
        }

        // Release lock briefly to allow processing
        drop(state);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state = session.lock().await;
    }

    // Stop typing indicator
    drop(typing);

    let response = state.format_session_response(None);
    debug!("Session response has {} messages", response.messages.len());

    let assistant_text = extract_assistant_text(&response);
    debug!("Extracted assistant text (len={})", assistant_text.len());

    if assistant_text.is_empty() {
        warn!("No assistant response to send!");
        msg.channel_id
            .say(&ctx.http, "🤔 I didn't generate a response. Please try again.")
            .await?;
        return Ok(());
    }

    let chunks = format_for_discord(&assistant_text);
    debug!("Formatted into {} chunks", chunks.len());

    if chunks.is_empty() {
        warn!("No chunks to send after formatting!");
        msg.channel_id
            .say(&ctx.http, "🤔 I didn't generate a response. Please try again.")
            .await?;
        return Ok(());
    }

    for (i, chunk) in chunks.iter().enumerate() {
        if chunk.trim().is_empty() {
            debug!("Skipping empty chunk {}", i);
            continue;
        }
        debug!("Sending chunk {} (len={})", i, chunk.len());
        msg.channel_id.say(&ctx.http, chunk).await?;
    }

    Ok(())
}

/// Check if the bot is mentioned in a message.
fn is_bot_mentioned(msg: &Message, bot_id: u64) -> bool {
    // Check direct mentions
    for mention in &msg.mentions {
        if mention.id.get() == bot_id {
            return true;
        }
    }

    // Check if message is a reply to the bot
    if let Some(ref referenced) = msg.referenced_message {
        if referenced.author.id.get() == bot_id {
            return true;
        }
    }

    false
}

/// Remove bot mention from message text.
fn remove_bot_mention(content: &str, bot_id: u64) -> String {
    let mention_pattern = format!("<@{}>", bot_id);
    let mention_pattern_nick = format!("<@!{}>", bot_id);
    
    content
        .replace(&mention_pattern, "")
        .replace(&mention_pattern_nick, "")
        .trim()
        .to_string()
}
