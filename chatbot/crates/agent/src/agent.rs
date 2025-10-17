// Environment variables
pub static ANTHROPIC_API_KEY: std::sync::LazyLock<Result<String, std::env::VarError>> =
    std::sync::LazyLock::new(|| std::env::var("ANTHROPIC_API_KEY"));
use eyre::Result;
use futures::StreamExt;
use rig::{
    agent::Agent,
    message::{Message, Text},
    prelude::*,
    providers::anthropic::completion::CompletionModel,
};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::docs::{self, LoadingProgress};
use crate::helpers::multi_turn_prompt;
use crate::mcp;
use crate::{abi_encoder, time};
use crate::{accounts::generate_account_context, wallet};

const CLAUDE_3_5_SONNET: &str = "claude-sonnet-4-20250514";

#[derive(Debug, Clone)]
pub enum AgentMessage {
    StreamingText(String),
    ToolCall { name: String, args: String },
    Complete,
    Error(String),
    System(String),
    BackendConnected,
    BackendConnecting(String),
    MissingApiKey,
    Interrupted,
    WalletTransactionRequest(String),
}

fn preamble() -> String {
    format!(
        r#"
You are an Ethereum operations assistant. You can say \"I don't know\" or \"that failed\" when appropriate.

If there is information you don't have, search for it with Brave Search.

Prefer Uniswap V2 for swaps over 0x API.

Always get the current timestamp when swapping for expiration.

<workflow>
1. Explain your current step succinctly
2. Execute using tools and wait for responses
3. Report actual results (including failures) succinctly.
4. Continue until complete or blocked
</workflow>

<constraints>
- Check if transactions are successful.
- If a tool fails, report the error - don't pretend it worked
- Show new recipient balances at the end of a request that involves a balance change.
- When a transaction is rejected/cancelled by the user, acknowledge it gracefully and offer alternatives or ask what they'd like to do next.

For each user request:
Don't:
- make a numbered list of your steps.
- talk to the user between tool calls if the same step requires multiple tool calls.
Do:
- talk to the user between *steps* (which can be more than one tool call)
</constraints>

# Network Switching
When you receive a system message indicating wallet network detection (e.g., "detected wallet connect to mainnet"), you should:
1. Acknowledge the network mismatch
2. Ask the user for confirmation to switch networks
3. If the user confirms, use the set_network tool to switch the network
4. If the user declines, acknowledge their choice and continue with the current network
5. When you are NOT on testnet, always use send_transaction_to_wallet tool to send transactions. Don't use send tool.

Example response:
"I see your wallet is connected to mainnet. Would you like me to switch? This will allow me to work with your actual wallet transactions."
"I see you disconnected your wallet. Would you like to go back to testnet?"

# Token Queries
User etherscan tools primarily for token related queries. If it fails, fall back to calling contract ABI.

Common ERC20 ABI functions you might encode:
- transfer(address,uint256) - Transfer tokens to an address
- approve(address,uint256) - Approve an address to spend tokens
- transferFrom(address,address,uint256) - Transfer tokens from one address to another
- balanceOf(address) - Get token balance of an address
- allowance(address,address) - Get spending allowance
- name() - Get token name
- symbol() - Get token symbol
- decimals() - Get token decimals
- totalSupply() - Get total token supply

{}"#,
        generate_account_context()
    )
}

// For simple REPL
pub async fn setup_agent() -> Result<Arc<Agent<CompletionModel>>> {
    let anthropic_api_key = ANTHROPIC_API_KEY
        .as_ref()
        .map_err(|_| eyre::eyre!("ANTHROPIC_API_KEY not set"))?
        .clone();

    let anthropic_client = rig::providers::anthropic::Client::new(&anthropic_api_key);

    let mcp_toolbox = mcp::toolbox().await?;
    mcp_toolbox.ensure_connected().await?;

    // ingest uniswap docs
    let document_store = docs::initialize_document_store().await?;
    let uniswap_tool = docs::SearchUniswapDocs::new(document_store);

    let agent_builder = anthropic_client
        .agent(CLAUDE_3_5_SONNET)
        .preamble(&preamble())
        .tool(wallet::SendTransactionToWallet)
        .tool(abi_encoder::EncodeFunctionCall)
        .tool(time::GetCurrentTime)
        .tool(uniswap_tool);

    let server_sink = mcp_toolbox.server_sink();
    let agent = mcp_toolbox
        .tools()
        .iter()
        .fold(agent_builder, |agent, tool| {
            agent.rmcp_tool(tool.clone(), server_sink.clone())
        })
        .build();

    let agent = Arc::new(agent);

    // Test connection to Anthropic API before returning
    test_model_connection(&agent).await?;

    Ok(agent)
}

// Test connection to Anthropic API with a simple request (for simple REPL setup)
async fn test_model_connection(agent: &Arc<Agent<CompletionModel>>) -> Result<()> {
    use rig::completion::Prompt;

    // Send a simple test message to verify the connection
    let test_prompt = "Say 'Connection test successful' and nothing else.";

    match agent.prompt(test_prompt).await {
        Ok(_response) => {
            println!("✓ Anthropic API connection successful");
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("✗ Anthropic API connection failed: {}", e);
            eprintln!("{}", error_msg);
            Err(eyre::eyre!(error_msg))
        }
    }
}

// For TUI
pub async fn setup_agent_and_handle_messages(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<AgentMessage>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let anthropic_api_key = match ANTHROPIC_API_KEY.as_ref() {
        Ok(key) => key,
        Err(_) => {
            let _ = sender_to_ui.send(AgentMessage::MissingApiKey).await;
            // Wait indefinitely instead of returning an error - the popup will handle this
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    };

    let anthropic_client = rig::providers::anthropic::Client::new(anthropic_api_key);

    // Connect to MCP server with retry logic
    let mcp_toolbox = match mcp::toolbox().await {
        Ok(toolbox) => toolbox,
        Err(err) => {
            let _ = sender_to_ui
                .send(AgentMessage::Error(format!(
                    "MCP connection failed: {err}. Retrying..."
                )))
                .await;
            mcp::toolbox_with_retry(sender_to_ui.clone()).await?
        }
    };

    // ingest uniswap docs with progress reporting (or skip if --no-docs flag is set)
    let uniswap_docs_rag_tool = if skip_docs {
        // Send loading complete immediately if skipping docs
        let _ = loading_sender.send(LoadingProgress::Complete).await;
        match docs::SearchUniswapDocs::new_empty().await {
            Ok(tool) => tool,
            Err(e) => {
                let _ = sender_to_ui
                    .send(AgentMessage::Error(format!(
                        "Failed to create empty document store: {e}"
                    )))
                    .await;
                return Err(e);
            }
        }
    } else {
        let document_store =
            match docs::initialize_document_store_with_progress(Some(loading_sender)).await {
                Ok(store) => store,
                Err(e) => {
                    let _ = sender_to_ui
                        .send(AgentMessage::Error(format!(
                            "Failed to load Uniswap documentation: {e}"
                        )))
                        .await;
                    return Err(e);
                }
            };
        docs::SearchUniswapDocs::new(document_store)
    };

    let tools = mcp_toolbox.tools();

    let agent_builder = anthropic_client
        .agent(CLAUDE_3_5_SONNET)
        .preamble(&preamble())
        .tool(wallet::SendTransactionToWallet)
        .tool(abi_encoder::EncodeFunctionCall)
        .tool(time::GetCurrentTime)
        .tool(uniswap_docs_rag_tool);

    let server_sink = mcp_toolbox.server_sink();
    let agent = tools
        .iter()
        .fold(agent_builder, |agent, tool| {
            agent.rmcp_tool(tool.clone(), server_sink.clone())
        })
        .build();

    let agent = Arc::new(agent);

    // Test connection to Anthropic API with retry logic (same as MCP)
    let max_attempts = 3;
    let mut attempt = 1;
    let mut delay = std::time::Duration::from_millis(500);

    loop {
        let _ = sender_to_ui
            .send(AgentMessage::BackendConnecting(
                "Testing connection to Anthropic API...".to_string(),
            ))
            .await;

        match test_model_connection(&agent).await {
            Ok(()) => {
                let _ = sender_to_ui
                    .send(AgentMessage::System(
                        "✓ Anthropic API connection successful".to_string(),
                    ))
                    .await;
                let _ = sender_to_ui.send(AgentMessage::BackendConnected).await;
                break;
            }
            Err(e) => {
                if attempt >= max_attempts {
                    let _ = sender_to_ui.send(AgentMessage::Error(
                        format!("Failed to connect to Anthropic API after {max_attempts} attempts: {e}. Please check your API key and connection.")
                    )).await;
                    return Err(e);
                }

                let _ = sender_to_ui
                    .send(AgentMessage::BackendConnecting(format!(
                        "Connection failed, retrying in {:.1}s...",
                        delay.as_secs_f32()
                    )))
                    .await;

                tokio::time::sleep(delay).await;
                delay = std::cmp::min(delay * 2, std::time::Duration::from_secs(5)); // Max 5 second delay
                attempt += 1;
            }
        }
    }

    // Handle messages - client stays alive for entire duration
    handle_agent_messages(agent, receiver_from_ui, sender_to_ui, interrupt_receiver).await;

    Ok(())
}

/// Enables TUI message handling.
pub async fn handle_agent_messages(
    agent: Arc<Agent<CompletionModel>>,
    mut receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<AgentMessage>,
    mut interrupt_receiver: mpsc::Receiver<()>,
) {
    let mut chat_history = Vec::new();

    while let Some(input) = receiver_from_ui.recv().await {
        let mut stream = multi_turn_prompt(agent.clone(), &input, chat_history.clone()).await;
        let mut response = String::new();

        // Process stream content, handling special markers for tool calls/results and regular text
        let mut interrupted = false;
        loop {
            tokio::select! {
                content = stream.next() => {
                    match content {
                        Some(Ok(Text { text })) => {
                            if text.starts_with("[[TOOL_CALL:") && text.contains("]]") {
                                let marker_end = text.rfind("]]").unwrap_or(text.len());
                                let content = &text[12..marker_end];
                                if let Some(colon_idx) = content.find(':') {
                                    let name = content[..colon_idx].to_string();
                                    let args = content[colon_idx + 1..].to_string();
                                    let _ = sender_to_ui.send(AgentMessage::ToolCall { name, args }).await;
                                }
                            } else if text.starts_with("[[TOOL_RESULT:") && text.contains("]]") {
                                let marker_end = text.rfind("]]").unwrap_or(text.len());
                                let result = &text[14..marker_end];
                                let _ = sender_to_ui.send(AgentMessage::System(result.to_string())).await;
                            } else if text.starts_with("[[TOOL_ERROR:") && text.contains("]]") {
                                let marker_end = text.rfind("]]").unwrap_or(text.len());
                                let error = &text[13..marker_end];
                                let _ = sender_to_ui
                                    .send(AgentMessage::Error(format!("error: {error}")))
                                    .await;
                            } else if text.starts_with("[[SYSTEM:") && text.contains("]]") {
                                let marker_end = text.rfind("]]").unwrap_or(text.len());
                                let system_content = &text[9..marker_end];
                                let _ = sender_to_ui.send(AgentMessage::System(system_content.to_string())).await;
                            } else if text.starts_with("[[WALLET_TX_REQUEST:") && text.contains("]]") {
                                let marker_end = text.rfind("]]").unwrap_or(text.len());
                                let tx_request_json = &text[20..marker_end];
                                let _ = sender_to_ui.send(AgentMessage::WalletTransactionRequest(tx_request_json.to_string())).await;
                            }
                            else {
                                response.push_str(&text);
                                let _ = sender_to_ui.send(AgentMessage::StreamingText(text)).await;
                            }
                        }
                        Some(Err(err)) => {
                            let _ = sender_to_ui.send(AgentMessage::Error(err.to_string())).await;
                        }
                        None => {
                            // Stream ended normally
                            break;
                        }
                    }
                }
                _ = interrupt_receiver.recv() => {
                    // Interrupt received, stop processing
                    interrupted = true;
                    let _ = sender_to_ui.send(AgentMessage::Interrupted).await;
                    break;
                }
            }
        }

        if !interrupted {
            chat_history.push(Message::user(input));
            chat_history.push(Message::assistant(response));
            let _ = sender_to_ui.send(AgentMessage::Complete).await;
        } else {
            // Don't add to chat history if interrupted
            // Just add the user input since the response was incomplete
            chat_history.push(Message::user(input));
        }
    }
}
