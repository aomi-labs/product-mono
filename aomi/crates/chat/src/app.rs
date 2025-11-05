use std::{sync::Arc, time::Duration};

use aomi_mcp::client::{self as mcp};
use aomi_rag::DocumentStore;
use aomi_tools::{
    ToolResultStream, ToolScheduler, abi_encoder, account, brave_search, cast, db_tools, time,
    wallet,
};
use eyre::Result;
use futures::StreamExt;
use rig::{
    agent::{Agent, AgentBuilder},
    message::Message,
    prelude::*,
    providers::anthropic::completion::CompletionModel,
    tool::Tool,
};
use tokio::sync::{Mutex, mpsc};

use crate::{
    completion::{StreamingError, stream_completion},
    connections::{ensure_connection_with_retries, toolbox_with_retry},
    generate_account_context,
};

// Type alias for ChatCommand with our specific ToolResultStream type
pub type ChatCommand = crate::ChatCommand<ToolResultStream>;

// Environment variables
pub static ANTHROPIC_API_KEY: std::sync::LazyLock<Result<String, std::env::VarError>> =
    std::sync::LazyLock::new(|| std::env::var("ANTHROPIC_API_KEY"));

const CLAUDE_3_5_SONNET: &str = "claude-sonnet-4-20250514";

// Loading progress enum for docs
#[derive(Debug, Clone)]
pub enum LoadingProgress {
    Message(String),
    Complete,
}

fn preamble() -> String {
    format!(
        "{}\n\n{}aa",
        crate::prompts::PREAMBLE,
        generate_account_context()
    )
}

pub struct ChatAppBuilder {
    agent_builder: Option<AgentBuilder<CompletionModel>>,
    scheduler: Arc<ToolScheduler>,
    document_store: Option<Arc<Mutex<DocumentStore>>>,
}

impl ChatAppBuilder {
    pub async fn new(preamble: &str) -> Result<Self> {
        let anthropic_api_key = match ANTHROPIC_API_KEY.as_ref() {
            Ok(key) => key.clone(),
            Err(_) => return Err(eyre::eyre!("ANTHROPIC_API_KEY not set")),
        };

        let anthropic_client = rig::providers::anthropic::Client::new(&anthropic_api_key);
        let agent_builder = anthropic_client.agent(CLAUDE_3_5_SONNET).preamble(preamble);

        // Get or initialize the global scheduler and register core tools
        let scheduler = ToolScheduler::get_or_init().await?;
        scheduler.register_tool(wallet::SendTransactionToWallet)?;
        scheduler.register_tool(abi_encoder::EncodeFunctionCall)?;
        scheduler.register_tool(time::GetCurrentTime)?;

        // Add core tools to agent builder
        let agent_builder = agent_builder
            .tool(wallet::SendTransactionToWallet)
            .tool(abi_encoder::EncodeFunctionCall)
            .tool(time::GetCurrentTime);

        Ok(Self {
            agent_builder: Some(agent_builder),
            scheduler,
            document_store: None,
        })
    }

    pub async fn new_with_api_key_handling(
        preamble: &str,
        sender_to_ui: Option<&mpsc::Sender<ChatCommand>>,
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
        let mut agent_builder = anthropic_client.agent(CLAUDE_3_5_SONNET).preamble(preamble);

        // Get or initialize the global scheduler and register core tools
        let scheduler = ToolScheduler::get_or_init().await?;

        // Register tools in the scheduler
        scheduler.register_tool(brave_search::BraveSearch)?;
        scheduler.register_tool(wallet::SendTransactionToWallet)?;
        scheduler.register_tool(abi_encoder::EncodeFunctionCall)?;
        scheduler.register_tool(cast::CallViewFunction)?;
        scheduler.register_tool(cast::SimulateContractCall)?;

        scheduler.register_tool(time::GetCurrentTime)?;
        scheduler.register_tool(db_tools::GetContractABI)?;
        scheduler.register_tool(db_tools::GetContractSourceCode)?;

        scheduler.register_tool(account::GetAccountInfo)?;
        scheduler.register_tool(account::GetAccountTransactionHistory)?;

        // Also add tools to the agent builder
        agent_builder = agent_builder
            .tool(brave_search::BraveSearch)
            .tool(wallet::SendTransactionToWallet)
            .tool(abi_encoder::EncodeFunctionCall)
            .tool(cast::CallViewFunction)
            .tool(cast::SimulateContractCall)
            .tool(time::GetCurrentTime)
            .tool(db_tools::GetContractABI)
            .tool(db_tools::GetContractSourceCode)
            .tool(account::GetAccountInfo)
            .tool(account::GetAccountTransactionHistory);

        Ok(Self {
            agent_builder: Some(agent_builder),
            scheduler,
            document_store: None,
        })
    }

    pub fn add_tool<T>(&mut self, tool: T) -> Result<&mut Self>
    where
        T: Tool + Clone + Send + Sync + 'static,
        T::Args: Send + Sync + Clone,
        T::Output: Send + Sync + Clone,
        T::Error: Send + Sync,
    {
        // Register tool in the scheduler
        self.scheduler.register_tool(tool.clone())?;

        // Add tool to the agent builder
        if let Some(builder) = self.agent_builder.take() {
            self.agent_builder = Some(builder.tool(tool));
        }

        Ok(self)
    }

    pub async fn add_docs_tool(
        &mut self,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
        sender_to_ui: Option<&mpsc::Sender<ChatCommand>>,
    ) -> Result<&mut Self> {
        use crate::connections::init_document_store;
        let docs_tool = match init_document_store(loading_sender).await {
            Ok(store) => store,
            Err(e) => {
                if let Some(sender) = sender_to_ui {
                    let _ = sender
                        .send(ChatCommand::Error(format!(
                            "Failed to load Uniswap documentation: {e}"
                        )))
                        .await;
                }
                return Err(e);
            }
        };

        if let Some(builder) = self.agent_builder.take() {
            self.agent_builder = Some(builder.tool(docs_tool.clone()));
        }
        self.document_store = Some(docs_tool.get_store());

        Ok(self)
    }

    pub async fn build(
        self,
        skip_mcp: bool,
        sender_to_ui: Option<&mpsc::Sender<ChatCommand>>,
    ) -> Result<ChatApp> {
        let agent_builder = self
            .agent_builder
            .ok_or_else(|| eyre::eyre!("ChatAppBuilder has no agent builder"))?;

        let agent = if skip_mcp {
            // Skip MCP initialization for testing
            if let Some(sender) = sender_to_ui {
                let _ = sender
                    .send(ChatCommand::System(
                        "⚠️ Running without MCP server (testing mode)".to_string(),
                    ))
                    .await;
            }
            agent_builder.build()
        } else {
            let mcp_toolbox = match mcp::toolbox().await {
                Ok(toolbox) => toolbox,
                Err(err) => {
                    if let Some(sender) = sender_to_ui {
                        let _ = sender
                            .send(ChatCommand::Error(format!(
                                "MCP connection failed: {err}. Retrying..."
                            )))
                            .await;
                        toolbox_with_retry(sender.clone()).await?
                    } else {
                        return Err(err);
                    }
                }
            };
            mcp_toolbox
                .tools()
                .iter()
                .fold(agent_builder, |agent, tool| {
                    agent.rmcp_tool(tool.clone(), mcp_toolbox.mcp_client())
                })
                .build()
        };

        Ok(ChatApp {
            agent: Arc::new(agent),
            document_store: self.document_store,
        })
    }
}

pub struct ChatApp {
    agent: Arc<Agent<CompletionModel>>,
    document_store: Option<Arc<Mutex<DocumentStore>>>,
}

impl ChatApp {
    pub async fn new() -> Result<Self> {
        Self::init_internal(true, true, None, None).await
    }

    pub async fn new_with_options(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        Self::init_internal(skip_docs, skip_mcp, None, None).await
    }

    pub async fn new_with_senders(
        sender_to_ui: &mpsc::Sender<ChatCommand>,
        loading_sender: mpsc::Sender<LoadingProgress>,
        skip_docs: bool,
    ) -> Result<Self> {
        Self::init_internal(skip_docs, false, Some(sender_to_ui), Some(loading_sender)).await
    }

    async fn init_internal(
        skip_docs: bool,
        skip_mcp: bool,
        sender_to_ui: Option<&mpsc::Sender<ChatCommand>>,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
    ) -> Result<Self> {
        let mut builder =
            ChatAppBuilder::new_with_api_key_handling(&preamble(), sender_to_ui).await?;

        // Add docs tool if not skipped
        if !skip_docs {
            builder.add_docs_tool(loading_sender, sender_to_ui).await?;
        }

        // Build the final ChatApp
        builder.build(skip_mcp, sender_to_ui).await
    }

    pub fn agent(&self) -> Arc<Agent<CompletionModel>> {
        self.agent.clone()
    }

    pub fn document_store(&self) -> Option<Arc<Mutex<DocumentStore>>> {
        self.document_store.clone()
    }

    pub async fn process_message(
        &self,
        history: &mut Vec<Message>,
        input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let agent = self.agent.clone();
        let scheduler = ToolScheduler::get_or_init().await?;
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
                                let _ = ensure_connection_with_retries(&self.agent, sender_to_ui).await;
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

pub async fn run_chat(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<ChatCommand>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let app = Arc::new(ChatApp::new_with_senders(&sender_to_ui, loading_sender, skip_docs).await?);
    let mut agent_history: Vec<Message> = Vec::new();
    ensure_connection_with_retries(&app.agent, &sender_to_ui).await?;

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
