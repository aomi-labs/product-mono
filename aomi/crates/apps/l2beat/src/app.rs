use aomi_core::{
    AomiModel, CoreApp, CoreAppBuilder, Selection,
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
    prompts::{PreambleBuilder, PromptSection},
};
use async_trait::async_trait;
use eyre::Result;

use crate::l2b_tools::{
    AnalyzeAbiToCallHandler, AnalyzeEventsToEventHandler, AnalyzeLayoutToStorageHandler,
    ExecuteHandler, GetSavedHandlers,
};

// Type alias for L2BeatCommand with our specific ToolReturn type
pub type L2BeatCommand = CoreCommand;

const L2BEAT_ROLE: &str = "You are an AI assistant specialized in L2Beat protocol analysis and smart contract discovery. You have access to tools for analyzing ABIs, events, storage layouts, and executing handlers to extract data from Ethereum smart contracts.";

const L2BEAT_CAPABILITIES: &[&str] = &[
    "Analyzing smart contract ABIs to generate call handlers",
    "Analyzing smart contract events to generate event handlers",
    "Analyzing storage layouts to generate storage handlers",
    "Executing generated handlers to extract contract data",
    "Working with L2Beat discovery and monitoring tools",
];

const L2BEAT_WORKFLOW: &[&str] = &[
    "Identify the contract(s) to analyze based on user request",
    "Use the appropriate analysis tool (ABI, events, or storage) to generate handlers",
    "Execute handlers to extract and present the data to the user",
    "Explain findings clearly, highlighting important protocol details",
];

const L2BEAT_CONSTRAINTS: &[&str] = &[
    "Always verify contract addresses before analysis",
    "Present extracted data in a clear, structured format",
    "Explain any errors from handler execution honestly",
    "When analyzing L2 protocols, note the chain and any L1/L2 relationships",
];

fn l2beat_preamble() -> String {
    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(L2BEAT_ROLE))
        .section(
            PromptSection::titled("Capabilities").bullet_list(L2BEAT_CAPABILITIES.iter().copied()),
        )
        .section(PromptSection::titled("Workflow").ordered_list(L2BEAT_WORKFLOW.iter().copied()))
        .section(
            PromptSection::titled("Constraints").bullet_list(L2BEAT_CONSTRAINTS.iter().copied()),
        )
        .section(
            PromptSection::titled("Account Context")
                .paragraph(aomi_core::generate_account_context()),
        )
        .build()
}

pub struct L2BeatApp {
    chat_app: CoreApp,
}

impl L2BeatApp {
    pub async fn default() -> Result<Self> {
        Self::new(true, true).await
    }

    pub async fn new(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        Self::new_with_models(skip_docs, skip_mcp, Selection {
            rig: AomiModel::ClaudeSonnet4,
            baml: AomiModel::ClaudeOpus4,
        })
        .await
    }

    pub async fn new_with_models(
        skip_docs: bool,
        skip_mcp: bool,
        selection: Selection,
    ) -> Result<Self> {
        let mut builder =
            CoreAppBuilder::new_with_model(&l2beat_preamble(), selection.rig, false, None).await?;
        let _baml_client = aomi_baml::BamlClient::new(selection.baml)
            .map_err(|err| eyre::eyre!(err))?;

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
        let chat_app = builder.build(skip_mcp, None, selection.rig).await?;

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
