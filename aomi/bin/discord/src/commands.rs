//! Slash command handlers for Discord bot.

use anyhow::Result;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateButton, CreateActionRow,
};
use tracing::{info, warn};

use aomi_bot_core::{DbWalletConnectService, WalletConnectService};
use sqlx::{Any, Pool};

use crate::session::dm_session_key;

/// Get Mini App URL for wallet connections
fn get_mini_app_url() -> Option<String> {
    std::env::var("MINI_APP_URL").ok()
}

/// Create connect wallet button with session context
/// The URL includes the session_key so the mini app can bind to the correct session
fn make_connect_button_with_session(session_key: &str) -> Option<CreateButton> {
    get_mini_app_url().map(|base_url| {
        // URL encode the session key
        let encoded_session = urlencoding::encode(session_key);
        let url = format!("{}?session_key={}", base_url, encoded_session);
        CreateButton::new_link(url)
            .label("🔗 Connect Wallet")
    })
}

/// Create sign transaction button with tx_id parameter
pub fn make_sign_button(tx_id: &str) -> Option<CreateButton> {
    get_mini_app_url().map(|base_url| {
        let url = format!("{}/sign?tx_id={}", base_url, tx_id);
        CreateButton::new_link(url)
            .label("🔐 Sign Transaction")
    })
}

/// Register all slash commands with Discord.
pub fn register_commands() -> Vec<CreateCommand> {
    vec![
        CreateCommand::new("connect")
            .description("Connect your Ethereum wallet"),
        CreateCommand::new("wallet")
            .description("Show your connected wallet"),
        CreateCommand::new("disconnect")
            .description("Disconnect your wallet"),
        CreateCommand::new("help")
            .description("Show available commands"),
    ]
}

/// Handle slash command interactions.
pub async fn handle_command(
    ctx: &Context,
    command: &CommandInteraction,
    pool: &Pool<Any>,
) -> Result<()> {
    let cmd_name = command.data.name.as_str();
    
    match cmd_name {
        "connect" => handle_connect(ctx, command).await,
        "wallet" => handle_wallet(ctx, command, pool).await,
        "disconnect" => handle_disconnect(ctx, command, pool).await,
        "help" => handle_help(ctx, command).await,
        _ => {
            warn!("Unknown command: {}", cmd_name);
            Ok(())
        }
    }
}

/// Handle /connect command.
async fn handle_connect(ctx: &Context, command: &CommandInteraction) -> Result<()> {
    let user_id = command.user.id;
    let session_key = dm_session_key(user_id);
    
    let response = if let Some(button) = make_connect_button_with_session(&session_key) {
        CreateInteractionResponseMessage::new()
            .content("Click the button below to connect your wallet:\n\n💡 *After connecting, use `/wallet` to verify your connection.*")
            .components(vec![CreateActionRow::Buttons(vec![button])])
    } else {
        CreateInteractionResponseMessage::new()
            .content("⚠️ Wallet connect is not configured. Please ask the admin to set MINI_APP_URL.")
            .ephemeral(true)
    };

    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

/// Handle /wallet command.
async fn handle_wallet(
    ctx: &Context,
    command: &CommandInteraction,
    pool: &Pool<Any>,
) -> Result<()> {
    let user_id = command.user.id;
    let session_key = dm_session_key(user_id);

    info!("Checking wallet for session: {}", session_key);
    
    let wallet_service = DbWalletConnectService::new(pool.clone());

    let response = match wallet_service.get_bound_wallet(&session_key).await {
        Ok(Some(address)) => {
            info!("Found wallet {} for session {}", address, session_key);
            let short_addr = format!("{}...{}", &address[..6], &address[address.len()-4..]);
            let msg = format!(
                "💳 **Wallet Connected**\n\n\
                Address: `{}`\n\
                Short: `{}`",
                address, short_addr
            );

            if let Some(button) = make_connect_button_with_session(&session_key) {
                CreateInteractionResponseMessage::new()
                    .content(msg)
                    .components(vec![CreateActionRow::Buttons(vec![
                        button,
                    ])])
            } else {
                CreateInteractionResponseMessage::new().content(msg)
            }
        }
        Ok(None) => {
            info!("No wallet found for session {}", session_key);
            if let Some(button) = make_connect_button_with_session(&session_key) {
                CreateInteractionResponseMessage::new()
                    .content("❌ **No wallet connected**\n\nClick below to connect your wallet:")
                    .components(vec![CreateActionRow::Buttons(vec![button])])
            } else {
                CreateInteractionResponseMessage::new()
                    .content("❌ No wallet connected. Use `/connect` to link your wallet.")
            }
        }
        Err(e) => {
            warn!("Error getting wallet for session {}: {}", session_key, e);
            CreateInteractionResponseMessage::new()
                .content(format!("❌ Error checking wallet: {}", e))
                .ephemeral(true)
        }
    };

    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

/// Handle /disconnect command.
async fn handle_disconnect(
    ctx: &Context,
    command: &CommandInteraction,
    pool: &Pool<Any>,
) -> Result<()> {
    let user_id = command.user.id;
    let session_key = dm_session_key(user_id);

    let wallet_service = DbWalletConnectService::new(pool.clone());

    let response = match wallet_service.disconnect(&session_key).await {
        Ok(()) => {
            info!("User {} disconnected wallet (session: {})", user_id, session_key);
            CreateInteractionResponseMessage::new()
                .content("✅ Wallet disconnected successfully.")
        }
        Err(e) => {
            CreateInteractionResponseMessage::new()
                .content(format!("❌ Error: {}", e))
                .ephemeral(true)
        }
    };

    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

/// Handle /help command.
async fn handle_help(ctx: &Context, command: &CommandInteraction) -> Result<()> {
    let response = CreateInteractionResponseMessage::new()
        .content(
            "🤖 **Aomi Commands**\n\n\
            `/connect` - Link your Ethereum wallet\n\
            `/wallet` - Show connected wallet status\n\
            `/disconnect` - Unlink your wallet\n\
            `/help` - Show this message\n\n\
            Or just chat with me naturally!"
        );

    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_commands() {
        let commands = register_commands();
        assert_eq!(commands.len(), 4);
    }
}
