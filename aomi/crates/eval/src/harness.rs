use std::{fmt::Write, sync::Arc};

use anyhow::{Result, anyhow, ensure};
use aomi_backend::session::BackendwithTool;
use aomi_chat::ChatApp;
use dashmap::DashMap;
use futures::stream::{self, StreamExt};
use rig::message::Message;
use tokio::sync::mpsc;

use crate::eval_app::EvaluationApp;
use crate::{EvalState, RoundResult};

pub struct Harness {
    pub eval_app: Arc<EvaluationApp>,
    pub backend: Arc<BackendwithTool>,
    pub intents: Vec<String>,
    pub eval_states: DashMap<usize, EvalState>,
    pub max_round: usize,
    pub concurrency_limit: usize,
}

impl Harness {
    pub fn new(
        eval_app: EvaluationApp,
        backend: Arc<BackendwithTool>,
        intents: Vec<String>,
        max_round: usize,
        concurrency_limit: usize,
    ) -> Result<Self> {
        ensure!(
            concurrency_limit > 0,
            "concurrency limit must be at least 1"
        );
        Ok(Self {
            eval_app: Arc::new(eval_app),
            backend,
            intents,
            eval_states: DashMap::new(),
            max_round,
            concurrency_limit,
        })
    }

    pub async fn default(
        intents: Vec<String>,
        max_round: usize,
        concurrency_limit: usize,
    ) -> Result<Self> {
        let eval_app = EvaluationApp::headless().await?;
        let backend = Arc::new(ChatApp::new().await.map_err(|err| anyhow!(err))?);
        Self::new(eval_app, backend,intents, max_round, concurrency_limit)
    }

    pub fn max_round(&self) -> usize {
        self.max_round
    }

    pub fn concurrency_limit(&self) -> usize {
        self.concurrency_limit
    }

    pub fn intents(&self) -> &[String] {
        &self.intents
    }

    pub async fn process_intent(&self, test_id: usize, intent: String) -> Result<bool> {
        let trimmed = intent.trim().to_string();
        if trimmed.is_empty(){
            return Ok(false);
        }
        let need_next_round = if let Some(mut eval_state) = self.eval_states.get_mut(&test_id) {
            eval_state.run_round(&trimmed).await?
        } else {
            let mut eval_state = EvalState::new(self.backend.clone(), self.max_round).await?;
            let need_next_round = eval_state.run_round(&trimmed).await?;
            self.eval_states.insert(test_id, eval_state);
            need_next_round
        };
        Ok(need_next_round)
    }

    pub fn get_rounds(&self, test_id: usize) -> Result<Vec<RoundResult>> {
        self.eval_states
            .get(&test_id)
            .map(|state| state.rounds().to_vec())
            .ok_or_else(|| anyhow!("no eval state found for test_id {}", test_id))
    }

    fn run(&self) {
        rayon::scope(op)
    }

    /// Runs evaluation suite with full concurrency across all intents.
    /// Each intent is evaluated independently with its own eval_app conversation.
    pub async fn run_eval_suite(&self) -> Result<Vec<(String, Vec<RoundResult>)>> {
        if self.intents.is_empty() {
            return Ok(Vec::new());
        }

        println!("running eval suite with {} intents", self.intents.len());

        let intents = self.intents.clone();
        let mut ordered_results: Vec<Option<(String, Vec<RoundResult>)>> =
            vec![None; intents.len()];

        let intent_stream = stream::iter(intents.into_iter().enumerate());

        let mut buffered = intent_stream
            .map(|(idx, intent)| async move {
                let rounds = self.eval_with_agent(idx, intent.clone()).await?;
                Ok::<_, anyhow::Error>((idx, intent, rounds))
            })
            .buffer_unordered(self.concurrency_limit);

        while let Some(result) = buffered.next().await {
            let (idx, intent, rounds) = result?;
            ordered_results[idx] = Some((intent, rounds));
        }

        ordered_results
            .into_iter()
            .map(|entry| entry.ok_or_else(|| anyhow!("missing eval result for intent")))
            .collect()
    }

    /// Evaluates a single intent with the agent, using eval_app to generate follow-up prompts.
    /// The eval_app is shared across threads (Arc) since it's immutable.
    async fn eval_with_agent(&self, test_id: usize, initial_intent: String) -> Result<Vec<RoundResult>> {
        println!("[eval] Starting eval_with_agent for test_id={}, intent: {}", test_id, initial_intent);

        let trimmed = initial_intent.trim().to_string();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let mut eval_history: Vec<Message> = Vec::new();
        let mut next_prompt = trimmed;

        loop {
            println!("[eval] test_id={}, sending prompt: {}", test_id, next_prompt);

            // Process the intent and check if we need another round
            let need_next_round = self.process_intent(test_id, next_prompt.clone()).await?;

            if !need_next_round {
                println!("[eval] test_id={}, reached max rounds or stopped", test_id);
                break;
            }

            // Get current rounds to generate transcript
            let rounds = self.get_rounds(test_id)?;
            println!("[eval] test_id={}, completed {} rounds, generating next prompt...",
                     test_id, rounds.len());

            // Generate next prompt using eval_app (shared across threads via Arc)
            let transcript = format_transcript(&rounds);
            let maybe_prompt = self.eval_app
                .next_eval_prompt(&mut eval_history, transcript, rounds.len(), self.max_round)
                .await?;

            match maybe_prompt {
                Some(prompt) => {
                    println!("[eval] test_id={}, eval agent generated: {}", test_id, prompt);
                    next_prompt = prompt;
                }
                None => {
                    println!("[eval] test_id={}, eval agent signaled DONE", test_id);
                    break;
                }
            }
        }

        let rounds = self.get_rounds(test_id)?;
        println!("[eval] test_id={}, completed with {} total rounds", test_id, rounds.len());
        Ok(rounds)
    }
}

fn format_transcript(rounds: &[RoundResult]) -> String {
    let mut buffer = String::new();
    for (idx, round) in rounds.iter().enumerate() {
        let _ = writeln!(&mut buffer, "Round {}:", idx + 1);
        let _ = writeln!(&mut buffer, "{round}");
    }
    buffer
}