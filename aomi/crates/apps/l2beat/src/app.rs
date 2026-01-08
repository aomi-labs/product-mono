use std::sync::Arc;

use aomi_chat::{CoreApp, CoreAppBuilder, SystemEventQueue, app::CoreCommand};
use aomi_tools::ToolScheduler;
use eyre::Result;
use rig::{agent::Agent, message::Message, providers::anthropic::completion::CompletionModel};
use tokio::sync::{Mutex, mpsc};

use crate::l2b_tools::{
    AnalyzeAbiToCallHandler, AnalyzeEventsToEventHandler, AnalyzeLayoutToStorageHandler,
    ExecuteHandler, GetSavedHandlers,
};

// Type alias for L2BeatCommand with our specific ToolStreamream type
pub type L2BeatCommand = CoreCommand;

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
    chat_app: CoreApp,
}

impl L2BeatApp {
    pub async fn default() -> Result<Self> {
        Self::new(true, true, None).await
    }


    async fn new(
        skip_docs: bool,
        skip_mcp: bool,
        system_events: Option<&SystemEventQueue>,
    ) -> Result<Self> {
        let mut builder = CoreAppBuilder::new_with_connection(
            &l2beat_preamble(),
            false,
            system_events,
        )
        .await?;

        // Add L2Beat-specific tools
        builder.add_tool(AnalyzeAbiToCallHandler)?;
        builder.add_tool(AnalyzeEventsToEventHandler)?;
        builder.add_tool(AnalyzeLayoutToStorageHandler)?;
        builder.add_tool(GetSavedHandlers)?;
        builder.add_tool(ExecuteHandler)?;

        // Add docs tool if not skipped
        if !skip_docs {
            builder.add_docs_tool().await?;
        }

        // Build the final L2BeatApp
        let chat_app = builder.build(skip_mcp, system_events).await?;

        Ok(Self { chat_app })
    }

    pub fn agent(&self) -> Arc<Agent<CompletionModel>> {
        self.chat_app.agent()
    }

    pub fn chat_app(&self) -> &CoreApp {
        &self.chat_app
    }

    pub fn document_store(&self) -> Option<Arc<Mutex<aomi_rag::DocumentStore>>> {
        self.chat_app.document_store()
    }

    pub async fn process_message(
        &self,
        history: &mut Vec<Message>,
        system_events: &SystemEventQueue,
        handler: Arc<Mutex<aomi_tools::scheduler::ToolHandler>>,
        input: String,
        sender_to_ui: &mpsc::Sender<L2BeatCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        tracing::debug!("[l2b] process message: {}", input);
        // Delegate to the inner ChatApp
        self.chat_app
            .process_message(
                history,
                input,
                sender_to_ui,
                system_events,
                handler,
                interrupt_receiver,
            )
            .await
    }
}
