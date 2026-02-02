use std::collections::HashMap;
use std::sync::Arc;

use aomi_baml::{AomiModel, Selection};
use aomi_mcp::client::{self as mcp};
use aomi_rag::DocumentStore;
use aomi_tools::{
    AomiTool, AomiToolWrapper, ToolScheduler, abi_encoder, account, brave_search, cast, context,
    db_tools, etherscan, wallet,
};
use async_trait::async_trait;
use eyre::Result;
use rig::{
    agent::{Agent, AgentBuilder},
    prelude::*,
    providers::anthropic::completion::CompletionModel,
};

use tokio::sync::Mutex;

use crate::{
    completion::stream_completion,
    connections::toolbox_with_retry,
    events::{SystemEvent, SystemEventQueue},
    prompts::{PromptSection, generate_account_context, preamble_builder},
};

// Re-export for backward compatibility
pub use crate::CoreCommand;
pub use crate::state::{CoreCtx, CoreState};

// Environment variables
pub static ANTHROPIC_API_KEY: std::sync::LazyLock<Result<String, std::env::VarError>> =
    std::sync::LazyLock::new(|| std::env::var("ANTHROPIC_API_KEY"));

const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";

async fn preamble() -> String {
    preamble_builder()
        .await
        .section(PromptSection::titled("Account Context").paragraph(generate_account_context()))
        .build()
}

#[derive(Clone, Copy, Debug)]
pub struct BuildOpts {
    pub no_docs: bool,
    pub skip_mcp: bool,
    pub no_tools: bool,
    pub selection: Selection,
}

impl Default for BuildOpts {
    fn default() -> Self {
        Self {
            no_docs: true,
            skip_mcp: true,
            no_tools: false,
            selection: Selection::default(),
        }
    }
}

pub struct CoreAppBuilder {
    agent_builder: Option<AgentBuilder<CompletionModel>>,
    scheduler: Arc<ToolScheduler>,
    document_store: Option<Arc<Mutex<DocumentStore>>>,
    tool_namespaces: HashMap<String, String>,
    model: AomiModel,
}

impl CoreAppBuilder {
    /// Lightweight constructor for tests that don't need a live model connection.
    /// Skips Anthropic client creation but keeps the shared ToolScheduler.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_for_tests(system_events: Option<&SystemEventQueue>) -> Result<Self> {
        let scheduler = ToolScheduler::new_for_test().await?;
        if let Some(events) = system_events {
            events.push(SystemEvent::SystemNotice(
                "⚠️ CoreAppBuilder running in test mode without model connection".to_string(),
            ));
        }

        Ok(Self {
            agent_builder: None,
            scheduler,
            document_store: None,
            tool_namespaces: HashMap::new(),
            model: AomiModel::ClaudeSonnet4,
        })
    }

    pub fn scheduler(&self) -> Arc<ToolScheduler> {
        self.scheduler.clone()
    }

    pub async fn new(
        preamble: &str,
        opts: BuildOpts,
        system_events: Option<&SystemEventQueue>,
    ) -> Result<Self> {
        let anthropic_api_key = match ANTHROPIC_API_KEY.as_ref() {
            Ok(key) => key.clone(),
            Err(_) => {
                if let Some(events) = system_events {
                    events.push(SystemEvent::SystemError("API Key missing".into()));
                }
                return Err(eyre::eyre!("ANTHROPIC_API_KEY not set"));
            }
        };

        let model_name = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| DEFAULT_ANTHROPIC_MODEL.to_string());
        if let Some(events) = system_events {
            events.push(SystemEvent::SystemNotice(format!(
                "Using Anthropic model: {model_name}"
            )));
        }

        let anthropic_client = rig::providers::anthropic::Client::new(&anthropic_api_key);
        let agent_builder = anthropic_client
            .agent(opts.selection.rig.rig_id())
            .preamble(preamble);

        // Get or initialize the global scheduler and register core tools
        let scheduler = ToolScheduler::get_or_init().await?;

        if !opts.no_tools {
            let mut builder_state = Self {
                agent_builder: Some(agent_builder),
                scheduler,
                document_store: None,
                tool_namespaces: HashMap::new(),
                model: opts.selection.rig,
            };

            builder_state.add_tool(brave_search::BraveSearch)?;
            builder_state.add_tool(wallet::SendTransactionToWallet)?;
            builder_state.add_tool(abi_encoder::EncodeFunctionCall)?;
            builder_state.add_tool(cast::CallViewFunction)?;
            builder_state.add_tool(cast::SimulateContractCall)?;
            builder_state.add_tool(context::GetTimeAndOnchainCtx)?;
            builder_state.add_tool(db_tools::GetContractABI)?;
            builder_state.add_tool(db_tools::GetContractSourceCode)?;
            builder_state.add_tool(etherscan::GetContractFromEtherscan)?;
            builder_state.add_tool(account::GetAccountInfo)?;
            builder_state.add_tool(account::GetAccountTransactionHistory)?;

            // Add docs tool if not skipped
            if !opts.no_docs {
                builder_state.add_docs_tool().await?;
            }

            return Ok(builder_state);
        }

        Ok(Self {
            agent_builder: Some(agent_builder),
            scheduler,
            document_store: None,
            tool_namespaces: HashMap::new(),
            model: opts.selection.rig,
        })
    }

    pub fn add_tool<T>(&mut self, tool: T) -> Result<&mut Self>
    where
        T: AomiTool + Clone + Send + Sync + 'static,
    {
        self.scheduler.register_tool(&tool)?;
        self.tool_namespaces
            .insert(T::NAME.to_string(), T::NAMESPACE.to_string());

        if let Some(builder) = self.agent_builder.take() {
            self.agent_builder = Some(builder.tool(AomiToolWrapper::new(tool)));
        }

        Ok(self)
    }

    pub async fn add_docs_tool(&mut self) -> Result<&mut Self> {
        use crate::connections::init_document_store;
        use aomi_tools::docs::SharedDocuments;
        let docs_tool = init_document_store().await?;
        self.tool_namespaces.insert(
            SharedDocuments::NAME.to_string(),
            SharedDocuments::NAMESPACE.to_string(),
        );

        if let Some(builder) = self.agent_builder.take() {
            self.agent_builder = Some(builder.tool(AomiToolWrapper::new(docs_tool.clone())));
        }
        self.document_store = Some(docs_tool.get_store());

        Ok(self)
    }

    pub async fn build(
        self,
        opts: BuildOpts,
        system_events: Option<&SystemEventQueue>,
    ) -> Result<CoreApp> {
        let agent_builder = self
            .agent_builder
            .ok_or_else(|| eyre::eyre!("CoreAppBuilder has no agent builder"))?;

        let agent = if opts.skip_mcp {
            // Skip MCP initialization for testing
            if let Some(events) = system_events {
                events.push(SystemEvent::SystemNotice(
                    "⚠️ Running without MCP server (testing mode)".to_string(),
                ));
            }
            agent_builder.build()
        } else {
            let mcp_toolbox = match mcp::toolbox().await {
                Ok(toolbox) => toolbox,
                Err(err) => {
                    if let Some(events) = system_events {
                        events.push(SystemEvent::SystemError(format!(
                            "MCP connection failed: {err}. Retrying..."
                        )));
                        toolbox_with_retry(events).await?
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

        Ok(CoreApp {
            agent: Arc::new(agent),
            document_store: self.document_store,
            tool_namespaces: Arc::new(self.tool_namespaces),
            model: self.model,
        })
    }
}

#[async_trait]
pub trait AomiApp: Send + Sync {
    type Command: Send;
    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()>;

    fn tool_namespaces(&self) -> Arc<HashMap<String, String>> {
        Arc::new(HashMap::new())
    }
}

pub struct CoreApp {
    agent: Arc<Agent<CompletionModel>>,
    document_store: Option<Arc<Mutex<DocumentStore>>>,
    tool_namespaces: Arc<HashMap<String, String>>,
    model: AomiModel,
}

impl CoreApp {
    pub async fn default() -> Result<Self> {
        Self::new(BuildOpts::default()).await
    }

    pub fn model(&self) -> AomiModel {
        self.model
    }

    pub async fn new(opts: BuildOpts) -> Result<Self> {
        let preamble = preamble().await;
        let builder = CoreAppBuilder::new(&preamble, opts, None).await?;

        // Build the final ChatApp
        builder.build(opts, None).await
    }

    pub fn agent(&self) -> Arc<Agent<CompletionModel>> {
        self.agent.clone()
    }

    pub fn document_store(&self) -> Option<Arc<Mutex<DocumentStore>>> {
        self.document_store.clone()
    }

    pub fn tool_namespaces(&self) -> Arc<HashMap<String, String>> {
        self.tool_namespaces.clone()
    }

    pub async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        mut ctx: CoreCtx<'_>,
    ) -> Result<()> {
        let agent = self.agent.clone();
        // Clone the state for the stream, preserving context window settings
        let mut core_state = CoreState::new(
            state.user_state.clone(),
            state.get_llm_context(), // Use context-limited history
            state.system_events.clone(),
            state.session_id.clone(),
            state.namespaces.clone(),
            state.tool_namespaces.clone(),
        );
        // If source state has context window enabled, enable it on the clone too
        if state.context_stats().is_some() {
            core_state.enable_context_window(None);
        }
        let stream = stream_completion(agent, input.clone(), core_state).await;

        let mut response = String::new();
        let interrupted = ctx.post_completion(&mut response, stream).await?;
        state.push_user(input);
        if !interrupted {
            if !response.trim().is_empty() {
                state.push_assistant(response);
            }
            let _ = ctx.command_sender.send(CoreCommand::Complete).await;
        }

        Ok(())
    }
}

#[async_trait]
impl AomiApp for CoreApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        CoreApp::process_message(self, input, state, ctx).await
    }

    fn tool_namespaces(&self) -> Arc<HashMap<String, String>> {
        self.tool_namespaces()
    }
}
