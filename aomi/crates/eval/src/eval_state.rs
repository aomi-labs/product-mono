use std::{
    sync::{Arc, OnceLock},
    time::Instant,
};

use anyhow::{Context, Result, bail};
use aomi_anvil::provider_manager;
use aomi_backend::{
    ChatMessage, MessageSender, UserState,
    session::{AomiBackend, DefaultSessionState},
};
use aomi_core::Message;
use chrono::Utc;
use colored::{ColoredString, Colorize};
use tokio::time::{Duration, sleep};

use crate::{
    AgentAction, RoundResult,
    eval_app::{alice_address, bob_address},
    truncate_tool_log,
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);
const ANVIL_CHAIN_ID: u64 = 1;
const COLOR_ENV_KEYS: &[&str] = &["EVAL_COLOR", "FORCE_COLOR", "CLICOLOR_FORCE"];

fn log_prefix(test_id: usize) -> ColoredString {
    format!("[test {}]", test_id).bright_black().bold()
}

fn init_color_output() {
    static COLOR_OVERRIDE: OnceLock<()> = OnceLock::new();
    COLOR_OVERRIDE.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            colored::control::set_override(false);
            return;
        }

        let force_color = COLOR_ENV_KEYS.iter().any(|key| env_flag_enabled(key));
        if force_color {
            colored::control::set_override(true);
        }
    });
}

fn system_message(content: String) -> ChatMessage {
    ChatMessage {
        sender: MessageSender::System,
        content,
        tool_result: None,
        timestamp: Utc::now().to_rfc3339(),
        is_streaming: false,
    }
}

async fn default_session_history() -> Result<Vec<ChatMessage>> {
    let alice = alice_address();
    let bob = bob_address();
    let rpc_url = provider_manager()
        .await?
        .default_endpoint()
        .ok_or_else(|| anyhow::anyhow!("No default endpoint configured"))?;

    Ok(vec![
        system_message(format!(
            "User connected wallet with address {} on mainnet network (Chain ID: {}). Ready to help with transactions.",
            alice, ANVIL_CHAIN_ID
        )),
        system_message(format!(
            "Local Anvil Ethereum mainnet is running at {}. Use the `ethereum` network for every tool call generated during evaluation.",
            rpc_url
        )),
        system_message(format!(
            "Evaluation harness provides two funded test accounts on this Anvil chain:\n- Alice (account 0): {}\n- Bob (account 1): {}\nUse Alice as the sending wallet and Bob as the counterparty when exercising on-chain transactions.",
            alice, bob
        )),
    ])
}

pub struct EvalState {
    test_id: usize,
    session: DefaultSessionState,
    rounds: Vec<RoundResult>,
    current_round: usize,
    max_round: usize,
}

/// Returns UserState for Alice (the test wallet) with connected status
fn alice_user_state() -> UserState {
    UserState {
        address: Some(alice_address().to_string()),
        chain_id: Some(ANVIL_CHAIN_ID),
        is_connected: true,
        ens_name: None,
    }
}

impl EvalState {
    /// Bootstraps a fresh agent session that can be used for scripted evaluations.
    pub async fn new(test_id: usize, backend: Arc<AomiBackend>, max_round: usize) -> Result<Self> {
        init_color_output();
        let mut session = DefaultSessionState::new(backend, default_session_history().await?)
            .await
            .context("failed to initialize eval session")?;

        // Sync Alice wallet state so agent knows about the connected wallet
        session.sync_user_state(alice_user_state()).await;
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

    /// Wrapper around the session's send_user_input that extracts the RoundResult.
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
            "{} {}",
            log_prefix(self.test_id),
            format!(
                "▶ Round {}/{} | user: {}",
                round_number, self.max_round, input
            )
            .bright_blue()
            .bold()
        );

        self.session
            .send_user_input(input.to_string())
            .await
            .with_context(|| format!("agent failed to process input: {input}"))?;

        println!(
            "{} {}",
            log_prefix(self.test_id),
            "  waiting for agent response...".cyan()
        );
        self.stream_until_idle().await?;
        self.compact_session_history();

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
            "{} {}",
            log_prefix(self.test_id),
            format!(
                "✅ Round {}/{} finished in {:.1}s | tools: {} | responses: {}",
                round_number,
                self.max_round,
                duration.as_secs_f32(),
                round.tool_call_count(),
                round.response_count()
            )
            .green()
        );

        // Return true if we haven't reached max rounds yet
        Ok(self.current_round < self.max_round)
    }

    fn get_new_tools(&self, last_tool_count: usize) -> Vec<(String, String)> {
        self.session
            .messages
            .iter()
            .filter_map(|msg| {
                msg.tool_result
                    .as_ref()
                    .map(|(topic, content)| (topic.clone(), content.clone()))
            })
            .skip(last_tool_count)
            .collect()
    }

    fn log_round_actions(&self, round_number: usize, round: &RoundResult) {
        if round.actions.is_empty() {
            println!(
                "{} {}",
                log_prefix(self.test_id),
                format!("  (no agent output captured for round {})", round_number).yellow()
            );
            return;
        }

        println!(
            "{} {}",
            log_prefix(self.test_id),
            format!("Agent output for round {}:", round_number)
                .bright_blue()
                .bold()
        );
        for (idx, action) in round.actions.iter().enumerate() {
            println!(
                "{} {}",
                log_prefix(self.test_id),
                format!("  [{idx:02}] {action}").white()
            );
        }
    }

    fn compact_session_history(&mut self) {
        let max_message_chars = env_usize("EVAL_MESSAGE_MAX_CHARS", 4000);
        let max_tool_chars = env_usize("EVAL_TOOL_CONTENT_MAX_CHARS", 2000);

        for message in &mut self.session.messages {
            if let Some((topic, content)) = message.tool_result.as_mut()
                && should_truncate_tool_stream(topic)
            {
                *content = truncate_middle(content, max_tool_chars);
            }
            if message.content.len() > max_message_chars {
                message.content = truncate_middle(&message.content, max_message_chars);
            }
        }
    }

    async fn stream_until_idle(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut total_messages = 0;
        let mut last_tool_count = 0;

        loop {
            self.session.sync_state().await;

            let new_tools = self.get_new_tools(last_tool_count);
            let total_tools = last_tool_count + new_tools.len();
            for (topic, content) in &new_tools {
                let preview = truncate_tool_log(content.lines().next().unwrap_or("").trim());
                let display_preview = if preview.is_empty() {
                    "[no content]".to_string()
                } else {
                    preview
                };
                println!(
                    "{} {}",
                    log_prefix(self.test_id),
                    format!("[tool-call] {} => {}", topic, display_preview).magenta()
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
                    "{} {}",
                    log_prefix(self.test_id),
                    format!(
                        "[streaming] {:?} messages={} tools={}: {}",
                        start.elapsed(),
                        total_messages,
                        total_tools,
                        tool_list
                    )
                    .bright_black()
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
                    "{} {}",
                    log_prefix(self.test_id),
                    format!(
                        "⚠️ timeout waiting for agent (is_processing={}, has_streaming={}, messages={})",
                        is_processing,
                        has_streaming,
                        self.session.messages.len()
                    )
                    .yellow()
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
        // Include system messages (unlike to_rig_messages which filters them out)
        let filtered: Vec<ChatMessage> = self
            .session
            .messages
            .iter()
            .filter(|msg| !msg.content.trim().is_empty())
            .cloned()
            .collect();
        // Convert directly to preserve system messages for eval purposes
        filtered.into_iter().map(Message::from).collect()
    }

    pub fn rounds(&self) -> &[RoundResult] {
        &self.rounds
    }
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(default)
}

fn truncate_middle(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    if max_len <= 10 {
        return value.chars().take(max_len).collect();
    }
    let head_len = max_len / 2 - 3;
    let tail_len = max_len - head_len - 3;
    let head: String = value.chars().take(head_len).collect();
    let tail: String = value
        .chars()
        .rev()
        .take(tail_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{head}...{tail}")
}

fn has_streaming_messages(messages: &[ChatMessage]) -> bool {
    messages.iter().any(|m| m.is_streaming)
}

fn env_flag_enabled(var: &str) -> bool {
    std::env::var(var)
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes")
        })
        .unwrap_or(false)
}

fn should_truncate_tool_stream(topic: &str) -> bool {
    let topic = topic.to_ascii_lowercase();
    topic.contains("abi") || topic.contains("contract")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_history_mentions_eval_accounts() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let history = match runtime.block_on(default_session_history()) {
            Ok(h) => h,
            Err(e) => {
                // Skip test if Anvil is not available
                if e.to_string().contains("Anvil") {
                    eprintln!("Skipping: Anvil not available - {}", e);
                    return;
                }
                panic!("session history: {}", e);
            }
        };
        let alice = alice_address();
        let bob = bob_address();

        assert!(
            history[0].content.contains(alice),
            "Alice address should appear in the initial system message"
        );
        assert!(
            history.last().unwrap().content.contains(bob),
            "Bob address should appear in the evaluation instructions"
        );
    }
}
