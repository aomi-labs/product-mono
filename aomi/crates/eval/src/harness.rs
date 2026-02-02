use std::sync::Arc;

use alloy_network_primitives::ReceiptResponse;
use alloy_primitives::{Address, B256, U256};
use alloy_provider::Provider;
use alloy_sol_types::{SolCall, sol};
use anyhow::{Context, Result, anyhow, bail};
use aomi_anvil::ethereum_endpoint;
use aomi_backend::session::AomiBackend;
use aomi_baml::AomiModel;
use aomi_core::prompts::PromptSection;
use aomi_core::{
    BuildOpts, CoreAppBuilder, Selection, SystemEventQueue, prompts::preamble_builder,
};
use dashmap::DashMap;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::sync::OnceCell;
use tokio::time::sleep;

use crate::assertions::{
    Assertion, AssertionPlan, AssertionResult, BalanceAsset, BalanceChange, BalanceCheck,
    DEFAULT_ASSERTION_NETWORK,
};
use crate::eval_app::{EVAL_ACCOUNTS, EvaluationApp, ExpectationVerdict};
use crate::{EvalState, RoundResult, TestResult};
use aomi_tools::clients::{CastClient, external_clients};

const SUMMARY_INTENT_WIDTH: usize = 48;
pub(crate) const LOCAL_WALLET_AUTOSIGN_ENV: &str = "LOCAL_TEST_WALLET_AUTOSIGN";

async fn configure_eval_network() -> anyhow::Result<()> {
    let endpoint = ethereum_endpoint().await?;

    let mut networks = std::collections::HashMap::new();
    networks.insert("ethereum".to_string(), endpoint.clone());
    let clients = aomi_tools::clients::ExternalClients::new_with_networks(
        networks,
        aomi_baml::AomiModel::ClaudeOpus4,
    )
    .await;
    aomi_tools::clients::init_external_clients(std::sync::Arc::new(clients)).await;
    Ok(())
}

const USDC_CONTRACT: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const USDC_WHALE: &str = "0x55fe002aeff02f77364de339a1292923a15844b8";
const USDC_PREFUND_AMOUNT: u64 = 2_000 * 1_000_000; // 2,000 USDC with 6 decimals
const USDC_GAS_LIMIT: u64 = 300_000;
const USDC_PREFUND_RECEIPT_TIMEOUT: Duration = Duration::from_secs(20);
const WHALE_GAS_TOPUP_WEI: u128 = 10u128.pow(20); // 100 ETH for gas when impersonated
static USDC_PREFUND_ONCE: OnceCell<()> = OnceCell::const_new();

#[derive(Debug, Clone)]
pub struct EvalCase {
    pub intent: String,
    pub expectation: Option<String>,
    pub assertions: Vec<AssertionPlan>,
}

impl EvalCase {
    pub fn new(intent: impl Into<String>) -> Self {
        Self {
            intent: intent.into(),
            expectation: None,
            assertions: Vec::new(),
        }
    }

    pub fn with_expectation(mut self, expectation: impl Into<String>) -> Self {
        self.expectation = Some(expectation.into());
        self
    }

    pub fn with_assertion(mut self, assertion: AssertionPlan) -> Self {
        self.assertions.push(assertion);
        self
    }

    pub fn with_balance_change(mut self, change: BalanceChange) -> Self {
        self.assertions.push(AssertionPlan::BalanceChange(change));
        self
    }

    pub fn with_balance_check(mut self, check: BalanceCheck) -> Self {
        self.assertions.push(AssertionPlan::BalanceCheck(check));
        self
    }

    pub fn with_balance_at_least(
        mut self,
        holder: impl Into<String>,
        asset: BalanceAsset,
        min_units: u128,
        label: impl Into<String>,
    ) -> Self {
        self.assertions.push(AssertionPlan::BalanceAtLeast {
            holder: holder.into(),
            asset,
            min_units,
            label: label.into(),
        });
        self
    }

    pub fn with_balance_change_at_least(mut self, change: BalanceChange) -> Self {
        self.assertions
            .push(AssertionPlan::BalanceDeltaAtLeast(change));
        self
    }

    pub fn with_balance_change_at_most(mut self, change: BalanceChange) -> Self {
        self.assertions
            .push(AssertionPlan::BalanceDeltaAtMost(change));
        self
    }
}

impl From<String> for EvalCase {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for EvalCase {
    fn from(value: &str) -> Self {
        Self::new(value.to_string())
    }
}

fn enable_local_wallet_autosign() {
    if std::env::var_os(LOCAL_WALLET_AUTOSIGN_ENV).is_some() {
        return;
    }
    unsafe {
        std::env::set_var(LOCAL_WALLET_AUTOSIGN_ENV, "true");
    }
}

async fn anvil_rpc<T: DeserializeOwned>(
    client: &reqwest::Client,
    method: &str,
    params: serde_json::Value,
) -> Result<T> {
    let endpoint = ethereum_endpoint().await?;
    let response = client
        .post(endpoint)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": format!("eval-{method}"),
            "method": method,
            "params": params,
        }))
        .send()
        .await
        .with_context(|| format!("failed to call {method} on Anvil"))?;

    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .with_context(|| format!("invalid JSON-RPC response for {method} (status {status})"))?;

    if let Some(error) = body.get("error") {
        bail!("{method} failed: {error}");
    }

    let result = body
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    serde_json::from_value::<T>(result)
        .with_context(|| format!("unexpected result shape for {method}"))
}

async fn wait_for_prefund_receipt(tx_hash: &str) -> Result<()> {
    let clients = external_clients().await;
    let cast_client = clients
        .get_cast_client(DEFAULT_ASSERTION_NETWORK)
        .await
        .context("failed to get cast client for USDC prefund receipt")?;
    let hash = B256::from_str(tx_hash)
        .with_context(|| format!("invalid prefund transaction hash '{tx_hash}'"))?;
    let start = Instant::now();

    loop {
        match cast_client.provider.get_transaction_receipt(hash).await {
            Ok(Some(receipt)) => {
                if !receipt.status() {
                    bail!("USDC prefund transaction reverted");
                }
                return Ok(());
            }
            Ok(None) => {
                if start.elapsed() > USDC_PREFUND_RECEIPT_TIMEOUT {
                    bail!("timed out waiting for USDC prefund receipt");
                }
                sleep(Duration::from_millis(250)).await;
            }
            Err(err) => bail!("failed to poll USDC prefund receipt: {}", err),
        }
    }
}

async fn fund_alice_with_usdc() -> Result<()> {
    if USDC_PREFUND_ONCE.get().is_some() {
        return Ok(());
    }

    USDC_PREFUND_ONCE
        .get_or_try_init(|| async {
            let alice = EVAL_ACCOUNTS
                .first()
                .map(|(_, address)| *address)
                .ok_or_else(|| anyhow!("missing Alice address for USDC prefund"))?;
            println!("Prefunding Alice ({alice}) with 2,000 USDC via impersonated whale...");

            sol! {
                function transfer(address to, uint256 amount) returns (bool);
            }

            let client = reqwest::Client::new();

            anvil_rpc::<serde_json::Value>(
                &client,
                "anvil_impersonateAccount",
                json!([USDC_WHALE]),
            )
            .await
            .context("failed to impersonate USDC whale")?;

            let gas_balance_hex = format!("0x{:x}", WHALE_GAS_TOPUP_WEI);
            anvil_rpc::<serde_json::Value>(
                &client,
                "anvil_setBalance",
                json!([USDC_WHALE, gas_balance_hex]),
            )
            .await
            .context("failed to top up impersonated whale with gas")?;

            let calldata = transferCall {
                to: Address::from_str(alice).context("invalid Alice address for USDC prefund")?,
                amount: U256::from(USDC_PREFUND_AMOUNT),
            }
            .abi_encode();

            let tx_hash: Result<String> = anvil_rpc(
                &client,
                "eth_sendTransaction",
                json!([{
                    "from": USDC_WHALE,
                    "to": USDC_CONTRACT,
                    "data": format!("0x{}", hex::encode(calldata)),
                    "gas": format!("0x{:x}", USDC_GAS_LIMIT),
                }]),
            )
            .await
            .context("failed to submit USDC prefund transaction");

            // Always stop impersonating to avoid leaking unlocked accounts into tests.
            let _ = anvil_rpc::<serde_json::Value>(
                &client,
                "anvil_stopImpersonatingAccount",
                json!([USDC_WHALE]),
            )
            .await;

            let tx_hash = tx_hash?;
            wait_for_prefund_receipt(&tx_hash).await?;
            println!("USDC prefund complete (tx: {tx_hash})");
            Ok(())
        })
        .await
        .map(|_| ())
}

fn build_case_assertions(cases: &[EvalCase]) -> Result<Vec<Vec<Box<dyn Assertion>>>> {
    let mut built: Vec<Vec<Box<dyn Assertion>>> = Vec::with_capacity(cases.len());
    for (test_id, case) in cases.iter().enumerate() {
        let mut case_assertions = Vec::with_capacity(case.assertions.len());
        for plan in &case.assertions {
            case_assertions.push(plan.clone().into_assertion(test_id)?);
        }
        built.push(case_assertions);
    }
    Ok(built)
}

pub struct Harness {
    pub eval_app: Arc<EvaluationApp>,
    pub backend: Arc<AomiBackend>,
    pub cases: Vec<EvalCase>,
    pub eval_states: DashMap<usize, EvalState>,
    pub max_round: usize,
    #[allow(dead_code)]
    assertion_network: String,
    case_assertions: Vec<Vec<Box<dyn Assertion>>>,
}

impl Harness {
    pub fn new(
        eval_app: EvaluationApp,
        backend: Arc<AomiBackend>,
        cases: Vec<EvalCase>,
        max_round: usize,
    ) -> Result<Self> {
        let case_assertions = build_case_assertions(&cases)?;
        Ok(Self {
            eval_app: Arc::new(eval_app),
            backend,
            cases,
            eval_states: DashMap::new(),
            max_round,
            assertion_network: DEFAULT_ASSERTION_NETWORK.to_string(),
            case_assertions,
        })
    }

    pub async fn default_with_cases(cases: Vec<EvalCase>, max_round: usize) -> Result<Self> {
        configure_eval_network().await?;

        enable_local_wallet_autosign();
        fund_alice_with_usdc().await?;
        let eval_app = EvaluationApp::headless().await?;

        // Add Alice and Bob account context to the agent preamble for eval tests
        let prompt = preamble_builder()
            .await
            .section(PromptSection::titled("Network id and connected accounts")
            .paragraph("User connected wallet with address 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 on the `ethereum` network (chain id 1)."))
            .section(PromptSection::titled("ERC20 token").paragraph("Make sure to find out the right decimals for the ERC20 token when calculating the ERC20 token balances."))
            .section(PromptSection::titled("Swap").paragraph("Always derive token amounts and mins from on-chain reserves; do not hardcode slippage. Always rebuild calldata with deadline = now + 10–15 minutes immediately before sending."))
            .build();
        let system_events = SystemEventQueue::new();
        let opts = BuildOpts {
            no_docs: true,
            skip_mcp: true,
            no_tools: false,
            selection: Selection {
                rig: AomiModel::ClaudeOpus4,
                baml: AomiModel::ClaudeOpus4,
            },
        };
        let chat_app_builder = CoreAppBuilder::new(&prompt, opts, None)
            .await
            .map_err(|err| anyhow!(err))?;
        let chat_app = chat_app_builder
            .build(opts, Some(&system_events))
            .await
            .map_err(|err| anyhow!(err))?;
        let backend = Arc::new(chat_app);

        Self::new(eval_app, backend, cases, max_round)
    }

    pub async fn default(intents: Vec<String>, max_round: usize) -> Result<Self> {
        let cases = intents.into_iter().map(EvalCase::from).collect();
        Self::default_with_cases(cases, max_round).await
    }

    /// Create a harness with ForgeApp backend for scripter/forge tests.
    /// Unlike default_with_cases, this:
    /// - Uses ForgeApp instead of ChatApp
    /// - Does NOT prefund USDC (scripts are simulated, not broadcast)
    /// - Uses forge-specific preamble
    pub async fn for_scripter(cases: Vec<EvalCase>, max_round: usize) -> Result<Self> {
        configure_eval_network().await?;
        // Note: No USDC prefund - forge scripts are simulated, not executed on-chain

        let eval_app = EvaluationApp::headless().await?;

        // Use ForgeApp instead of ChatApp
        let forge_app = aomi_forge::ForgeApp::new(BuildOpts {
            no_docs: true,
            skip_mcp: true,
            no_tools: false,
            selection: Selection {
                rig: AomiModel::ClaudeOpus4,
                baml: AomiModel::ClaudeOpus4,
            },
        })
        .await
        .map_err(|e| anyhow!("Failed to create ForgeApp: {}", e))?;

        let backend: Arc<AomiBackend> = Arc::new(forge_app);

        Self::new(eval_app, backend, cases, max_round)
    }

    pub fn max_round(&self) -> usize {
        self.max_round
    }

    pub fn cases(&self) -> &[EvalCase] {
        &self.cases
    }

    pub fn case(&self, index: usize) -> Option<&EvalCase> {
        self.cases.get(index)
    }

    pub fn case_count(&self) -> usize {
        self.cases.len()
    }

    pub fn intents(&self) -> Vec<String> {
        self.cases.iter().map(|case| case.intent.clone()).collect()
    }

    pub async fn process_intent(&self, test_id: usize, intent: String) -> Result<(usize, bool)> {
        let trimmed = intent.trim().to_string();
        if trimmed.is_empty() {
            return Ok((test_id, false));
        }
        let next_round = if let Some(mut eval_state) = self.eval_states.get_mut(&test_id) {
            eval_state.run_round(&trimmed).await?
        } else {
            let mut eval_state =
                EvalState::new(test_id, self.backend.clone(), self.max_round).await?;
            let need_next_round = eval_state.run_round(&trimmed).await?;
            self.eval_states.insert(test_id, eval_state);
            need_next_round
        };
        Ok((test_id, next_round))
    }

    pub async fn generate_intent(&self, test_id: usize) -> Result<(usize, Option<String>)> {
        let eval_state = self.eval_states.get(&test_id).unwrap();
        let mut history = eval_state.messages();
        let intent = &self.cases[test_id].intent;
        self.eval_app
            .next_eval_prompt(
                &mut history,
                intent,
                eval_state.rounds().len(),
                self.max_round,
            )
            .await
            .map(|next_prompt| (test_id, next_prompt))
    }

    pub async fn run_suites(&self) -> Result<Vec<TestResult>> {
        if self.cases.is_empty() {
            return Err(anyhow!("no test cases to start"));
        }

        self.ensure_assertion_snapshots().await?;

        let mut completed_results: Vec<TestResult> = Vec::with_capacity(self.cases.len());

        for (test_id, case) in self.cases.iter().enumerate() {
            let mut next_round = self.process_intent(test_id, case.intent.clone()).await?.1;

            while next_round {
                let (_, next_prompt) = self.generate_intent(test_id).await?;
                if let Some(prompt) = next_prompt {
                    next_round = self.process_intent(test_id, prompt.clone()).await?.1;
                } else {
                    next_round = false;
                }
            }

            let result = self.snapshot_test_result(test_id)?;
            tracing::info!(
                test_id,
                rounds = result.rounds.len(),
                "Completed evaluation test"
            );
            completed_results.push(result);
        }

        Ok(completed_results)
    }

    pub fn get_rounds(&self, test_id: usize) -> Result<Vec<RoundResult>> {
        self.eval_states
            .get(&test_id)
            .map(|state| state.rounds().to_vec())
            .ok_or_else(|| anyhow!("no eval state found for test_id {}", test_id))
    }

    pub fn result_for(&self, test_id: usize) -> Result<TestResult> {
        self.snapshot_test_result(test_id)
    }

    pub fn results(&self) -> Result<Vec<TestResult>> {
        let mut results = Vec::with_capacity(self.cases.len());
        for test_id in 0..self.cases.len() {
            results.push(self.snapshot_test_result(test_id)?);
        }
        Ok(results)
    }

    pub async fn verify_expectations(&self, expectations: &[&str]) -> Result<Vec<bool>> {
        if expectations.len() != self.cases.len() {
            return Err(anyhow!(
                "expectations count {} does not match intents {}",
                expectations.len(),
                self.cases.len()
            ));
        }

        let mut verdicts = Vec::with_capacity(self.cases.len());
        for (test_id, expectation) in expectations.iter().enumerate() {
            let mut history = self
                .eval_states
                .get(&test_id)
                .ok_or_else(|| anyhow!("missing eval state for test_id {}", test_id))?
                .messages();
            let verdict = self
                .eval_app
                .judge_expectation(&mut history, expectation)
                .await?;
            log_expectation_verdict(test_id, expectation, &verdict);
            verdicts.push(verdict.satisfied);
        }

        Ok(verdicts)
    }

    pub async fn verify_case_expectations(&self) -> Result<Vec<Option<ExpectationVerdict>>> {
        let mut verdicts = Vec::with_capacity(self.cases.len());

        for (test_id, case) in self.cases.iter().enumerate() {
            if let Some(expectation) = case.expectation.as_deref() {
                let mut history = self
                    .eval_states
                    .get(&test_id)
                    .ok_or_else(|| anyhow!("missing eval state for test_id {}", test_id))?
                    .messages();
                let verdict = self
                    .eval_app
                    .judge_expectation(&mut history, expectation)
                    .await?;
                log_expectation_verdict(test_id, expectation, &verdict);
                verdicts.push(Some(verdict));
            } else {
                verdicts.push(None);
            }
        }

        Ok(verdicts)
    }

    pub async fn verify_assertions(&self) -> Result<Vec<AssertionResult>> {
        if !self.has_assertions() {
            return Ok(Vec::new());
        }

        let client = self.cast_client().await?;
        let mut results = Vec::new();
        for assertions in &self.case_assertions {
            for assertion in assertions {
                results.push(assertion.verify(client.as_ref()).await?);
            }
        }
        Ok(results)
    }

    pub fn print_assertions(&self, results: &[AssertionResult]) {
        if results.is_empty() {
            return;
        }

        for result in results {
            let status = if result.passed { "✅" } else { "❌" };
            println!("[test {}] {status} {}", result.test_id, result.label);
            println!("          {}", result.detail);
        }
    }

    pub fn assert_assertions(&self, results: &[AssertionResult]) -> Result<()> {
        let failures: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        if failures.is_empty() {
            return Ok(());
        }

        let mut msg = String::from("deterministic assertions failed:\n");
        for failure in failures {
            msg.push_str(&format!(
                "- [test {}] {} => {}\n",
                failure.test_id, failure.label, failure.detail
            ));
        }
        Err(anyhow!(msg))
    }

    pub fn print_outcome_table(
        &self,
        expectations: &[Option<ExpectationVerdict>],
        assertions: &[AssertionResult],
    ) -> Result<()> {
        if expectations.len() != self.cases.len() {
            return Err(anyhow!(
                "expectations length {} did not match case count {}",
                expectations.len(),
                self.cases.len()
            ));
        }

        println!("{:-<80}", "");
        println!("TEST STATUS");
        println!("{:-<80}", "");
        let header = format!(
            "{:<5} │ {:<width$} │ {:<16} │ {:<40}",
            "ID",
            "Intent",
            "Assertions",
            "Expectation",
            width = SUMMARY_INTENT_WIDTH
        );
        println!("{header}");
        println!("{:-<80}", "");

        for test_id in 0..self.cases.len() {
            let intent = &self.cases[test_id].intent;
            let truncated_intent = truncate_for_table(intent, SUMMARY_INTENT_WIDTH);

            let (assert_status, assert_note) = summarize_assertions_for(test_id, assertions);
            let (expect_status, expect_note) = summarize_expectation(expectations.get(test_id));

            let assertion_cell = if assert_note.is_empty() {
                assert_status
            } else {
                format!("{assert_status}: {assert_note}")
            };

            let expectation_cell = if expect_note.is_empty() {
                expect_status
            } else {
                format!("{expect_status}: {expect_note}")
            };

            println!(
                "{:<5} │ {:<width$} │ {:<16} │ {:<40}",
                test_id,
                truncated_intent,
                assertion_cell,
                truncate_for_table(&expectation_cell, 40),
                width = SUMMARY_INTENT_WIDTH
            );
        }

        println!("{:-<80}", "");
        Ok(())
    }

    async fn ensure_assertion_snapshots(&self) -> Result<()> {
        if !self.has_assertions() {
            return Ok(());
        }

        let client = self.cast_client().await?;
        for assertions in &self.case_assertions {
            for assertion in assertions {
                assertion.snapshot(client.as_ref()).await?;
            }
        }
        Ok(())
    }

    async fn cast_client(&self) -> Result<Arc<CastClient>> {
        // Create direct connection to anvil endpoint instead of using cached global
        let endpoint = ethereum_endpoint().await?;
        let client = CastClient::connect(&endpoint)
            .await
            .map_err(|e| anyhow!("failed to connect to anvil: {}", e))?;
        Ok(Arc::new(client))
    }

    fn has_assertions(&self) -> bool {
        self.case_assertions.iter().any(|a| !a.is_empty())
    }

    /// Print the results of a single test by test_id
    pub fn flush_test(&self, test_id: usize) -> Result<()> {
        let result = self.result_for(test_id)?;

        println!("\n{:=<80}", "");
        println!("Test #{}: {}", test_id, result.intent);
        println!("{:=<80}", "");

        if result.rounds.is_empty() {
            println!("  (no rounds recorded for this intent)");
        } else {
            for (round_idx, round) in result.rounds.iter().enumerate() {
                println!("\nRound {}:", round_idx + 1);
                println!("{}", round);
            }
        }
        println!("{:=<80}\n", "");

        Ok(())
    }

    /// Print the results of all tests
    pub fn flush(&self) -> Result<()> {
        println!("\n{:=<80}", "");
        println!("EVALUATION RESULTS");
        println!("{:=<80}", "");
        self.print_summary_table()?;

        for test_id in 0..self.cases.len() {
            self.flush_test(test_id)?;
        }

        Ok(())
    }

    fn snapshot_test_result(&self, test_id: usize) -> Result<TestResult> {
        let intent = self
            .cases
            .get(test_id)
            .map(|case| case.intent.clone())
            .ok_or_else(|| anyhow!("invalid test_id {}", test_id))?;

        let rounds = self
            .eval_states
            .get(&test_id)
            .map(|state| state.rounds().to_vec())
            .unwrap_or_default();

        Ok(TestResult {
            test_id,
            intent,
            rounds,
        })
    }

    fn print_summary_table(&self) -> Result<()> {
        let results = self.results()?;
        if results.is_empty() {
            println!("(no evaluation results recorded)");
            return Ok(());
        }

        println!("{:-<80}", "");
        let header = format!(
            "{:<5} │ {:<width$} │ {:>6} │ {:>7} │ {:>9}",
            "ID",
            "Intent",
            "Rounds",
            "Tools",
            "Responses",
            width = SUMMARY_INTENT_WIDTH
        );
        println!("{header}");
        println!("{:-<80}", "");

        for result in results {
            let truncated_intent = truncate_for_table(&result.intent, SUMMARY_INTENT_WIDTH);
            println!(
                "{:<5} │ {:<width$} │ {:>6} │ {:>7} │ {:>9}",
                result.test_id,
                truncated_intent,
                result.total_rounds(),
                result.total_tool_calls(),
                result.total_responses(),
                width = SUMMARY_INTENT_WIDTH
            );
        }

        println!("{:-<80}", "");
        Ok(())
    }
}

fn truncate_for_table(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let keep = max_chars.saturating_sub(3);
    let truncated: String = text.chars().take(keep).collect();
    format!("{truncated}...")
}

fn summarize_assertions_for(test_id: usize, assertions: &[AssertionResult]) -> (String, String) {
    let relevant: Vec<&AssertionResult> = assertions
        .iter()
        .filter(|result| result.test_id == test_id)
        .collect();

    if relevant.is_empty() {
        return ("none".to_string(), String::new());
    }

    let failures: Vec<&AssertionResult> = relevant.iter().cloned().filter(|r| !r.passed).collect();
    if failures.is_empty() {
        let note = if relevant.len() == 1 {
            relevant[0].label.clone()
        } else {
            format!("{} checks", relevant.len())
        };
        ("pass".to_string(), note)
    } else {
        let first = failures[0];
        (
            format!("fail ({}/{})", failures.len(), relevant.len()),
            truncate_for_table(&first.detail, 40),
        )
    }
}

fn summarize_expectation(expectation: Option<&Option<ExpectationVerdict>>) -> (String, String) {
    match expectation {
        Some(Some(verdict)) => {
            let status = if verdict.satisfied { "YES" } else { "NO" };
            let note = truncate_for_table(&verdict.explanation, 40);
            (status.to_string(), note)
        }
        Some(None) => ("n/a".to_string(), String::new()),
        None => ("n/a".to_string(), String::new()),
    }
}

fn log_expectation_verdict(test_id: usize, expectation: &str, verdict: &ExpectationVerdict) {
    println!(
        "[test {}] expectation: {}\n          verdict: {}",
        test_id, expectation, verdict.explanation
    );
    if verdict.satisfied {
        println!("[test {}] ✅ expectation satisfied\n", test_id);
    } else {
        println!("[test {}] ❌ expectation failed\n", test_id);
    }
}
