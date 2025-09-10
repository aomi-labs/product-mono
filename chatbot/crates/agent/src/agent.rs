use eyre::Result;
use futures::StreamExt;
use rig::{
    agent::Agent,
    message::{Message, Text},
    prelude::*,
    providers::anthropic::completion::CompletionModel,
};
use rmcp::{
    ServiceExt,
    model::{ClientCapabilities, ClientInfo, Implementation, Tool as RmcpTool},
    transport::StreamableHttpClientTransport,
};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::accounts::generate_account_context;
use crate::docs::{self, LoadingProgress};
use crate::helpers::multi_turn_prompt;
use crate::{abi_encoder, time};

const CLAUDE_3_5_SONNET: &str = "claude-sonnet-4-20250514";

#[derive(Debug, Clone)]
pub enum AgentMessage {
    StreamingText(String),
    ToolCall { name: String, args: String },
    Complete,
    Error(String),
    System(String),
    McpConnected,
    McpConnecting(String),
    MissingApiKey,
    Interrupted,
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

For each user request:
Don't:
- make a numbered list of your steps.
- talk to the user between tool calls if the same step requires multiple tool calls.
Do:
- talk to the user between *steps* (which can be more than one tool call)
</constraints>

Common ERC20 functions you might encode:
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
    let anthropic_api_key =
        std::env::var("ANTHROPIC_API_KEY").map_err(|_| eyre::eyre!("ANTHROPIC_API_KEY not set"))?;

    let anthropic_client = rig::providers::anthropic::Client::new(&anthropic_api_key);

    let transport = StreamableHttpClientTransport::from_uri("http://127.0.0.1:3000");

    let client_info = ClientInfo {
        protocol_version: Default::default(),
        capabilities: ClientCapabilities::default(),
        client_info: Implementation::default(),
    };

    let client = client_info.serve(transport).await.map_err(|e| {
        eyre::eyre!("Failed to connect to MCP server at http://127.0.0.1:3000: {:?}. Make sure the MCP server is running.", e)
    })?;

    let _server_info = client.peer_info();

    // ingest uniswap docs
    let document_store = docs::initialize_document_store().await?;
    let uniswap_tool = docs::SearchUniswapDocs::new(document_store);

    let tools: Vec<RmcpTool> = client.list_tools(Default::default()).await?.tools;

    let agent_builder = anthropic_client
        .agent(CLAUDE_3_5_SONNET)
        .preamble(&preamble())
        .tool(abi_encoder::EncodeFunctionCall)
        .tool(time::GetCurrentTime)
        .tool(uniswap_tool);

    let agent = tools
        .into_iter()
        .fold(agent_builder, |agent, tool| {
            agent.rmcp_tool(tool, client.clone())
        })
        .build();

    Ok(Arc::new(agent))
}

// For TUI
pub async fn setup_agent_and_handle_messages(
    receiver: mpsc::Receiver<String>,
    sender: mpsc::Sender<AgentMessage>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let anthropic_api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            let _ = sender.send(AgentMessage::MissingApiKey).await;
            // Wait indefinitely instead of returning an error - the popup will handle this
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    };

    let anthropic_client = rig::providers::anthropic::Client::new(&anthropic_api_key);

    // Connect to MCP server with retry logic
    let client = {
        let mut attempt = 1;
        let max_attempts = 12; // About 1 minute of retries
        let mut delay = std::time::Duration::from_millis(500);

        loop {
            let _ = sender
                .send(AgentMessage::McpConnecting(format!(
                    "Connecting to MCP server (attempt {attempt}/{max_attempts})"
                )))
                .await;

            let transport = StreamableHttpClientTransport::from_uri("http://127.0.0.1:3000");
            let client_info = ClientInfo {
                protocol_version: Default::default(),
                capabilities: ClientCapabilities::default(),
                client_info: Implementation::default(),
            };

            match client_info.serve(transport).await {
                Ok(client) => {
                    let _ = sender.send(AgentMessage::McpConnected).await;
                    break client;
                }
                Err(e) => {
                    if attempt >= max_attempts {
                        let _ = sender.send(AgentMessage::Error(
                            format!("Failed to connect to MCP server after {max_attempts} attempts: {e}. Please make sure it's running at http://127.0.0.1:3000")
                        )).await;
                        return Err(e.into());
                    }

                    let _ = sender
                        .send(AgentMessage::McpConnecting(format!(
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
    };

    // ingest uniswap docs with progress reporting (or skip if --no-docs flag is set)
    let uniswap_docs_rag_tool = if skip_docs {
        // Send loading complete immediately if skipping docs
        let _ = loading_sender.send(LoadingProgress::Complete).await;
        match docs::SearchUniswapDocs::new_empty().await {
            Ok(tool) => tool,
            Err(e) => {
                let _ = sender
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
                    let _ = sender
                        .send(AgentMessage::Error(format!(
                            "Failed to load Uniswap documentation: {e}"
                        )))
                        .await;
                    return Err(e);
                }
            };
        docs::SearchUniswapDocs::new(document_store)
    };

    let tools: Vec<RmcpTool> = client.list_tools(Default::default()).await?.tools;

    let agent_builder = anthropic_client
        .agent(CLAUDE_3_5_SONNET)
        .preamble(&preamble())
        .tool(abi_encoder::EncodeFunctionCall)
        .tool(time::GetCurrentTime)
        .tool(uniswap_docs_rag_tool);

    let agent = tools
        .into_iter()
        .fold(agent_builder, |agent, tool| {
            agent.rmcp_tool(tool, client.clone())
        })
        .build();

    let agent = Arc::new(agent);

    // Handle messages - client stays alive for entire duration
    handle_agent_messages(agent, receiver, sender, interrupt_receiver).await;

    Ok(())
}

/// Enables TUI message handling.
pub async fn handle_agent_messages(
    agent: Arc<Agent<CompletionModel>>,
    mut receiver: mpsc::Receiver<String>,
    sender: mpsc::Sender<AgentMessage>,
    mut interrupt_receiver: mpsc::Receiver<()>,
) {
    let mut chat_history = Vec::new();

    while let Some(input) = receiver.recv().await {
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
                                    let _ = sender.send(AgentMessage::ToolCall { name, args }).await;
                                }
                            } else if text.starts_with("[[TOOL_RESULT:") && text.contains("]]") {
                                let marker_end = text.rfind("]]").unwrap_or(text.len());
                                let result = &text[14..marker_end];
                                let _ = sender.send(AgentMessage::System(result.to_string())).await;
                            } else if text.starts_with("[[TOOL_ERROR:") && text.contains("]]") {
                                let marker_end = text.rfind("]]").unwrap_or(text.len());
                                let error = &text[13..marker_end];
                                let _ = sender
                                    .send(AgentMessage::Error(format!("error: {error}")))
                                    .await;
                            } else {
                                response.push_str(&text);
                                let _ = sender.send(AgentMessage::StreamingText(text)).await;
                            }
                        }
                        Some(Err(err)) => {
                            let _ = sender.send(AgentMessage::Error(err.to_string())).await;
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
                    let _ = sender.send(AgentMessage::Interrupted).await;
                    break;
                }
            }
        }

        if !interrupted {
            chat_history.push(Message::user(input));
            chat_history.push(Message::assistant(response));
            let _ = sender.send(AgentMessage::Complete).await;
        } else {
            // Don't add to chat history if interrupted
            // Just add the user input since the response was incomplete
            chat_history.push(Message::user(input));
        }
    }
}
