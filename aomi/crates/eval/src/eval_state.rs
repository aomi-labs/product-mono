use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result, bail};
use aomi_backend::{
    ChatMessage, MessageSender,
    session::{BackendwithTool, DefaultSessionState},
    to_rig_messages,
};
use aomi_chat::{Message, accounts::ANVIL_ACCOUNTS};
use chrono::Utc;
use tokio::time::{Duration, sleep};

use crate::{AgentAction, RoundResult};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);
const ANVIL_CHAIN_ID: u64 = 31337;
const ANVIL_RPC_URL: &str = "http://127.0.0.1:8545";
const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

fn account_address_or_default(index: usize) -> &'static str {
    ANVIL_ACCOUNTS
        .get(index)
        .map(|(address, _)| *address)
        .unwrap_or(ZERO_ADDRESS)
}

fn system_message(content: String) -> ChatMessage {
    ChatMessage {
        sender: MessageSender::System,
        content,
        tool_stream: None,
        timestamp: Utc::now().to_rfc3339(),
        is_streaming: false,
    }
}

fn default_session_history() -> Vec<ChatMessage> {
    let alice = account_address_or_default(0);
    let bob = account_address_or_default(1);

    vec![
        system_message(format!(
            "User connected wallet with address {} on testnet network (Chain ID: {}). Ready to help with transactions.",
            alice, ANVIL_CHAIN_ID
        )),
        system_message(format!(
            "Local Anvil Ethereum testnet is running at {}. Use the `testnet` network for every tool call generated during evaluation.",
            ANVIL_RPC_URL
        )),
        system_message(format!(
            "Evaluation harness provides two funded test accounts on this Anvil chain:\n- Alice (account 0): {}\n- Bob (account 1): {}\nUse Alice as the sending wallet and Bob as the counterparty when exercising on-chain transactions.",
            alice, bob
        )),
    ]
}

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
        let session = DefaultSessionState::new(backend, default_session_history())
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
        let round_number = self.current_round;
        let start_index = self.session.messages.len();
        let round_start = Instant::now();
        println!(
            "[test {}] ▶ Round {}/{} | user: {}",
            self.test_id, round_number, self.max_round, input
        );

        self.session
            .process_user_message(input.to_string())
            .await
            .with_context(|| format!("agent failed to process input: {input}"))?;

        println!("[test {}]   waiting for agent response...", self.test_id);
        self.stream_until_idle().await?;

        let new_messages = self.session.messages[start_index..].to_vec();
        let actions = AgentAction::from_messages(&new_messages);

        let round = RoundResult {
            input: input.to_string(),
            actions,
        };
        self.rounds.push(round.clone());
        self.log_round_actions(round_number, &round);

        let duration = round_start.elapsed();
        println!(
            "[test {}] ✅ Round {}/{} finished in {:.1}s | tools: {} | responses: {}",
            self.test_id,
            round_number,
            self.max_round,
            duration.as_secs_f32(),
            round.tool_call_count(),
            round.response_count()
        );

        // Return true if we haven't reached max rounds yet
        Ok(self.current_round < self.max_round)
    }

    fn get_new_tools(&self, last_tool_count: usize) -> Vec<(String, String)> {
        self.session
            .messages
            .iter()
            .filter_map(|msg| {
                msg.tool_stream
                    .as_ref()
                    .map(|(topic, content)| (topic.clone(), content.clone()))
            })
            .skip(last_tool_count)
            .collect()
    }

    fn log_round_actions(&self, round_number: usize, round: &RoundResult) {
        if round.actions.is_empty() {
            println!(
                "[test {}]   (no agent output captured for round {})",
                self.test_id, round_number
            );
            return;
        }

        println!(
            "[test {}] Agent output for round {}:",
            self.test_id, round_number
        );
        for (idx, action) in round.actions.iter().enumerate() {
            println!("[test {}]   [{idx:02}] {action}", self.test_id);
        }
    }

    async fn stream_until_idle(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut total_messages = 0;
        let mut last_tool_count = 0;

        loop {
            self.session.update_state().await;

            let new_tools = self.get_new_tools(last_tool_count);
            let total_tools = last_tool_count + new_tools.len();
            for (topic, content) in &new_tools {
                let preview = content.lines().next().unwrap_or("").trim();
                println!(
                    "[test {}][tool-call] {} => {}",
                    self.test_id,
                    topic,
                    if preview.is_empty() { "[no content]" } else { preview }
                );
            }

            if total_messages != self.session.messages.len() {
                total_messages = self.session.messages.len();

                let tool_list = new_tools
                    .iter()
                    .map(|(t, _)| format!("'{}'", t))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "[test {}][streaming] {:?} messages={} tools={}: {}",
                    self.test_id,
                    start.elapsed(),
                    total_messages,
                    total_tools,
                    tool_list
                );

                last_tool_count = total_tools;
            }

            // Check if agent is idle
            let is_processing = self.session.is_processing;
            let has_streaming = has_streaming_messages(&self.session.messages);

            if !is_processing && !has_streaming {
                return Ok(());
            }

            if start.elapsed() > RESPONSE_TIMEOUT {
                println!(
                    "[test {}] ⚠️ timeout waiting for agent (is_processing={}, has_streaming={}, messages={})",
                    self.test_id,
                    is_processing,
                    has_streaming,
                    self.session.messages.len()
                );
                bail!("timed out waiting for agent response after {RESPONSE_TIMEOUT:?}");
            }

            // Sleep to avoid tight loop and let async runtime process messages
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
