use std::sync::Arc;

use aomi_chat::{
    ChatApp, ChatAppBuilder, SystemEventQueue,
    app::{ChatCommand, LoadingProgress},
};
use aomi_tools::ToolScheduler;
use eyre::Result;
use rig::{agent::Agent, message::Message, providers::anthropic::completion::CompletionModel};
use tokio::sync::{Mutex, mpsc};

use crate::tools::{
    CompileSession, EditScript, ExecuteContract, FetchContract, SearchDocs,
};

pub type ScriptAppCommand = ChatCommand;

fn script_app_preamble() -> String {
    format!(
        "You are an AI assistant that reviews and iterates on generated Solidity scripts.

Your workflow:
- Review the generated script and dependencies
- Edit and retry compilation when needed
- Execute the script and audit results for correctness

Tools will provide compile, execute, edit, fetch, and doc search capabilities.\n\n{}",
        aomi_chat::generate_account_context()
    )
}

pub struct ScriptApp {
    chat_app: ChatApp,
}

impl ScriptApp {
    pub async fn new() -> Result<Self> {
        Self::init_internal(true, true, None, None).await
    }

    pub async fn new_with_options(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        Self::init_internal(skip_docs, skip_mcp, None, None).await
    }

    pub async fn new_with_senders(
        sender_to_ui: &mpsc::Sender<ScriptAppCommand>,
        loading_sender: mpsc::Sender<LoadingProgress>,
        skip_docs: bool,
    ) -> Result<Self> {
        Self::init_internal(skip_docs, false, Some(sender_to_ui), Some(loading_sender)).await
    }

    async fn init_internal(
        skip_docs: bool,
        skip_mcp: bool,
        sender_to_ui: Option<&mpsc::Sender<ScriptAppCommand>>,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
    ) -> Result<Self> {
        let mut builder = ChatAppBuilder::new_with_model_connection(
            &script_app_preamble(),
            sender_to_ui,
            false,
            None,
        )
        .await?;

        builder.add_tool(CompileSession)?;
        builder.add_tool(ExecuteContract)?;
        builder.add_tool(EditScript)?;
        builder.add_tool(FetchContract)?;
        builder.add_tool(SearchDocs)?;

        if !skip_docs {
            builder.add_docs_tool(loading_sender, sender_to_ui).await?;
        }

        let chat_app = builder.build(skip_mcp, None, sender_to_ui).await?;

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
        system_events: &SystemEventQueue,
        handler: Arc<Mutex<aomi_tools::scheduler::ToolApiHandler>>,
        input: String,
        sender_to_ui: &mpsc::Sender<ScriptAppCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        tracing::debug!("[script_app] process message: {}", input);
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

pub async fn run_script_app_chat(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<ScriptAppCommand>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let app = Arc::new(ScriptApp::new_with_senders(&sender_to_ui, loading_sender, skip_docs).await?);
    let mut agent_history: Vec<Message> = Vec::new();
    let system_events = SystemEventQueue::new();

    use aomi_chat::connections::ensure_connection_with_retries;
    ensure_connection_with_retries(&app.agent(), &system_events).await?;

    let mut receiver_from_ui = receiver_from_ui;
    let mut interrupt_receiver = interrupt_receiver;
    let scheduler = ToolScheduler::get_or_init().await?;
    let handler = Arc::new(Mutex::new(scheduler.get_handler()));

    while let Some(input) = receiver_from_ui.recv().await {
        app.process_message(
            &mut agent_history,
            &system_events,
            handler.clone(),
            input,
            &sender_to_ui,
            &mut interrupt_receiver,
        )
        .await?;
    }

    Ok(())
}
