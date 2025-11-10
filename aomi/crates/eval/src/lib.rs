pub mod evaluation;

use std::{fmt, fmt::Write, sync::Arc, time::Instant};

use anyhow::{Context, Result, anyhow, bail};
use aomi_backend::{
    ChatMessage,
    session::{BackendwithTool, DefaultSessionState, MessageSender},
};
use aomi_chat::ChatApp;
use rig::message::Message;
use tokio::time::{Duration, sleep};

use crate::evaluation::EvaluationApp;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);

/// High-level harness for replaying scripted conversations against the agent.
pub struct Eval {
    session: DefaultSessionState,
}

impl Eval {
    /// Bootstraps a fresh agent session that can be used for scripted evaluations.
    pub async fn new(backend: Arc<BackendwithTool>) -> Result<Self> {
        let session = DefaultSessionState::new(backend, Vec::new())
            .await
            .context("failed to initialize eval session")?;
        Ok(Self { session })
    }

    /// Replays a series of user prompts and captures the agent's actions for each round.
    pub async fn run_script<S, I>(&mut self, script: I) -> Result<Vec<RoundResult>>
    where
        S: AsRef<str>,
        I: IntoIterator<Item = S>,
    {
        let mut results = Vec::new();

        for line in script.into_iter() {
            let input = line.as_ref().trim();
            if input.is_empty() {
                continue;
            }

            let round = self.run_round(input).await?;
            results.push(round);
        }

        Ok(results)
    }

    async fn run_round(&mut self, input: &str) -> Result<RoundResult> {
        let start_index = self.session.messages.len();
        self.session
            .process_user_message(input.to_string())
            .await
            .with_context(|| format!("agent failed to process input: {input}"))?;

        self.pump_until_idle().await?;

        let new_messages = self.session.messages[start_index..]
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let actions = AgentAction::from_messages(&new_messages);

        Ok(RoundResult {
            input: input.to_string(),
            actions,
        })
    }

    async fn pump_until_idle(&mut self) -> Result<()> {
        let start = Instant::now();
        loop {
            self.session.update_state().await;
            if !self.session.is_processing && !has_streaming_messages(&self.session.messages) {
                return Ok(());
            }

            if start.elapsed() > RESPONSE_TIMEOUT {
                bail!("timed out waiting for agent response after {RESPONSE_TIMEOUT:?}");
            }

            sleep(POLL_INTERVAL).await;
        }
    }

    /// Returns the underlying session for advanced/custom evaluation flows.
    pub fn session(&self) -> &DefaultSessionState {
        &self.session
    }
}

fn has_streaming_messages(messages: &[ChatMessage]) -> bool {
    messages.iter().any(|m| m.is_streaming)
}

#[derive(Debug, Clone)]
pub struct RoundResult {
    pub input: String,
    pub actions: Vec<AgentAction>,
}

impl RoundResult {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

impl fmt::Display for RoundResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, ">> {}", self.input)?;
        for (idx, action) in self.actions.iter().enumerate() {
            writeln!(f, "  [{idx:02}] {action}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum AgentAction {
    System(String),
    Response(String),
    ToolCall(ToolCall),
}

impl AgentAction {
    fn from_messages(messages: &[ChatMessage]) -> Vec<Self> {
        messages
            .iter()
            .filter_map(|msg| {
                if let Some(tool) = ToolCall::from_message(msg) {
                    return Some(AgentAction::ToolCall(tool));
                }

                match msg.sender {
                    MessageSender::Assistant => Some(AgentAction::Response(msg.content.clone())),
                    MessageSender::System => {
                        if msg.content.trim().is_empty() {
                            None
                        } else {
                            Some(AgentAction::System(msg.content.clone()))
                        }
                    }
                    MessageSender::User => None,
                }
            })
            .collect()
    }
}

impl fmt::Display for AgentAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentAction::System(text) => write!(f, "[system] {text}"),
            AgentAction::Response(text) => write!(f, "[response] {text}"),
            AgentAction::ToolCall(call) => write!(f, "[tool] {call}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub topic: String,
    pub content: String,
}

impl ToolCall {
    fn from_message(msg: &ChatMessage) -> Option<Self> {
        msg.tool_stream.as_ref().map(|(topic, content)| ToolCall {
            topic: topic.clone(),
            content: content.clone(),
        })
    }
}

impl fmt::Display for ToolCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.topic, self.content)
    }
}

/// Runs the primary agent against the evaluation persona until it decides to stop or hits `max_round`.
pub async fn eval_with_agent(initial_intent: String, max_round: usize) -> Result<Vec<RoundResult>> {
    if max_round == 0 {
        return Ok(Vec::new());
    }

    let trimmed = initial_intent.trim().to_string();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let chat_app = Arc::new(ChatApp::new().await.map_err(|err| anyhow!(err))?);
    let backend: Arc<BackendwithTool> = chat_app;
    let mut harness = Eval::new(backend).await?;

    let eval_app = EvaluationApp::headless().await?;
    let mut eval_history: Vec<Message> = Vec::new();
    let mut rounds = Vec::new();
    let mut next_prompt = trimmed;

    for _ in 0..max_round {
        let round = harness.run_round(&next_prompt).await?;
        rounds.push(round);

        if rounds.len() >= max_round {
            break;
        }

        let transcript = format_transcript(&rounds);
        let maybe_prompt = eval_app
            .next_eval_prompt(&mut eval_history, transcript, rounds.len(), max_round)
            .await?;

        match maybe_prompt {
            Some(prompt) => next_prompt = prompt,
            None => break,
        }
    }

    Ok(rounds)
}

fn format_transcript(rounds: &[RoundResult]) -> String {
    let mut buffer = String::new();
    for (idx, round) in rounds.iter().enumerate() {
        let _ = writeln!(&mut buffer, "Round {}:", idx + 1);
        let _ = writeln!(&mut buffer, "{round}");
    }
    buffer
}
