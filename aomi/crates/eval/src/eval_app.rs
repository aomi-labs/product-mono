use std::{pin::Pin, sync::Arc};

use anyhow::{Result, anyhow};
use aomi_chat::{self, ChatApp, ChatAppBuilder, app::ChatCommand};
use rig::{agent::Agent, message::Message, providers::anthropic::completion::CompletionModel};
use tokio::{select, sync::mpsc};

pub type EvalCommand = ChatCommand;

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
    pub async fn headless() -> Result<Self> {
        Self::new(None).await
    }

    pub async fn with_sender(sender_to_ui: &mpsc::Sender<EvalCommand>) -> Result<Self> {
        Self::new(Some(sender_to_ui)).await
    }

    async fn new(sender_to_ui: Option<&mpsc::Sender<EvalCommand>>) -> Result<Self> {
        let builder = ChatAppBuilder::new_with_model_connection(
            &evaluation_preamble(),
            sender_to_ui,
            true, // no_tools: evaluation agent only needs model responses
        )
        .await
        .map_err(|err| anyhow!(err))?;

        let chat_app = builder
            .build(true, sender_to_ui)
            .await
            .map_err(|err| anyhow!(err))?;
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
        sender_to_ui: &mpsc::Sender<EvalCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        tracing::debug!("[eval] process message: {input}");
        self.chat_app
            .process_message(history, input, sender_to_ui, interrupt_receiver)
            .await
            .map_err(|err| anyhow!(err))
    }

    pub async fn next_eval_prompt(
        &self,
        history: &mut Vec<Message>,
        rounds_complete: usize,
        max_round: usize,
    ) -> Result<Option<String>> {
        let prompt = format!(
            "Conversation so far ({} of {max_round} rounds complete):\n\
             Provide the next user message you would send to the intent-to-trade agent. \
             If the evaluation is complete or you would repeat yourself, reply with DONE (exact word).",
            rounds_complete
        );

        // History is already filtered for empty content in EvalState::messages()
        let response = self.collect_eval_response(history, prompt).await?;
        let trimmed = response.trim();

        println!("[eval app]: {rounds_complete} out of {max_round} rounds complete\n      next: {response}");

        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("done") {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }

    async fn collect_eval_response(
        &self,
        history: &mut Vec<Message>,
        prompt: String,
    ) -> Result<String> {
        let (sender_to_ui, mut receiver_from_app) = mpsc::channel::<EvalCommand>(64);
        let (_interrupt_sender, mut interrupt_receiver) = mpsc::channel::<()>(1);
        // Keep interrupt_sender alive to prevent channel from closing

        let mut process_fut: Pin<Box<_>> =
            Box::pin(self.process_message(history, prompt, &sender_to_ui, &mut interrupt_receiver));

        let mut response = String::new();
        let mut finished_processing = false;
        let mut saw_complete = false;

        loop {
            select! {
                cmd = receiver_from_app.recv() => {
                    match cmd {
                        Some(EvalCommand::StreamingText(chunk)) => response.push_str(&chunk),
                        Some(EvalCommand::Complete) => {
                            saw_complete = true;
                            if finished_processing && receiver_from_app.is_empty() {
                                break;
                            }
                        }
                        Some(EvalCommand::Error(err)) => {
                            return Err(anyhow!("evaluation agent error: {err}"));
                        }
                        Some(EvalCommand::Interrupted) => {
                            return Err(anyhow!("evaluation agent interrupted"));
                        }
                        Some(_) => {}
                        None => {
                            if finished_processing {
                                break;
                            }
                        }
                    }
                }
                result = &mut process_fut, if !finished_processing => {
                    result?;
                    finished_processing = true;
                    if saw_complete && receiver_from_app.is_empty() {
                        break;
                    }
                }
            }

            if finished_processing && saw_complete && receiver_from_app.is_empty() {
                break;
            }
        }

        Ok(response.trim().to_string())
    }
}
