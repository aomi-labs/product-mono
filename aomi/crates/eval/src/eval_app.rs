use std::{pin::Pin, sync::Arc};

use anyhow::{Result, anyhow};
use aomi_chat::{self, ChatApp, ChatAppBuilder, SystemEventQueue, app::ChatCommand};
use aomi_tools::scheduler::ToolApiHandler;
use rig::{agent::Agent, message::Message, providers::anthropic::completion::CompletionModel};
use tokio::{select, sync::{mpsc, Mutex}};

pub type EvalCommand = ChatCommand;

pub const EVAL_ACCOUNTS: &[(&str, &str)] = &[
    ("Alice", "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
    ("Bob", "0x8D343ba80a4cD896e3e5ADFF32F9cF339A697b28"),
];

fn evaluation_preamble() -> String {
    let accounts_str = EVAL_ACCOUNTS
        .iter()
        .map(|(name, address)| format!("    - {}: {}", name, address))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are a Web3 user evaluating this onchain trading agent. \
        Talk like an real user with straight forward request. Your goal as an user is to execute your trading request ASAP.\
        For example: \
        - 'check my balance' \
        - 'i want the best yield' \
        - 'find my balance'\
        When the agent ask you for decision, you should reply with 'yes' or 'no'.\
        The environment is Ethereum mainnet with funded default accounts. \
        When you see \"Transaction confirmed on-chain\" in system messages, the transaction is complete and you should NOT ask for additional verification. \
        Never ask the agent to simulate or fabricate balances—demand verifiable on-chain state each time. \
        Known accounts:\n{accounts_str}"
    )
}

pub struct EvaluationApp {
    chat_app: ChatApp,
    system_events: SystemEventQueue,
    tool_handler: Arc<Mutex<ToolApiHandler>>,
}

#[derive(Debug, Clone)]
pub struct ExpectationVerdict {
    pub satisfied: bool,
    pub explanation: String,
}

impl EvaluationApp {
    pub async fn headless() -> Result<Self> {
        Self::new(None).await
    }

    pub async fn with_sender(sender_to_ui: &mpsc::Sender<EvalCommand>) -> Result<Self> {
        Self::new(Some(sender_to_ui)).await
    }

    async fn new(sender_to_ui: Option<&mpsc::Sender<EvalCommand>>) -> Result<Self> {
        let system_events = SystemEventQueue::new();
        let scheduler = aomi_tools::scheduler::ToolScheduler::get_or_init()
            .await
            .map_err(|err| anyhow!(err))?;
        let tool_handler = Arc::new(Mutex::new(scheduler.get_handler()));
        let builder = ChatAppBuilder::new_with_model_connection(
            &evaluation_preamble(),
            sender_to_ui,
            true, // no_tools: evaluation agent only needs model responses
            Some(&system_events),
        )
        .await
        .map_err(|err| anyhow!(err))?;

        let chat_app = builder
            .build(true, Some(&system_events), sender_to_ui)
            .await
            .map_err(|err| anyhow!(err))?;
        Ok(Self {
            chat_app,
            system_events,
            tool_handler,
        })
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
            .process_message(
                history,
                input,
                sender_to_ui,
                &self.system_events,
                self.tool_handler.clone(),
                interrupt_receiver,
            )
            .await
            .map_err(|err| anyhow!(err))
    }

    pub async fn next_eval_prompt(
        &self,
        history: &mut Vec<Message>,
        original_intent: &str,
        rounds_complete: usize,
        max_round: usize,
    ) -> Result<Option<String>> {
        let prompt = format!(
            "Original user intent:\n{original_intent}\n\n\
            Conversation so far ({rounds_complete} of {max_round} rounds complete):\n\
            Decide if the user's intent has been satisfied. If it has (or more messages would be redundant), reply with DONE (exact word).\n\
            Otherwise, provide the next user message you would send to progress the original intent.\n\n\
            IMPORTANT: Transaction execution rules:\n\
            - When you see \"Transaction confirmed on-chain\" in the conversation, the transaction has been successfully executed. \
            DO NOT ask the agent to verify the transaction, check balances, or confirm execution again.\n\
            - Once a transaction is confirmed on-chain, consider the agent's work complete for that transaction. \
            Do not insist on additional verification rounds.\n\
            - The evaluation focuses on whether the agent correctly prepared and submitted the transaction, not on post-execution verification.\n\
            - If the user's original intent was to execute a transaction and it has been confirmed, reply with DONE.\
            "
        );

        // History is already filtered for empty content in EvalState::messages()
        let response = self.run_eval_prompt(history, prompt).await?;
        let trimmed = response.trim();

        println!(
            "[eval app]: {rounds_complete} out of {max_round} rounds complete\n      next: {response}"
        );

        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("done") {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }

    pub async fn run_eval_prompt(
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

    pub async fn judge_expectation(
        &self,
        history: &mut Vec<Message>,
        expectation: &str,
    ) -> Result<ExpectationVerdict> {
        let prompt = format!(
            "You are reviewing the entire prior conversation between a user and an agent (already included in history). \
            Determine whether the agent satisfied this expectation:\n\"{expectation}\".\n\
            Reply with either 'YES - <reason>' if the expectation was met or 'NO - <reason>' if it was not. \
            <!-- Since we evaluate the agent on Ethereum mainnet, the transaction execution is not part of the evaluation. --> \
            DO NOT fail the expectation because of the transaction execution.
            Keep the reason under 40 words."
        );

        let response = self.run_eval_prompt(history, prompt).await?;
        let trimmed = response.trim().to_string();
        let satisfied = trimmed
            .chars()
            .take(3)
            .collect::<String>()
            .to_ascii_uppercase()
            .starts_with("YES");

        Ok(ExpectationVerdict {
            satisfied,
            explanation: trimmed,
        })
    }
}
