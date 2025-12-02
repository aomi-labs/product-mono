use std::sync::Arc;

use aomi_chat::{ChatApp, ChatAppBuilder, app::ChatCommand, app::LoadingProgress};
use eyre::Result;
use rig::{agent::Agent, message::Message, providers::anthropic::completion::CompletionModel};
use tokio::sync::{Mutex, mpsc};

// Type alias for ForgeCommand with our specific ToolResultStream type
pub type ForgeCommand = ChatCommand;

fn forge_preamble() -> String {
    format!(
        "You are an AI assistant specialized in Ethereum smart contract development and deployment using Foundry/Forge.

        Your role is to help users deploy and interact with smart contracts by:
        - Understanding user intents and translating them into blockchain operations
        - Researching contracts and protocols to gather necessary information (ABIs, source code, addresses)
        - Planning and structuring operations for safe execution
        - Simulating transactions before they are broadcast to ensure correctness

        You have access to tools that allow you to:
        - Fetch contract ABIs and source code from various sources
        - Build Forge scripts from structured operations
        - Compile and simulate scripts in a safe environment
        - Generate broadcastable transactions for user review

        General workflow:
        1. Understand the user's intent and identify what contracts/operations are needed
        2. Gather necessary contract information (ABIs, addresses, source code as needed)
        3. Structure operations with proper parameters and sequencing
        4. Build and simulate the script to verify correctness
        5. Present results and transactions to the user for approval

        Key principles:
        - Always simulate before suggesting real transactions
        - Use ABIs to ensure type-safe contract interactions
        - Always provide full contract source when intent involves deployment. (Use known interfaces)
        - Validate addresses and contract existence when possible
        - Break down complex operations into clear, verifiable steps
        - Provide context and explanations for your proposed actions

        {}",
        aomi_chat::generate_account_context()
    )
}

pub struct ForgeApp {
    chat_app: ChatApp,
}

impl ForgeApp {
    pub async fn new() -> Result<Self> {
        Self::init_internal(true, true, None, None).await
    }

    pub async fn new_with_options(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        Self::init_internal(skip_docs, skip_mcp, None, None).await
    }

    pub async fn new_with_senders(
        sender_to_ui: &mpsc::Sender<ForgeCommand>,
        loading_sender: mpsc::Sender<LoadingProgress>,
        skip_docs: bool,
    ) -> Result<Self> {
        Self::init_internal(skip_docs, false, Some(sender_to_ui), Some(loading_sender)).await
    }

    async fn init_internal(
        skip_docs: bool,
        skip_mcp: bool,
        sender_to_ui: Option<&mpsc::Sender<ForgeCommand>>,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
    ) -> Result<Self> {
        let mut builder =
            ChatAppBuilder::new_with_model_connection(&forge_preamble(), sender_to_ui, false)
                .await?;

        // Add Forge-specific tools
        builder.add_tool(aomi_tools::ForgeScriptBuilder)?;

        // Add docs tool if not skipped
        if !skip_docs {
            builder.add_docs_tool(loading_sender, sender_to_ui).await?;
        }

        // Build the final ForgeApp
        let chat_app = builder.build(skip_mcp, sender_to_ui).await?;

        Ok(Self { chat_app })
    }

    pub fn agent(&self) -> Arc<Agent<CompletionModel>> {
        self.chat_app.agent()
    }

    pub fn chat_app(&self) -> &ChatApp {
        &self.chat_app
    }

    /// Consume ForgeApp and return the inner ChatApp for use as BackendwithTool
    pub fn into_chat_app(self) -> ChatApp {
        self.chat_app
    }

    pub fn document_store(&self) -> Option<Arc<Mutex<aomi_rag::DocumentStore>>> {
        self.chat_app.document_store()
    }

    pub async fn process_message(
        &self,
        history: &mut Vec<Message>,
        input: String,
        sender_to_ui: &mpsc::Sender<ForgeCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        tracing::debug!("[forge] process message: {}", input);
        // Delegate to the inner ChatApp
        self.chat_app
            .process_message(history, input, sender_to_ui, interrupt_receiver)
            .await
    }
}

pub async fn run_forge_chat(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<ForgeCommand>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let app = Arc::new(ForgeApp::new_with_senders(&sender_to_ui, loading_sender, skip_docs).await?);
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
