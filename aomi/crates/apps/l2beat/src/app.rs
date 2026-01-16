use aomi_chat::{
    CoreApp, CoreAppBuilder,
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
};
use async_trait::async_trait;
use eyre::Result;

use crate::l2b_tools::{
    AnalyzeAbiToCallHandler, AnalyzeEventsToEventHandler, AnalyzeLayoutToStorageHandler,
    ExecuteHandler, GetSavedHandlers,
};

// Type alias for L2BeatCommand with our specific ToolReturn type
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
        Self::new(true, true).await
    }

    pub async fn new(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        let mut builder = CoreAppBuilder::new(&l2beat_preamble(), false, None).await?;

        // Add L2Beat-specific tools
        // AnalyzeAbiToCallHandler NAMESPACE = "l2beat";

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
        let chat_app = builder.build(skip_mcp, None).await?;

        Ok(Self { chat_app })
    }

    pub async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        tracing::debug!("[l2b] process message: {}", input);
        self.chat_app.process_message(input, state, ctx).await
    }
}

#[async_trait]
impl AomiApp for L2BeatApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        L2BeatApp::process_message(self, input, state, ctx).await
    }

    fn tool_namespaces(&self) -> std::sync::Arc<std::collections::HashMap<String, String>> {
        self.chat_app.tool_namespaces()
    }
}
