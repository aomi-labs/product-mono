pub mod eval_app;
pub mod harness;
#[cfg(test)]
pub mod test_entry;

use std::{fmt, sync::Arc, time::Instant};

use anyhow::{Context, Result, bail};
use aomi_backend::{
    ChatMessage,
    session::{BackendwithTool, DefaultSessionState, MessageSender},
    to_rig_messages,
};
use aomi_chat::Message;
use tokio::time::{Duration, sleep};

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);

pub struct EvalState {
    test_id: usize,
    session: DefaultSessionState,
    rounds: Vec<RoundResult>,
    current_round: usize,
    max_round: usize,
}

impl EvalState {
    /// Bootstraps a fresh agent session that can be used for scripted evaluations.
    pub async fn new(
        test_id: usize,
        backend: Arc<BackendwithTool>,
        max_round: usize,
    ) -> Result<Self> {
        let session = DefaultSessionState::new(backend, Vec::new())
            .await
            .context("failed to initialize eval session")?;
        Ok(Self {
            test_id,
            session,
            rounds: Vec::new(),
            current_round: 0,
            max_round,
        })
    }

    /// Replays a series of user prompts and captures the agent's actions for each round.
    pub async fn run_script<S, I>(&mut self, script: I) -> Result<()>
    where
        S: AsRef<str>,
        I: IntoIterator<Item = S>,
    {
        for line in script.into_iter() {
            let input = line.as_ref().trim();
            if input.is_empty() {
                continue;
            }

            self.run_round(input).await?;
        }
        Ok(())
    }

    /// Wrapper around the session's process_user_message that extracts the RoundResult.
    /// Hides the reciver.recv from outside, unlike in prod where we pulls the channel to stream to FE
    pub async fn run_round(&mut self, input: &str) -> Result<bool> {
        if self.current_round >= self.max_round {
            return Ok(false);
        }
        self.current_round += 1;
        let start_index = self.session.messages.len();
        println!(
            "[test {}][run_round]: Starting round {}/{} with {} messages",
            self.test_id, self.current_round, self.max_round, start_index
        );

        self.session
            .process_user_message(input.to_string())
            .await
            .with_context(|| format!("agent failed to process input: {input}"))?;

        println!(
            "[test {}][run_round]: Message sent, waiting for agent response...",
            self.test_id
        );
        self.stream_until_idle().await?;

        let new_messages = self.session.messages[start_index..]
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let actions = AgentAction::from_messages(&new_messages);
        println!("[test {}][actions] {:?}", self.test_id, actions.len());

        let round = RoundResult {
            input: input.to_string(),
            actions,
        };
        self.rounds.push(round.clone());

        // Return true if we haven't reached max rounds yet
        Ok(self.current_round < self.max_round)
    }

    async fn stream_until_idle(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut last_log = Instant::now();
        loop {
            self.session.update_state().await;

            let elapsed_secs = start.elapsed().as_secs();
            if last_log.elapsed().as_secs() >= 2 && elapsed_secs >= 80 {
                println!(
                    "[test {}][streaming] is_processing={}, has_streaming={}, messages={}, elapsed={:?}",
                    self.test_id,
                    self.session.is_processing,
                    has_streaming_messages(&self.session.messages),
                    self.session.messages.len(),
                    start.elapsed()
                );

                // Print ALL messages for debugging when close to timeout
                // for (i, msg) in self.session.messages.iter().enumerate() {
                //     println!(
                //         "  msg[{}]: sender={:?}, streaming={}, content_len={}, tool={:?}",
                //         i,
                //         msg.sender,
                //         msg.is_streaming,
                //         msg.content.len(),
                //         msg.tool_stream.as_ref().map(|(topic, _)| topic)
                //     );
                // }

                last_log = Instant::now();
            }

            if !self.session.is_processing && !has_streaming_messages(&self.session.messages) {
                println!("[streaming] Agent is idle, returning");
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

    pub fn messages(&self) -> Vec<Message> {
        // Filter out messages with empty content before converting to rig messages
        let filtered: Vec<ChatMessage> = self
            .session
            .messages
            .iter()
            .filter(|msg| !msg.content.trim().is_empty())
            .cloned()
            .collect();
        to_rig_messages(&filtered)
    }

    pub fn rounds(&self) -> &[RoundResult] {
        &self.rounds
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
                    MessageSender::Assistant => {
                        if msg.content.trim().is_empty() {
                            None
                        } else {
                            Some(AgentAction::Response(msg.content.clone()))
                        }
                    }
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
            AgentAction::ToolCall(call) => {
                if std::env::var("DEBUG").is_ok() {
                    write!(f, "[tool] {call}")
                } else {
                    let first_line = call.content.lines().next().unwrap_or("");
                    write!(f, "[tool] {} => {}", call.topic, first_line)
                }
            }
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
