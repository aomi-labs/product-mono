use std::{collections::HashSet, sync::Arc};

use anyhow::{Context, Result, anyhow};
use aomi_backend::session::BackendwithTool;
use aomi_chat::prompts::PromptSection;
use aomi_chat::{ChatAppBuilder, prompts::agent_preamble_builder};
use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};

use crate::assertions::{
    Assertion, AssertionPlan, AssertionResult, BalanceAsset, BalanceChange, BalanceCheck,
    DEFAULT_ASSERTION_NETWORK,
};
use crate::eval_app::{EvaluationApp, ExpectationVerdict};
use crate::{EvalState, RoundResult, TestResult};
use aomi_tools::clients::{CastClient, external_clients};

const NETWORK_ENV: &str = "CHAIN_NETWORK_URLS_JSON";
const DEFAULT_NETWORKS: &str = r#"{"testnet":"http://127.0.0.1:8545"}"#;
const SUMMARY_INTENT_WIDTH: usize = 48;
pub(crate) const LOCAL_WALLET_AUTOSIGN_ENV: &str = "LOCAL_TEST_WALLET_AUTOSIGN";

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

fn ensure_anvil_network_configured() {
    if std::env::var_os(NETWORK_ENV).is_some() {
        return;
    }

    tracing::info!(
        "Setting {} to default local Anvil endpoint for evaluation runs",
        NETWORK_ENV
    );
    unsafe {
        // SAFETY: writing a simple ASCII value into the process environment for tests
        std::env::set_var(NETWORK_ENV, DEFAULT_NETWORKS);
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
    pub backend: Arc<BackendwithTool>,
    pub cases: Vec<EvalCase>,
    pub eval_states: DashMap<usize, EvalState>,
    pub max_round: usize,
    assertion_network: String,
    case_assertions: Vec<Vec<Box<dyn Assertion>>>,
}

impl Harness {
    pub fn new(
        eval_app: EvaluationApp,
        backend: Arc<BackendwithTool>,
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
        ensure_anvil_network_configured();
        enable_local_wallet_autosign();
        let eval_app = EvaluationApp::headless().await?;

        // Add Alice and Bob account context to the agent preamble for eval tests
        let agent_preamble = agent_preamble_builder().section(PromptSection::titled("Network id and connected accounts").paragraph("User connected wallet with address 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 on the `ethereum` network (chain id 31337).")).build();
        let chat_app_builder = ChatAppBuilder::new(&agent_preamble)
            .await
            .map_err(|err| anyhow!(err))?;
        let chat_app = chat_app_builder
            .build(true, None)
            .await
            .map_err(|err| anyhow!(err))?;
        let backend = Arc::new(chat_app);

        Self::new(eval_app, backend, cases, max_round)
    }

    pub async fn default(intents: Vec<String>, max_round: usize) -> Result<Self> {
        let cases = intents.into_iter().map(EvalCase::from).collect();
        Self::default_with_cases(cases, max_round).await
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

        let total_cases = self.cases.len();
        let mut test_queue = FuturesUnordered::new();
        let mut intent_queue = FuturesUnordered::new();
        let mut active_tests: HashSet<usize> = (0..total_cases).collect::<HashSet<usize>>();
        let mut completed_results: Vec<TestResult> = Vec::with_capacity(total_cases);

        for (test_id, case) in self.cases.iter().enumerate() {
            test_queue.push(self.process_intent(test_id, case.intent.clone()));
        }
        loop {
            tokio::select! {
                Some(test) = test_queue.next() => {
                    let (test_id, next_round) = test?;
                    if !next_round {
                        self.finish_test(test_id, &mut active_tests, &mut completed_results)?;
                    } else {
                        intent_queue.push(self.generate_intent(test_id));
                    }
                }
                Some(intent) = intent_queue.next() => {
                    let (test_id, next_prompt) = intent?;
                    if let Some(prompt) = next_prompt {
                        test_queue.push(self.process_intent(test_id, prompt.clone()));
                    } else {
                        self.finish_test(test_id, &mut active_tests, &mut completed_results)?;
                    }
                }
                // Exit when both queues are empty
                else => {
                    break;
                }
            }
        }

        for test_id in active_tests.into_iter() {
            tracing::debug!(
                test_id,
                "Finalizing test without explicit completion signal"
            );
            let result = self.snapshot_test_result(test_id)?;
            completed_results.push(result);
        }

        completed_results.sort_by_key(|result| result.test_id);
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
        let clients = external_clients().await;
        clients
            .get_cast_client(&self.assertion_network)
            .await
            .context("failed to initialize cast client for assertions")
            .map_err(|err| anyhow!(err))
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

    fn finish_test(
        &self,
        test_id: usize,
        active_tests: &mut HashSet<usize>,
        completed_results: &mut Vec<TestResult>,
    ) -> Result<()> {
        if !active_tests.remove(&test_id) {
            return Ok(());
        }

        let result = self.snapshot_test_result(test_id)?;
        tracing::info!(
            test_id,
            rounds = result.rounds.len(),
            "Completed evaluation test"
        );
        completed_results.push(result);
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
