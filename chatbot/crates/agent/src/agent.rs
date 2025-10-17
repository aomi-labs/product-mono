// Environment variables
static ANTHROPIC_API_KEY: std::sync::LazyLock<Result<String, std::env::VarError>> =
    std::sync::LazyLock::new(|| std::env::var("ANTHROPIC_API_KEY"));
static MCP_SERVER_HOST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("MCP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
    // local connection only
});
static MCP_SERVER_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("MCP_SERVER_PORT").unwrap_or_else(|_| "5000".to_string())
});

use eyre::Result;
use futures::StreamExt;
use rig::{
    agent::Agent,
    message::{Message, Text},
    prelude::*,
    providers::anthropic::completion::CompletionModel,
};
use rmcp::{
    model::{ClientCapabilities, ClientInfo, Implementation, Tool as RmcpTool},
    service::{RoleClient, RunningService},
    transport::StreamableHttpClientTransport,
    ServiceExt,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::docs::{self, LoadingProgress};
use crate::helpers::multi_turn_prompt;
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

pub struct ChatApp {
    anthropic_client: rig::providers::anthropic::Client,
    rmcp_client: RunningService<RoleClient, ClientInfo>,
    uniswap_docs_tool: docs::SearchUniswapDocs,
}

impl ChatApp {
    pub async fn new_for_repl(skip_docs: bool) -> Result<Self> {
        let anthropic_api_key = ANTHROPIC_API_KEY
            .as_ref()
            .map_err(|_| eyre::eyre!("ANTHROPIC_API_KEY not set"))?
            .clone();

        let anthropic_client = rig::providers::anthropic::Client::new(&anthropic_api_key);
        let rmcp_client = Self::connect_rmcp(None).await?;
        let uniswap_docs_tool = Self::load_uniswap_docs(skip_docs, None, None).await?;

        let _server_info = rmcp_client.peer_info();

        Ok(Self {
            anthropic_client,
            rmcp_client,
            uniswap_docs_tool,
        })
    }

    pub async fn new_with_ui(
        sender_to_ui: &mpsc::Sender<AgentMessage>,
        loading_sender: mpsc::Sender<LoadingProgress>,
        skip_docs: bool,
    ) -> Result<Self> {
        let anthropic_api_key = match ANTHROPIC_API_KEY.as_ref() {
            Ok(key) => key.clone(),
            Err(_) => {
                let _ = sender_to_ui.send(AgentMessage::MissingApiKey).await;
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };

        let anthropic_client = rig::providers::anthropic::Client::new(&anthropic_api_key);
        let rmcp_client = Self::connect_rmcp(Some(sender_to_ui)).await?;
        let uniswap_docs_tool =
            Self::load_uniswap_docs(skip_docs, Some(sender_to_ui), Some(loading_sender)).await?;

        Ok(Self {
            anthropic_client,
            rmcp_client,
            uniswap_docs_tool,
        })
    }

    async fn connect_rmcp(
        sender_to_ui: Option<&mpsc::Sender<AgentMessage>>,
    ) -> Result<RunningService<RoleClient, ClientInfo>> {
        let mcp_host = &*MCP_SERVER_HOST;
        let mcp_port = &*MCP_SERVER_PORT;
        let mcp_url = format!("http://{}:{}", mcp_host, mcp_port);

        let max_attempts = if sender_to_ui.is_some() { 12 } else { 1 };
        let mut attempt = 1;
        let mut delay = Duration::from_millis(500);

        loop {
            if let Some(sender) = sender_to_ui {
                let _ = sender
                    .send(AgentMessage::BackendConnecting(format!(
                        "Connecting to MCP server (attempt {attempt}/{max_attempts})"
                    )))
                    .await;
            }

            let transport = StreamableHttpClientTransport::from_uri(mcp_url.clone());
            let mcp_client = ClientInfo {
                protocol_version: Default::default(),
                capabilities: ClientCapabilities::default(),
                client_info: Implementation::default(),
            };

            match mcp_client.serve(transport).await {
                Ok(client) => {
                    if let Some(sender) = sender_to_ui {
                        let _ = sender
                            .send(AgentMessage::System(
                                "✓ MCP server connection successful".to_string(),
                            ))
                            .await;
                    }
                    return Ok(client);
                }
                Err(e) => {
                    if attempt >= max_attempts {
                        if let Some(sender) = sender_to_ui {
                            let _ = sender
                                .send(AgentMessage::Error(format!(
                                    "Failed to connect to MCP server after {max_attempts} attempts: {e}. Please make sure it's running at {mcp_url}"
                                )))
                                .await;
                        }
                        return Err(e.into());
                    }

                    if let Some(sender) = sender_to_ui {
                        let _ = sender
                            .send(AgentMessage::BackendConnecting(format!(
                                "Connection failed, retrying in {:.1}s...",
                                delay.as_secs_f32()
                            )))
                            .await;
                        tokio::time::sleep(delay).await;
                        delay = std::cmp::min(delay * 2, Duration::from_secs(5));
                    }
                    attempt += 1;
                }
            }
        }
    }

    async fn load_uniswap_docs(
        skip_docs: bool,
        sender_to_ui: Option<&mpsc::Sender<AgentMessage>>,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
    ) -> Result<docs::SearchUniswapDocs> {
        if skip_docs {
            if let Some(sender) = loading_sender {
                let _ = sender.send(LoadingProgress::Complete).await;
            }

            match docs::SearchUniswapDocs::new_empty().await {
                Ok(tool) => Ok(tool),
                Err(e) => {
                    if let Some(ui_sender) = sender_to_ui {
                        let _ = ui_sender
                            .send(AgentMessage::Error(format!(
                                "Failed to create empty document store: {e}"
                            )))
                            .await;
                    }
                    Err(e)
                }
            }
        } else {
            let document_store = match loading_sender {
                Some(sender) => docs::initialize_document_store_with_progress(Some(sender)).await,
                None => docs::initialize_document_store().await,
            };

            match document_store {
                Ok(store) => Ok(docs::SearchUniswapDocs::new(store)),
                Err(e) => {
                    if let Some(ui_sender) = sender_to_ui {
                        let _ = ui_sender
                            .send(AgentMessage::Error(format!(
                                "Failed to load Uniswap documentation: {e}"
                            )))
                            .await;
                    }
                    Err(e)
                }
            }
        }
    }

    pub async fn build_agent(&self) -> Result<Arc<Agent<CompletionModel>>> {
        let tools: Vec<RmcpTool> = self.rmcp_client.list_tools(Default::default()).await?.tools;

        let agent_builder = self
            .anthropic_client
            .agent(CLAUDE_3_5_SONNET)
            .preamble(&preamble())
            .tool(wallet::SendTransactionToWallet)
            .tool(abi_encoder::EncodeFunctionCall)
            .tool(time::GetCurrentTime)
            .tool(self.uniswap_docs_tool.clone());

        let agent = tools
            .into_iter()
            .fold(agent_builder, |agent, tool| {
                agent.rmcp_tool(tool, self.rmcp_client.peer().clone())
            })
            .build();

        Ok(Arc::new(agent))
    }

    async fn test_model_connection(&self, agent: &Arc<Agent<CompletionModel>>) -> Result<()> {
        use rig::completion::Prompt;

        let test_prompt = "Say 'Connection test successful' and nothing else.";

        match agent.prompt(test_prompt).await {
            Ok(_response) => Ok(()),
            Err(e) => Err(eyre::eyre!(e.to_string())),
        }
    }

    pub async fn ensure_model_connection_with_retries(
        &self,
        agent: &Arc<Agent<CompletionModel>>,
        sender_to_ui: &mpsc::Sender<AgentMessage>,
    ) -> Result<()> {
        let max_attempts = 3;
        let mut attempt = 1;
        let mut delay = Duration::from_millis(500);

        loop {
            let _ = sender_to_ui
                .send(AgentMessage::BackendConnecting(
                    "Testing connection to Anthropic API...".to_string(),
                ))
                .await;

            match self.test_model_connection(agent).await {
                Ok(()) => {
                    let _ = sender_to_ui
                        .send(AgentMessage::System(
                            "✓ Anthropic API connection successful".to_string(),
                        ))
                        .await;
                    let _ = sender_to_ui.send(AgentMessage::BackendConnected).await;
                    println!("✓ Anthropic API connection successful");
                    return Ok(());
                }
                Err(e) => {
                    if attempt >= max_attempts {
                        let message = format!(
                            "Failed to connect to Anthropic API after {max_attempts} attempts: {e}. Please check your API key and connection."
                        );
                        let _ = sender_to_ui
                            .send(AgentMessage::Error(message.clone()))
                            .await;
                        eprintln!("✗ Anthropic API connection failed: {e}");
                        return Err(e);
                    }

                    let _ = sender_to_ui
                        .send(AgentMessage::BackendConnecting(format!(
                            "Connection failed, retrying in {:.1}s...",
                            delay.as_secs_f32()
                        )))
                        .await;

                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, Duration::from_secs(5));
                    attempt += 1;
                }
            }
        }
    }

    pub async fn run(
        self: Arc<Self>,
        receiver_from_ui: mpsc::Receiver<String>,
        sender_to_ui: mpsc::Sender<AgentMessage>,
        interrupt_receiver: mpsc::Receiver<()>,
    ) -> Result<()> {
        let agent = self.build_agent().await?;
        self.ensure_model_connection_with_retries(&agent, &sender_to_ui)
            .await?;
        self.handle_agent_messages(agent, receiver_from_ui, sender_to_ui, interrupt_receiver)
            .await;
        Ok(())
    }

    pub async fn handle_agent_messages(
        &self,
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
                                } else {
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
}

pub async fn setup_agent() -> Result<Arc<Agent<CompletionModel>>> {
    let app = ChatApp::new_for_repl(false).await?;
    let agent = app.build_agent().await?;

    match app.test_model_connection(&agent).await {
        Ok(()) => {
            println!("✓ Anthropic API connection successful");
            Ok(agent)
        }
        Err(e) => {
            eprintln!("✗ Anthropic API connection failed: {e}");
            Err(e)
        }
    }
}

pub async fn setup_agent_and_handle_messages(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<AgentMessage>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let app = ChatApp::new_with_ui(&sender_to_ui, loading_sender, skip_docs).await?;
    let app = Arc::new(app);
    app.run(receiver_from_ui, sender_to_ui, interrupt_receiver)
        .await
}
