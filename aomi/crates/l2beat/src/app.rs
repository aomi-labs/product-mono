use std::sync::Arc;

use aomi_chat::{ChatApp, ChatAppBuilder, app::ChatCommand, app::LoadingProgress};
use eyre::Result;
use rig::{agent::Agent, message::Message, providers::anthropic::completion::CompletionModel};
use tokio::sync::{Mutex, mpsc};

use crate::l2b_tools::{
    AnalyzeAbiToCallHandler, AnalyzeEventsToEventHandler, AnalyzeLayoutToStorageHandler,
    ExecuteHandler, GetSavedHandlers,
};

// Type alias for L2BeatCommand with our specific ToolResultStream type
pub type L2BeatCommand = ChatCommand;

fn l2beat_preamble() -> String {
    format!(
        "You are an AI assistant specialized in L2Beat protocol analysis and smart contract discovery. 
        You have access to tools for analyzing ABIs, events, storage layouts, and executing handlers 
        to extract data from Ethereum smart contracts.

        Your capabilities include:
        - Analyzing smart contract ABIs to generate call handlers
        - Analyzing smart contract events to generate event handlers  
        - Analyzing storage layouts to generate storage handlers
        - Executing generated handlers to extract contract data
        - Working with L2Beat discovery and monitoring tools

        Use these tools to help users understand and analyze smart contracts on Ethereum and L2 networks.\n\n{}", 
        aomi_chat::generate_account_context()
    )
}

pub struct L2BeatApp {
    chat_app: ChatApp,
}

impl L2BeatApp {
    pub async fn new() -> Result<Self> {
        Self::init_internal(true, true, None, None).await
    }

    pub async fn new_with_options(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        Self::init_internal(skip_docs, skip_mcp, None, None).await
    }

    pub async fn new_with_senders(
        sender_to_ui: &mpsc::Sender<L2BeatCommand>,
        loading_sender: mpsc::Sender<LoadingProgress>,
        skip_docs: bool,
    ) -> Result<Self> {
        Self::init_internal(skip_docs, false, Some(sender_to_ui), Some(loading_sender)).await
    }

    async fn init_internal(
        skip_docs: bool,
        skip_mcp: bool,
        sender_to_ui: Option<&mpsc::Sender<L2BeatCommand>>,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
    ) -> Result<Self> {
        let mut builder =
            ChatAppBuilder::new_with_api_key_handling(&l2beat_preamble(), sender_to_ui).await?;

        // Add L2Beat-specific tools
        builder.add_tool(AnalyzeAbiToCallHandler)?;
        builder.add_tool(AnalyzeEventsToEventHandler)?;
        builder.add_tool(AnalyzeLayoutToStorageHandler)?;
        builder.add_tool(GetSavedHandlers)?;
        builder.add_tool(ExecuteHandler)?;

        // Add docs tool if not skipped
        if !skip_docs {
            builder.add_docs_tool(loading_sender, sender_to_ui).await?;
        }

        // Build the final L2BeatApp
        let chat_app = builder.build(skip_mcp, sender_to_ui).await?;

        Ok(Self { chat_app })
    }

    pub fn agent(&self) -> Arc<Agent<CompletionModel>> {
        self.chat_app.agent()
    }

    pub fn chat_app(&self) -> &ChatApp {
        &self.chat_app
    }

    pub fn document_store(&self) -> Option<Arc<Mutex<aomi_rag::DocumentStore>>> {
        self.chat_app.document_store()
    }

    pub async fn process_message(
        &self,
        history: &mut Vec<Message>,
        input: String,
        sender_to_ui: &mpsc::Sender<L2BeatCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        tracing::debug!("[l2b] process message: {}", input);
        // Delegate to the inner ChatApp
        self.chat_app
            .process_message(history, input, sender_to_ui, interrupt_receiver)
            .await
    }
}

pub async fn run_l2beat_chat(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<L2BeatCommand>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let app =
        Arc::new(L2BeatApp::new_with_senders(&sender_to_ui, loading_sender, skip_docs).await?);
    let mut agent_history: Vec<Message> = Vec::new();

    use aomi_chat::connections::ensure_connection_with_retries;
    ensure_connection_with_retries(&app.agent(), &sender_to_ui).await?;

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
