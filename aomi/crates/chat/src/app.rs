use std::collections::HashMap;
use std::sync::Arc;

use aomi_mcp::client::{self as mcp};
use aomi_rag::DocumentStore;
use aomi_tools::{
    AomiTool, AomiToolWrapper, CallMetadata, ToolReturn, ToolScheduler, abi_encoder, account, brave_search, cast, db_tools, etherscan, time, wallet
};
use async_trait::async_trait;
use eyre::Result;
use futures::{StreamExt, future};
use rig::{
    OneOrMany,
    agent::{Agent, AgentBuilder},
    message::{AssistantContent, Message},
    prelude::*,
    providers::anthropic::completion::CompletionModel,
};
use tokio::sync::{Mutex, mpsc};

use crate::{
    SystemEvent, SystemEventQueue,
    completion::{StreamingError, stream_completion},
    connections::toolbox_with_retry,
    generate_account_context,
    prompts::{PromptSection, agent_preamble_builder},
};

// Type alias for CoreCommand
pub type CoreCommand = crate::CoreCommand;

// Environment variables
pub static ANTHROPIC_API_KEY: std::sync::LazyLock<Result<String, std::env::VarError>> =
    std::sync::LazyLock::new(|| std::env::var("ANTHROPIC_API_KEY"));

const CLAUDE_3_5_SONNET: &str = "claude-sonnet-4-20250514";

async fn preamble() -> String {
    agent_preamble_builder()
        .await
        .section(PromptSection::titled("Account Context").paragraph(generate_account_context()))
        .build()
}

pub struct CoreAppBuilder {
    agent_builder: Option<AgentBuilder<CompletionModel>>,
    scheduler: Arc<ToolScheduler>,
    document_store: Option<Arc<Mutex<DocumentStore>>>,
    tool_namespaces: HashMap<String, String>,
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
        })
    }

    pub fn scheduler(&self) -> Arc<ToolScheduler> {
        self.scheduler.clone()
    }

    pub async fn new(
        preamble: &str,
        no_tools: bool,
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

        let anthropic_client = rig::providers::anthropic::Client::new(&anthropic_api_key);
        let agent_builder = anthropic_client.agent(CLAUDE_3_5_SONNET).preamble(preamble);

        // Get or initialize the global scheduler and register core tools
        let scheduler = ToolScheduler::get_or_init().await?;

        if !no_tools {
            let mut builder_state = Self {
                agent_builder: Some(agent_builder),
                scheduler,
                document_store: None,
                tool_namespaces: HashMap::new(),
            };

            builder_state.add_tool(brave_search::BraveSearch)?;
            builder_state.add_tool(wallet::SendTransactionToWallet)?;
            builder_state.add_tool(abi_encoder::EncodeFunctionCall)?;
            builder_state.add_tool(cast::CallViewFunction)?;
            builder_state.add_tool(cast::SimulateContractCall)?;
            builder_state.add_tool(time::GetCurrentTime)?;
            builder_state.add_tool(db_tools::GetContractABI)?;
            builder_state.add_tool(db_tools::GetContractSourceCode)?;
            builder_state.add_tool(etherscan::GetContractFromEtherscan)?;
            builder_state.add_tool(account::GetAccountInfo)?;
            builder_state.add_tool(account::GetAccountTransactionHistory)?;

            return Ok(builder_state);
        }

        Ok(Self {
            agent_builder: Some(agent_builder),
            scheduler,
            document_store: None,
            tool_namespaces: HashMap::new(),
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
        skip_mcp: bool,
        system_events: Option<&SystemEventQueue>,
    ) -> Result<CoreApp> {
        let agent_builder = self
            .agent_builder
            .ok_or_else(|| eyre::eyre!("CoreAppBuilder has no agent builder"))?;

        let agent = if skip_mcp {
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
        })
    }
}

#[derive(Clone)]
pub struct CoreState {
    pub history: Vec<Message>,
    pub system_events: Option<SystemEventQueue>,
    /// Session identifier for session-aware tool execution
    pub session_id: String,
    /// Tool namespaces allowed for this session
    pub namespaces: Vec<String>,
    /// Aomi tool name to namespace map for runtime envelope handling
    pub tool_namespaces: Arc<HashMap<String, String>>,
}

impl CoreState {
    pub fn push_tool_call(&mut self, tool_call: &rig::message::ToolCall) {
        self.history.push(Message::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::ToolCall(tool_call.clone())),
        });
    }

    pub fn push_async_update(
        &mut self,
        tool_name: String,
        call_id: aomi_tools::CallMetadata,
        result_text: String,
    ) {
        let call_id_text = call_id.call_id.as_deref().unwrap_or("none").to_string();
        self.history.push(Message::user(format!(
            "[[SYSTEM]] Tool result for {} with id {} (call_id={}): {}",
            tool_name, call_id.id, call_id_text, result_text
        )));
    }

    pub fn push_tool_results(&mut self, tool_returns: Vec<ToolReturn>) {
        for tool_return in tool_returns {
            let ToolReturn { metadata, inner, .. } = tool_return;
            let CallMetadata { id, call_id, .. } = metadata;
            if let Some(call_id) = call_id {
                self.history.push(Message::User {
                    content: OneOrMany::one(rig::message::UserContent::tool_result_with_call_id(
                        id,
                        call_id,
                        OneOrMany::one(rig::message::ToolResultContent::text(inner.to_string())),
                    )),
                });
            } else {
                self.history.push(Message::User {
                    content: OneOrMany::one(rig::message::UserContent::tool_result(
                        id,
                        OneOrMany::one(rig::message::ToolResultContent::text(inner.to_string())),
                    )),
                });
            }
        }
    }

    pub fn push_user(&mut self, content: impl Into<String>) {
        self.history.push(Message::user(content));
    }

    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.history.push(Message::assistant(content));
    }
}

pub struct CoreCtx<'a> {
    pub command_sender: mpsc::Sender<CoreCommand>,
    pub interrupt_receiver: Option<&'a mut mpsc::Receiver<()>>,
}

impl<'a> CoreCtx<'a> {
    async fn post_completion<S>(&mut self, response: &mut String, mut stream: S) -> Result<bool>
    where
        S: futures::Stream<Item = Result<CoreCommand, StreamingError>> + Unpin,
    {
        let mut interrupted = false;

        loop {
            tokio::select! {
                content = stream.next() => {
                    match content {
                        Some(Ok(command)) => {
                            if let CoreCommand::StreamingText(text) = &command {
                                response.push_str(text);
                            }
                            let _ = self.command_sender.send(command).await;
                        },
                        Some(Err(err)) => {
                            let is_completion_error = matches!(err, StreamingError::Completion(_));
                            let message = err.to_string();
                            let _ = self.command_sender.send(CoreCommand::Error(message)).await;
                            if is_completion_error {
                                todo!();
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }
                _ = async {
                    if let Some(interrupt_receiver) = self.interrupt_receiver.as_mut() {
                        interrupt_receiver.recv().await;
                    } else {
                        future::pending::<()>().await;
                    }
                } => {
                    interrupted = true;
                    let _ = self.command_sender.send(CoreCommand::Interrupted).await;
                    break;
                }
            }
        }
        Ok(interrupted)
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
}

impl CoreApp {
    pub async fn default() -> Result<Self> {
        Self::new(true, true, false, None).await
    }

    pub async fn new_with_options(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        Self::new(skip_docs, skip_mcp, false, None).await
    }

    pub async fn headless() -> Result<Self> {
        // For evaluation/testing: skip docs, skip MCP, and skip tools
        Self::new(true, true, true, None).await
    }

    async fn new(
        skip_docs: bool,
        skip_mcp: bool,
        no_tools: bool,
        system_events: Option<&SystemEventQueue>,
    ) -> Result<Self> {
        let mut builder = CoreAppBuilder::new(&preamble().await, no_tools, system_events).await?;

        // Add docs tool if not skipped
        if !skip_docs {
            builder.add_docs_tool().await?;
        }

        // Build the final ChatApp
        builder.build(skip_mcp, system_events).await
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
        let core_state = CoreState {
            history: state.history.clone(),
            system_events: state.system_events.clone(),
            session_id: state.session_id.clone(),
            namespaces: state.namespaces.clone(),
            tool_namespaces: state.tool_namespaces.clone(),
        };
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
