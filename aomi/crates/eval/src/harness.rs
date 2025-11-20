use std::{collections::HashSet, sync::Arc};

use anyhow::{Result, anyhow};
use aomi_backend::session::BackendwithTool;
use aomi_chat::prompts::PromptSection;
use aomi_chat::{ChatAppBuilder, prompts::agent_preamble_builder};
use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};

use crate::eval_app::{EvaluationApp, ExpectationVerdict};
use crate::{EvalState, RoundResult, TestResult};

const NETWORK_ENV: &str = "CHAIN_NETWORK_URLS_JSON";
const DEFAULT_NETWORKS: &str = r#"{"testnet":"http://127.0.0.1:8545"}"#;
const SUMMARY_INTENT_WIDTH: usize = 48;
pub(crate) const LOCAL_WALLET_AUTOSIGN_ENV: &str = "LOCAL_TEST_WALLET_AUTOSIGN";

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

pub struct Harness {
    pub eval_app: Arc<EvaluationApp>,
    pub backend: Arc<BackendwithTool>,
    pub intents: Vec<String>,
    pub eval_states: DashMap<usize, EvalState>,
    pub max_round: usize,
}

impl Harness {
    pub fn new(
        eval_app: EvaluationApp,
        backend: Arc<BackendwithTool>,
        intents: Vec<String>,
        max_round: usize,
    ) -> Result<Self> {
        Ok(Self {
            eval_app: Arc::new(eval_app),
            backend,
            intents,
            eval_states: DashMap::new(),
            max_round,
        })
    }

    pub async fn default(intents: Vec<String>, max_round: usize) -> Result<Self> {
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

        Self::new(eval_app, backend, intents, max_round)
    }

    pub fn max_round(&self) -> usize {
        self.max_round
    }

    pub fn intents(&self) -> &[String] {
        &self.intents
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
        let intent = &self.intents[test_id];
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
        if self.intents.is_empty() {
            return Err(anyhow!("no intents to start"));
        }
        let mut test_queue = FuturesUnordered::new();
        let mut intent_queue = FuturesUnordered::new();
        let mut active_tests: HashSet<usize> = (0..self.intents.len()).collect::<HashSet<usize>>();
        let mut completed_results: Vec<TestResult> = Vec::with_capacity(self.intents.len());

        for (test_id, intent) in self.intents.iter().enumerate() {
            test_queue.push(self.process_intent(test_id, intent.clone()));
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
        let mut results = Vec::with_capacity(self.intents.len());
        for test_id in 0..self.intents.len() {
            results.push(self.snapshot_test_result(test_id)?);
        }
        Ok(results)
    }

    pub async fn verify_expectations(&self, expectations: &[&str]) -> Result<Vec<bool>> {
        if expectations.len() != self.intents.len() {
            return Err(anyhow!(
                "expectations count {} does not match intents {}",
                expectations.len(),
                self.intents.len()
            ));
        }

        let mut verdicts = Vec::with_capacity(self.intents.len());
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

        for test_id in 0..self.intents.len() {
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
            .intents
            .get(test_id)
            .ok_or_else(|| anyhow!("invalid test_id {}", test_id))?
            .clone();

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
