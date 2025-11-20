use std::{str::FromStr, sync::Arc, time::Instant};

use alloy_network_primitives::ReceiptResponse;
use alloy_primitives::B256;
use alloy_provider::Provider;
use anyhow::{Context, Result, anyhow, bail};
use aomi_backend::{
    ChatMessage, MessageSender,
    session::{BackendwithTool, DefaultSessionState},
    to_rig_messages,
};
use aomi_chat::{Message, accounts::ANVIL_ACCOUNTS};
use aomi_tools::{
    cast::{SendTransactionParameters, execute_send_transaction},
    clients,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json;
use tokio::time::{Duration, sleep};

use crate::{AgentAction, RoundResult, harness::LOCAL_WALLET_AUTOSIGN_ENV};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);
const ANVIL_CHAIN_ID: u64 = 31337;
const ANVIL_RPC_URL: &str = "http://127.0.0.1:8545";
const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
const AUTOSIGN_NETWORK_KEY: &str = "testnet";
const AUTOSIGN_FROM_ACCOUNT_INDEX: usize = 0;
const AUTOSIGN_POLL_INTERVAL: Duration = Duration::from_millis(250);
const AUTOSIGN_RECEIPT_TIMEOUT: Duration = Duration::from_secs(20);

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
    wallet_autosign_enabled: bool,
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

    async fn autosign_pending_wallet_tx(&mut self) -> Result<()> {
        if !self.wallet_autosign_enabled {
            return Ok(());
        }

        let Some(raw) = self.session.pending_wallet_tx.clone() else {
            return Ok(());
        };

        let request = parse_wallet_transaction_request(&raw)?;
        println!(
            "[test {}] 🤖 Auto-signing transaction to {} (value: {})",
            self.test_id, request.to, request.value
        );

        let tx_hash = self
            .submit_wallet_transaction(&request)
            .await
            .context("failed to submit wallet transaction")?;

        self.session
            .add_system_message(&format!("Transaction sent: {}", tx_hash));

        println!(
            "[test {}] ✅ Transaction confirmed on-chain (hash: {})",
            self.test_id, tx_hash
        );
        Ok(())
    }

    async fn submit_wallet_transaction(
        &self,
        request: &WalletTransactionRequest,
    ) -> Result<String> {
        let from = account_address_or_default(AUTOSIGN_FROM_ACCOUNT_INDEX).to_string();
        let topic = request
            .description
            .clone()
            .unwrap_or_else(|| format!("send transaction to {}", request.to));
        let value = if request.value.trim().is_empty() {
            "0".to_string()
        } else {
            request.value.clone()
        };
        let calldata = normalize_calldata(&request.data);

        let params = SendTransactionParameters {
            topic,
            from,
            to: request.to.clone(),
            value,
            input: calldata,
            network: Some(AUTOSIGN_NETWORK_KEY.to_string()),
        };

        let tx_hash = execute_send_transaction(params)
            .await
            .map_err(|err| anyhow!("wallet auto-sign failed: {}", err))?;
        wait_for_transaction_confirmation(&tx_hash).await?;
        Ok(tx_hash)
    }

    async fn stream_until_idle(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut total_messages = 0;
        let mut last_tool_count = 0;

        loop {
            self.session.update_state().await;
            if let Err(err) = self.autosign_pending_wallet_tx().await {
                println!(
                    "[test {}] ⚠️ auto-sign wallet flow failed: {}",
                    self.test_id, err
                );
                self.session
                    .add_system_message(&format!("Transaction rejected by user: {}", err));
            }

            let new_tools = self.get_new_tools(last_tool_count);
            let total_tools = last_tool_count + new_tools.len();
            for (topic, content) in &new_tools {
                let preview = content.lines().next().unwrap_or("").trim();
                println!(
                    "[test {}][tool-call] {} => {}",
                    self.test_id,
                    topic,
                    if preview.is_empty() {
                        "[no content]"
                    } else {
                        preview
                    }
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

#[derive(Debug, Deserialize)]
struct WalletTransactionEnvelope {
    wallet_transaction_request: WalletTransactionRequest,
}

#[derive(Debug, Deserialize)]
struct WalletTransactionRequest {
    to: String,
    value: String,
    data: String,
    #[serde(default)]
    description: Option<String>,
}

fn parse_wallet_transaction_request(raw: &str) -> Result<WalletTransactionRequest> {
    if raw.trim().is_empty() {
        bail!("missing wallet transaction payload");
    }

    if let Ok(enveloped) = serde_json::from_str::<WalletTransactionEnvelope>(raw) {
        return Ok(enveloped.wallet_transaction_request);
    }

    serde_json::from_str::<WalletTransactionRequest>(raw)
        .map_err(|err| anyhow!("invalid wallet transaction payload: {}", err))
}

fn normalize_calldata(data: &str) -> Option<String> {
    let trimmed = data.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
        return None;
    }

    if trimmed.starts_with("0x") {
        Some(trimmed.to_string())
    } else {
        Some(format!("0x{}", trimmed))
    }
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
