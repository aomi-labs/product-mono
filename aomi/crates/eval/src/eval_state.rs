use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result, bail};
use aomi_backend::{
    ChatMessage,
    session::{BackendwithTool, DefaultSessionState},
    to_rig_messages,
};
use aomi_chat::Message;
use tokio::time::{Duration, sleep};

use crate::{AgentAction, RoundResult};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
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

    fn get_new_tools(&self, last_tool_count: usize) -> Vec<String> {
        self.session
            .messages
            .iter()
            .filter_map(|msg| {
                msg.tool_stream.as_ref().map(|(topic, _)| topic.clone())
            })
            .skip(last_tool_count)
            .collect()
    }

    async fn stream_until_idle(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut total_messages = 0;
        let mut last_tool_count = 0;
        loop {
            self.session.update_state().await;

            let new_tools = self.get_new_tools(last_tool_count);
            let total_tools = last_tool_count + new_tools.len();

            if total_messages == self.session.messages.len() {
                continue;
            }
            total_messages = self.session.messages.len();
            
            let tool_list = new_tools.iter().map(|t| format!("'{}'", t)).collect::<Vec<_>>().join(", ");
            println!(
                "[test {}][streaming] {:?} messages={} tools={}: {}",
                self.test_id,
                start.elapsed(),
                total_messages,
                total_tools,
                tool_list
            );

            last_tool_count = total_tools;

            if !self.session.is_processing && !has_streaming_messages(&self.session.messages) {
                println!("[test {}][streaming] Agent is idle, returning", self.test_id);
                return Ok(());
            }

            if start.elapsed() > RESPONSE_TIMEOUT {
                bail!("timed out waiting for agent response after {RESPONSE_TIMEOUT:?}");
            }

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

