use std::{
    str::FromStr,
    sync::{Arc, OnceLock},
    time::Instant,
};

use alloy_network_primitives::ReceiptResponse;
use alloy_primitives::B256;
use alloy_provider::Provider;
use anyhow::{Context, Result, anyhow, bail};
use aomi_anvil::default_endpoint;
use aomi_backend::{
    ChatMessage, MessageSender,
    session::{AomiBackend, DefaultSessionState},
};
use aomi_core::{Message, SystemEvent};
use aomi_tools::{
    cast::{SendTransactionParameters, execute_send_transaction},
    clients,
};
use chrono::Utc;
use colored::{ColoredString, Colorize};
use serde::Deserialize;
use serde_json;
use tokio::time::{Duration, sleep};

use crate::{
    AgentAction, RoundResult, eval_app::EVAL_ACCOUNTS, harness::LOCAL_WALLET_AUTOSIGN_ENV,
    truncate_tool_log,
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);
const ANVIL_CHAIN_ID: u64 = 1;
const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

const AUTOSIGN_NETWORK_KEY: &str = "ethereum";
const AUTOSIGN_POLL_INTERVAL: Duration = Duration::from_millis(250);
const AUTOSIGN_RECEIPT_TIMEOUT: Duration = Duration::from_secs(20);
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

fn autosign_from_account() -> &'static str {
    EVAL_ACCOUNTS
        .first()
        .map(|(_, address)| *address)
        .unwrap_or(ZERO_ADDRESS)
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
    let alice = EVAL_ACCOUNTS
        .first()
        .map(|(_, address)| *address)
        .unwrap_or(ZERO_ADDRESS);
    let bob = EVAL_ACCOUNTS
        .get(1)
        .map(|(_, address)| *address)
        .unwrap_or(ZERO_ADDRESS);
    let rpc_url = default_endpoint().await?;

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
    wallet_autosign_enabled: bool,
}

impl EvalState {
    /// Bootstraps a fresh agent session that can be used for scripted evaluations.
    pub async fn new(test_id: usize, backend: Arc<AomiBackend>, max_round: usize) -> Result<Self> {
        init_color_output();
        let session = DefaultSessionState::new(backend, default_session_history().await?)
            .await
            .context("failed to initialize eval session")?;
        Ok(Self {
            test_id,
            session,
            rounds: Vec::new(),
            current_round: 0,
            max_round,
            wallet_autosign_enabled: env_flag_enabled(LOCAL_WALLET_AUTOSIGN_ENV),
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

    fn take_wallet_request(&mut self) -> Result<Option<WalletTransactionRequest>> {
        let mut wallet_request = None;
        let mut remaining_events = Vec::new();

        for event in self.session.advance_http_events() {
            match event {
                SystemEvent::InlineCall(payload)
                    if payload.get("type").and_then(|v| v.as_str())
                        == Some("wallet_tx_request")
                        && wallet_request.is_none() =>
                {
                    let request_value = payload
                        .get("payload")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    wallet_request = Some(parse_wallet_transaction_request_value(&request_value)?);
                }
                other => remaining_events.push(other),
            }
        }

        for event in remaining_events {
            self.session.system_event_queue.push(event);
        }

        Ok(wallet_request)
    }

    async fn autosign_wallet_requests(&mut self) -> Result<()> {
        if !self.wallet_autosign_enabled {
            return Ok(());
        }

        let Some(request) = self.take_wallet_request()? else {
            return Ok(());
        };

        println!(
            "{} {}",
            log_prefix(self.test_id),
            format!(
                "🤖 Auto-signing transaction to {} (value: {})",
                request.to, request.value
            )
            .magenta()
        );

        let tx_hash = self
            .submit_wallet_transaction(&request)
            .await
            .context("failed to submit wallet transaction")?;

        let confirmation = format!("Transaction sent: {}", tx_hash);
        // Notify the agent so it does not keep re-requesting the same wallet action.
        self.session
            .send_ui_event(confirmation)
            .await
            .context("failed to deliver auto-sign confirmation to agent")?;

        // Add the transaction confirmation to the system message history for evaluation
        let transaction_confirmation =
            format!("Transaction confirmed on-chain (hash: {})", tx_hash);
        let _ = self.session.send_ui_event(transaction_confirmation).await;

        println!(
            "{} {}",
            log_prefix(self.test_id),
            format!("✅ Transaction confirmed on-chain (hash: {})", tx_hash).green()
        );
        Ok(())
    }

    async fn submit_wallet_transaction(
        &self,
        request: &WalletTransactionRequest,
    ) -> Result<String> {
        let from = autosign_from_account().to_string();
        let value = if request.value.trim().is_empty() {
            "0".to_string()
        } else {
            request.value.clone()
        };
        let calldata = validate_calldata(&request.data).map_err(|err| {
            anyhow!(
                "invalid wallet transaction calldata (to={}, value={}): {}",
                request.to,
                request.value,
                err
            )
        })?;
        let data_len = calldata.as_ref().map(|d| d.len()).unwrap_or(0);
        let data_preview = calldata
            .as_ref()
            .map(|d| {
                if d.len() > 66 {
                    format!("{}…", &d[..66])
                } else {
                    d.clone()
                }
            })
            .unwrap_or_else(|| "None".to_string());

        println!(
            "{} autosign submit params: from={}, to={}, value={}, data_len={}, data_preview={}, network={}",
            log_prefix(self.test_id),
            from,
            request.to,
            value,
            data_len,
            data_preview,
            AUTOSIGN_NETWORK_KEY
        );

        let params = SendTransactionParameters {
            from,
            to: request.to.clone(),
            value,
            input: calldata,
            network: Some(AUTOSIGN_NETWORK_KEY.to_string()),
        };

        let tx_hash = execute_send_transaction(params)
            .await
            .map_err(|err| {
                anyhow!(
                    "wallet auto-sign failed (to={}, value={}, data_len={}, data_preview={}, network={}): {}",
                    request.to,
                    request.value,
                    data_len,
                    data_preview,
                    AUTOSIGN_NETWORK_KEY,
                    err
                )
            })?;
        wait_for_transaction_confirmation(&tx_hash).await?;
        Ok(tx_hash)
    }

    async fn stream_until_idle(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut total_messages = 0;
        let mut last_tool_count = 0;

        loop {
            self.session.sync_state().await;
            if let Err(err) = self.autosign_wallet_requests().await {
                let detailed_error = format_error_chain(&err);
                println!(
                    "{} {}",
                    log_prefix(self.test_id),
                    format!("⚠️ auto-sign wallet flow failed: {}", detailed_error).yellow()
                );
                let _ = self
                    .session
                    .send_ui_event(format!("Transaction rejected by user: {}", detailed_error))
                    .await;
            }

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

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WalletTransactionRequest {
    to: String,
    value: String,
    data: String,
    #[serde(default)]
    description: Option<String>,
}

fn parse_wallet_transaction_request_value(
    payload: &serde_json::Value,
) -> Result<WalletTransactionRequest> {
    if payload.is_null() || payload.is_boolean() {
        bail!("missing wallet transaction payload");
    }

    if let Some(nested) = payload.get("wallet_transaction_request") {
        return serde_json::from_value::<WalletTransactionRequest>(nested.clone())
            .map_err(|err| anyhow!("invalid wallet transaction payload: {}", err));
    }

    serde_json::from_value::<WalletTransactionRequest>(payload.clone())
        .map_err(|err| anyhow!("invalid wallet transaction payload: {}", err))
}

fn validate_calldata(data: &str) -> Result<Option<String>> {
    let trimmed = data.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
        return Ok(None);
    }

    let normalized = if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        trimmed.to_string()
    } else {
        format!("0x{}", trimmed)
    };

    let hex = normalized
        .strip_prefix("0x")
        .or_else(|| normalized.strip_prefix("0X"))
        .unwrap_or("");
    if hex.is_empty() {
        return Ok(None);
    }
    if hex.len() % 2 != 0 {
        bail!(
            "calldata hex has odd length (len={}): raw='{}' normalized='{}'",
            hex.len(),
            data,
            normalized
        );
    }
    if let Some((idx, ch)) = hex.char_indices().find(|(_, ch)| !ch.is_ascii_hexdigit()) {
        bail!(
            "calldata hex contains non-hex character '{}' at index {}: raw='{}' normalized='{}'",
            ch,
            idx,
            data,
            normalized
        );
    }

    Ok(Some(normalized))
}

async fn wait_for_transaction_confirmation(tx_hash: &str) -> Result<()> {
    let clients = clients::external_clients().await;
    let cast_client = clients
        .get_cast_client(AUTOSIGN_NETWORK_KEY)
        .await
        .map_err(|err| anyhow!("failed to get cast client for auto-sign network: {}", err))?;
    let hash = B256::from_str(tx_hash)
        .map_err(|err| anyhow!("invalid transaction hash '{tx_hash}': {err}"))?;
    let start = Instant::now();
    loop {
        match cast_client.provider.get_transaction_receipt(hash).await {
            Ok(Some(receipt)) => {
                if !receipt.status() {
                    bail!("transaction reverted on-chain");
                }
                return Ok(());
            }
            Ok(None) => {
                if start.elapsed() > AUTOSIGN_RECEIPT_TIMEOUT {
                    bail!("timed out waiting for transaction receipt");
                }
                sleep(AUTOSIGN_POLL_INTERVAL).await;
            }
            Err(err) => {
                bail!("failed to poll transaction receipt: {}", err);
            }
        }
    }
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

fn format_error_chain(err: &anyhow::Error) -> String {
    let mut parts = Vec::new();
    for (idx, cause) in err.chain().enumerate() {
        if idx == 0 {
            parts.push(cause.to_string());
        } else {
            parts.push(format!("caused by: {}", cause));
        }
    }
    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autosign_uses_alice_address() {
        let expected = EVAL_ACCOUNTS
            .first()
            .map(|(_, address)| *address)
            .unwrap_or(ZERO_ADDRESS);
        assert_eq!(autosign_from_account(), expected);
    }

    #[test]
    fn session_history_mentions_eval_accounts() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let history = runtime
            .block_on(default_session_history())
            .expect("session history");
        let alice = EVAL_ACCOUNTS
            .first()
            .map(|(_, address)| *address)
            .unwrap_or(ZERO_ADDRESS);
        let bob = EVAL_ACCOUNTS
            .get(1)
            .map(|(_, address)| *address)
            .unwrap_or(ZERO_ADDRESS);

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
