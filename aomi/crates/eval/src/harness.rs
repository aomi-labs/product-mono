use std::sync::Arc;

use anyhow::{Result, anyhow};
use aomi_backend::session::BackendwithTool;
use aomi_chat::ChatApp;
use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};

use crate::eval_app::EvaluationApp;
use crate::{EvalState, RoundResult};

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
        let eval_app = EvaluationApp::headless().await?;
        let backend = Arc::new(ChatApp::new().await.map_err(|err| anyhow!(err))?);
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
        self.eval_app
            .next_eval_prompt(&mut history, eval_state.rounds().len(), self.max_round)
            .await
            .map(|next_prompt| (test_id, next_prompt))
    }

    pub async fn run_suites(&self) -> Result<()> {
        if self.intents.is_empty() {
            return Err(anyhow!("no intents to start"));
        }
        let mut test_queue = FuturesUnordered::new();
        let mut intent_queue = FuturesUnordered::new();

        for (test_id, intent) in self.intents.iter().enumerate() {
            test_queue.push(self.process_intent(test_id, intent.clone()));
        }
        loop {
            tokio::select! {
                Some(test) = test_queue.next() => {
                    let (test_id, next_round) = test?;
                    if !next_round {
                        continue;
                    } else {
                        intent_queue.push(self.generate_intent(test_id));
                    }
                }
                Some(intent) = intent_queue.next() => {
                    let (test_id, next_prompt) = intent?;
                    if let Some(prompt) = next_prompt {
                        test_queue.push(self.process_intent(test_id, prompt.clone()));
                    } else {
                        continue;
                    }
                }
                // Exit when both queues are empty
                else => {
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn get_rounds(&self, test_id: usize) -> Result<Vec<RoundResult>> {
        self.eval_states
            .get(&test_id)
            .map(|state| state.rounds().to_vec())
            .ok_or_else(|| anyhow!("no eval state found for test_id {}", test_id))
    }

    /// Print the results of a single test by test_id
    pub fn flush_test(&self, test_id: usize) -> Result<()> {
        let intent = self
            .intents
            .get(test_id)
            .ok_or_else(|| anyhow!("invalid test_id {}", test_id))?;

        let rounds = self.get_rounds(test_id)?;

        println!("\n{:=<80}", "");
        println!("Test #{}: {}", test_id, intent);
        println!("{:=<80}", "");
        for (round_idx, round) in rounds.iter().enumerate() {
            println!("\nRound {}:", round_idx + 1);
            println!("{}", round);
        }
        println!("{:=<80}\n", "");

        Ok(())
    }

    /// Print the results of all tests
    pub fn flush(&self) -> Result<()> {
        println!("\n{:=<80}", "");
        println!("EVALUATION RESULTS");
        println!("{:=<80}", "");

        for test_id in 0..self.intents.len() {
            self.flush_test(test_id)?;
        }

        Ok(())
    }
}
