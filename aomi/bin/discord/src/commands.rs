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

/// Create connect wallet button - opens wallet connect page in browser
fn make_connect_button() -> Option<CreateButton> {
    get_mini_app_url().map(|url| {
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
    let response = if let Some(button) = make_connect_button() {
        CreateInteractionResponseMessage::new()
            .content("Click the button below to connect your wallet:")
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

    let wallet_service = DbWalletConnectService::new(pool.clone());

    let response = match wallet_service.get_bound_wallet(&session_key).await {
        Ok(Some(address)) => {
            let msg = format!("💳 **Connected wallet:**\n\n`{}`", address);

            if let Some(url) = get_mini_app_url() {
                CreateInteractionResponseMessage::new()
                    .content(msg)
                    .components(vec![CreateActionRow::Buttons(vec![
                        CreateButton::new_link(url)
                            .label("🔄 Change Wallet")
                    ])])
            } else {
                CreateInteractionResponseMessage::new().content(msg)
            }
        }
        Ok(None) => {
            if let Some(button) = make_connect_button() {
                CreateInteractionResponseMessage::new()
                    .content("No wallet connected. Click below to connect:")
                    .components(vec![CreateActionRow::Buttons(vec![button])])
            } else {
                CreateInteractionResponseMessage::new()
                    .content("No wallet connected. Use /connect to link your wallet.")
            }
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
            info!("User {} disconnected wallet", user_id);
            CreateInteractionResponseMessage::new()
                .content("✅ Wallet disconnected.")
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
            `/wallet` - Show connected wallet\n\
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
