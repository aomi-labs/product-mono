use std::{sync::Arc, time::Duration};

use aomi_rag::DocumentStore;
use eyre::Result;
use futures::StreamExt;
use rig::{
    agent::Agent, message::Message, prelude::*, providers::anthropic::completion::CompletionModel,
};
use tokio::sync::{Mutex, mpsc};

use crate::{
    abi_encoder,
    accounts::generate_account_context,
    completion::{StreamingError, stream_completion},
    docs::{self, LoadingProgress},
    mcp, time, wallet,
};

// Environment variables
pub static ANTHROPIC_API_KEY: std::sync::LazyLock<Result<String, std::env::VarError>> =
    std::sync::LazyLock::new(|| std::env::var("ANTHROPIC_API_KEY"));

const CLAUDE_3_5_SONNET: &str = "claude-sonnet-4-20250514";

#[derive(Debug, Clone)]
pub enum ChatCommand {
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
    agent: Arc<Agent<CompletionModel>>,
    document_store: Option<Arc<Mutex<DocumentStore>>>,
}

impl ChatApp {
    async fn init(
        skip_docs: bool,
        sender_to_ui: Option<&mpsc::Sender<ChatCommand>>,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
    ) -> Result<Self> {
        let anthropic_api_key = match ANTHROPIC_API_KEY.as_ref() {
            Ok(key) => key.clone(),
            Err(_) => {
                if let Some(sender) = sender_to_ui {
                    let _ = sender.send(ChatCommand::MissingApiKey).await;
                    loop {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                } else {
                    return Err(eyre::eyre!("ANTHROPIC_API_KEY not set"));
                }
            }
        };

        let anthropic_client = rig::providers::anthropic::Client::new(&anthropic_api_key);
        let mut agent_builder = anthropic_client
            .agent(CLAUDE_3_5_SONNET)
            .preamble(&preamble());

        // Get or initialize the global scheduler and register tools
        let scheduler = crate::ToolScheduler::get_or_init().await?;

        // Register tools in the scheduler
        scheduler.register_tool(wallet::SendTransactionToWallet)?;
        scheduler.register_tool(abi_encoder::EncodeFunctionCall)?;
        scheduler.register_tool(time::GetCurrentTime)?;

        // Also add tools to the agent builder
        agent_builder = agent_builder
            .tool(wallet::SendTransactionToWallet)
            .tool(abi_encoder::EncodeFunctionCall)
            .tool(time::GetCurrentTime);

        let document_store = if !skip_docs {
            let (uniswap_docs_tool, store) =
                Self::load_uniswap_docs(sender_to_ui, loading_sender).await?;
            agent_builder = agent_builder.tool(uniswap_docs_tool);
            Some(store)
        } else {
            None
        };

        let mcp_toolbox = match mcp::toolbox().await {
            Ok(toolbox) => toolbox,
            Err(err) => {
                if let Some(sender) = sender_to_ui {
                    let _ = sender
                        .send(ChatCommand::Error(format!(
                            "MCP connection failed: {err}. Retrying..."
                        )))
                        .await;
                    mcp::toolbox_with_retry(sender.clone()).await?
                } else {
                    return Err(err);
                }
            }
        };
        let agent = mcp_toolbox
            .tools()
            .iter()
            .fold(agent_builder, |agent, tool| {
                agent.rmcp_tool(tool.clone(), mcp_toolbox.mcp_client())
            })
            .build();

        Ok(Self {
            agent: Arc::new(agent),
            document_store,
        })
    }

    pub async fn new(skip_docs: bool) -> Result<Self> {
        Self::init(skip_docs, None, None).await
    }

    pub async fn new_with_senders(
        sender_to_ui: &mpsc::Sender<ChatCommand>,
        loading_sender: mpsc::Sender<LoadingProgress>,
        skip_docs: bool,
    ) -> Result<Self> {
        Self::init(skip_docs, Some(sender_to_ui), Some(loading_sender)).await
    }

    async fn load_uniswap_docs(
        sender_to_ui: Option<&mpsc::Sender<ChatCommand>>,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
    ) -> Result<(docs::SearchUniswapDocs, Arc<Mutex<DocumentStore>>)> {
        let document_store = match loading_sender {
            Some(sender) => docs::initialize_document_store_with_progress(Some(sender)).await,
            None => docs::initialize_document_store().await,
        };

        match document_store {
            Ok(store) => {
                let tool = docs::SearchUniswapDocs::new(store.clone());
                Ok((tool, store))
            }
            Err(e) => {
                if let Some(ui_sender) = sender_to_ui {
                    let _ = ui_sender
                        .send(ChatCommand::Error(format!(
                            "Failed to load Uniswap documentation: {e}"
                        )))
                        .await;
                }
                Err(e)
            }
        }
    }

    pub fn agent(&self) -> Arc<Agent<CompletionModel>> {
        self.agent.clone()
    }

    pub fn document_store(&self) -> Option<Arc<Mutex<DocumentStore>>> {
        self.document_store.clone()
    }

    async fn test_model_connection(&self, agent: &Arc<Agent<CompletionModel>>) -> Result<()> {
        use rig::completion::Prompt;

        let test_prompt = "Say 'Connection test successful' and nothing else.";

        match agent.prompt(test_prompt).await {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn ensure_connection_with_retries(
        &self,
        sender_to_ui: &mpsc::Sender<ChatCommand>,
    ) -> Result<()> {
        let mut delay = Duration::from_millis(500);

        for attempt in 1..=3 {
            if attempt == 1 {
                sender_to_ui
                    .send(ChatCommand::BackendConnecting(
                        "Testing connection to Anthropic API...".into(),
                    ))
                    .await
                    .ok();
            }

            match self.test_model_connection(&self.agent).await {
                Ok(()) => {
                    let _ = tokio::join!(
                        sender_to_ui.send(ChatCommand::System(
                            "✓ Anthropic API connection successful".into()
                        )),
                        sender_to_ui.send(ChatCommand::BackendConnected)
                    );
                    return Ok(());
                }
                Err(e) if attempt == 3 => {
                    sender_to_ui.send(ChatCommand::Error(format!("Failed to connect to Anthropic API after 3 attempts: {e}. Please check your API key and connection."))).await.ok();
                    return Err(e);
                }
                Err(_) => {
                    sender_to_ui
                        .send(ChatCommand::BackendConnecting(format!(
                            "Connection failed, retrying in {:.1}s...",
                            delay.as_secs_f32()
                        )))
                        .await
                        .ok();
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(5));
                }
            }
        }
        unreachable!()
    }

    pub async fn process_message(
        &self,
        history: &mut Vec<Message>,
        input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let agent = self.agent.clone();
        let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init().await?;
        let handler = scheduler.get_handler();
        let mut stream = stream_completion(agent, handler, &input, history.clone()).await;
        let mut response = String::new();

        let mut interrupted = false;
        loop {
            tokio::select! {
                content = stream.next() => {
                    match content {
                        Some(Ok(command)) => {
                            if let ChatCommand::StreamingText(text) = &command {
                                response.push_str(text);
                            }
                            let _ = sender_to_ui.send(command).await;
                        },
                        Some(Err(err)) => {
                            let is_completion_error = matches!(err, StreamingError::Completion(_));
                            let message = err.to_string();
                            let _ = sender_to_ui.send(ChatCommand::Error(message)).await;
                            if is_completion_error {
                                let _ = self.ensure_connection_with_retries(sender_to_ui).await;
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }
                _ = interrupt_receiver.recv() => {
                    interrupted = true;
                    let _ = sender_to_ui.send(ChatCommand::Interrupted).await;
                    break;
                }
            }
        }

        let user_message = Message::user(input.clone());
        history.push(user_message);

        if !interrupted {
            history.push(Message::assistant(response));
            let _ = sender_to_ui.send(ChatCommand::Complete).await;
        }

        Ok(())
    }
}

pub async fn setup_agent() -> Result<Arc<Agent<CompletionModel>>> {
    let app = ChatApp::new(false).await?;
    let agent = app.agent();

    match app.test_model_connection(&agent).await {
        Ok(()) => {
            println!("✓ Anthropic API connection successful");
            Ok(agent)
        }
        Err(err) => {
            eprintln!("✗ Anthropic API connection failed: {err}");
            Err(err)
        }
    }
}

pub async fn setup_agent_and_handle_messages(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<ChatCommand>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let app = Arc::new(ChatApp::new_with_senders(&sender_to_ui, loading_sender, skip_docs).await?);
    let mut agent_history: Vec<Message> = Vec::new();
    app.ensure_connection_with_retries(&sender_to_ui).await?;

    let mut receiver_from_ui = receiver_from_ui;
    let mut interrupt_receiver = interrupt_receiver;

    while let Some(input) = receiver_from_ui.recv().await {
        app.process_message(
            &mut agent_history,
            input,
            &sender_to_ui,
            &mut interrupt_receiver,
        )
        .await?;
    }

    Ok(())
}
