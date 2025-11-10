use std::sync::Arc;

use anyhow::Result;
use aomi_chat::{
    self, ChatApp, ChatAppBuilder, app::ChatCommand, connections::ensure_connection_with_retries,
};
use rig::{agent::Agent, message::Message, providers::anthropic::completion::CompletionModel};
use tokio::sync::mpsc;

pub type EvaluationCommand = ChatCommand;

fn evaluation_preamble() -> String {
    format!(
        "You are a Web3 user evaluating this intent-to-trade agent. \
        Review every response like it directly affects your own wallet—push for execution details, \
        question missing data, and surface risks before approving any trade intent.\n\n{}",
        aomi_chat::generate_account_context()
    )
}

pub struct EvaluationApp {
    chat_app: ChatApp,
}

impl EvaluationApp {
    pub async fn new(skip_mcp: bool) -> Result<Self> {
        Self::init_internal(skip_mcp, None).await
    }

    async fn init_internal(
        skip_mcp: bool,
        sender_to_ui: Option<&mpsc::Sender<EvaluationCommand>>,
    ) -> Result<Self> {
        let builder = ChatAppBuilder::new_with_model_connection(
            &evaluation_preamble(),
            sender_to_ui,
            true, // no_tools: evaluation agent only needs model responses
        )
        .await?;

        let chat_app = builder.build(skip_mcp, sender_to_ui).await?;
        Ok(Self { chat_app })
    }

    pub fn agent(&self) -> Arc<Agent<CompletionModel>> {
        self.chat_app.agent()
    }

    pub fn chat_app(&self) -> &ChatApp {
        &self.chat_app
    }

    pub async fn process_message(
        &self,
        history: &mut Vec<Message>,
        input: String,
        sender_to_ui: &mpsc::Sender<EvaluationCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        tracing::debug!("[eval] process message: {input}");
        self.chat_app
            .process_message(history, input, sender_to_ui, interrupt_receiver)
            .await
    }
}

pub async fn run_evaluation_chat(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<EvaluationCommand>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_mcp: bool,
) -> Result<()> {
    let app = Arc::new(EvaluationApp::new_with_sender(&sender_to_ui, skip_mcp).await?);
    let mut agent_history: Vec<Message> = Vec::new();

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
