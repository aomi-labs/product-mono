use std::pin::Pin;

use anyhow::{Result, anyhow};
use aomi_core::{
    AomiModel, BuildOpts, CoreApp, CoreAppBuilder, Selection, SystemEventQueue, UserState,
    app::{AgentKind, CoreCommand, CoreCtx, CoreState},
    prompts::{PreambleBuilder, PromptSection},
};
use rig::message::Message;
use tokio::{select, sync::mpsc};

pub type EvalCommand = CoreCommand;

pub const EVAL_ACCOUNTS: &[(&str, &str)] = &[
    ("Alice", "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
    ("Bob", "0x8D343ba80a4cD896e3e5ADFF32F9cF339A697b28"),
];

const EVAL_ROLE: &str = "You are a Web3 user evaluating this onchain trading agent. Talk like a real user with straightforward requests. Your goal as a user is to execute your trading request ASAP.";

const EVAL_EXAMPLES: &[&str] = &[
    "'check my balance'",
    "'i want the best yield'",
    "'find my balance'",
];

const EVAL_BEHAVIOR: &[&str] = &[
    "When the agent asks for a decision, reply with 'yes' or 'no'",
    "When you see \"Transaction confirmed on-chain\" in system messages, the transaction is complete—do NOT ask for additional verification",
    "Never ask the agent to simulate or fabricate balances—demand verifiable on-chain state each time",
];

fn evaluation_preamble() -> String {
    let accounts_list: Vec<String> = EVAL_ACCOUNTS
        .iter()
        .map(|(name, address)| format!("{}: {}", name, address))
        .collect();

    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(EVAL_ROLE))
        .section(
            PromptSection::titled("Example Requests").bullet_list(EVAL_EXAMPLES.iter().copied()),
        )
        .section(PromptSection::titled("Behavior Rules").bullet_list(EVAL_BEHAVIOR.iter().copied()))
        .section(
            PromptSection::titled("Environment")
                .paragraph("Ethereum mainnet with funded default accounts."),
        )
        .section(PromptSection::titled("Known Accounts").bullet_list(accounts_list))
        .build()
}

pub struct EvaluationApp {
    chat_app: CoreApp,
    system_events: SystemEventQueue,
}

#[derive(Debug, Clone)]
pub struct ExpectationVerdict {
    pub satisfied: bool,
    pub explanation: String,
}

impl EvaluationApp {
    pub async fn headless() -> Result<Self> {
        Self::new().await
    }

    pub async fn with_sender(command_sender: &mpsc::Sender<EvalCommand>) -> Result<Self> {
        let _ = command_sender;
        Self::new().await
    }

    async fn new() -> Result<Self> {
        let system_events = SystemEventQueue::new();
        let opts = BuildOpts {
            no_tools: true,
            selection: Selection {
                rig: AomiModel::ClaudeOpus4,
                baml: AomiModel::ClaudeOpus4,
            },
            ..BuildOpts::default()
        };
        let builder = CoreAppBuilder::new(&evaluation_preamble(), opts, Some(&system_events))
            .await
            .map_err(|err| anyhow!(err))?;

        let chat_app = builder
            .build(opts, Some(&system_events))
            .await
            .map_err(|err| anyhow!(err))?;
        Ok(Self {
            chat_app,
            system_events,
        })
    }

    pub fn agent(&self) -> AgentKind {
        self.chat_app.agent()
    }

    pub fn chat_app(&self) -> &CoreApp {
        &self.chat_app
    }

    pub async fn process_message(
        &self,
        history: &mut Vec<Message>,
        input: String,
        command_sender: &mpsc::Sender<EvalCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        tracing::debug!("[eval] process message: {input}");
        let mut state = CoreState {
            user_state: UserState::default(),
            history: history.clone(),
            system_events: Some(self.system_events.clone()),
            session_id: "eval".to_string(),
            namespaces: vec!["default".to_string()],
            tool_namespaces: self.chat_app.tool_namespaces(),
        };
        let ctx = CoreCtx {
            command_sender: command_sender.clone(),
            interrupt_receiver: Some(interrupt_receiver),
        };
        self.chat_app
            .process_message(input, &mut state, ctx)
            .await
            .map_err(|err| anyhow!(err))?;
        *history = state.history;
        Ok(())
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
        let (command_sender, mut receiver_from_app) = mpsc::channel::<EvalCommand>(64);
        let (_interrupt_sender, mut interrupt_receiver) = mpsc::channel::<()>(1);
        // Keep interrupt_sender alive to prevent channel from closing

        let mut process_fut: Pin<Box<_>> = Box::pin(self.process_message(
            history,
            prompt,
            &command_sender,
            &mut interrupt_receiver,
        ));

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
