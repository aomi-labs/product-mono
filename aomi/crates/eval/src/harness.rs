use std::{fmt::Write, sync::Arc};

use anyhow::{Result, anyhow, ensure};
use aomi_backend::session::BackendwithTool;
use aomi_chat::ChatApp;
use futures::stream::{self, StreamExt};
use rig::message::Message;

use crate::eval_app::EvaluationApp;
use crate::{Eval, RoundResult};

pub struct Harness {
    pub eval_app: EvaluationApp,
    pub intents: Vec<String>,
    pub max_round: usize,
    pub concurrency_limit: usize,
}

impl Harness {
    pub fn new(
        eval_app: EvaluationApp,
        intents: Vec<String>,
        max_round: usize,
        concurrency_limit: usize,
    ) -> Result<Self> {
        ensure!(
            concurrency_limit > 0,
            "concurrency limit must be at least 1"
        );
        Ok(Self {
            eval_app,
            intents,
            max_round,
            concurrency_limit,
        })
    }

    pub async fn headless(
        intents: Vec<String>,
        max_round: usize,
        concurrency_limit: usize,
    ) -> Result<Self> {
        let eval_app = EvaluationApp::headless().await?;
        Self::new(eval_app, intents, max_round, concurrency_limit)
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
                let rounds = self.eval_with_agent(intent.clone()).await?;
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

    pub async fn eval_with_agent(&self, initial_intent: String) -> Result<Vec<RoundResult>> {
        println!("[eval] Starting eval_with_agent for: {}", initial_intent);

        if self.max_round == 0 {
            return Ok(Vec::new());
        }

        let trimmed = initial_intent.trim().to_string();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        println!("[eval] Creating ChatApp (headless mode, no tools)...");
        let chat_app = Arc::new(ChatApp::new_headless().await.map_err(|err| anyhow!(err))?);
        let backend: Arc<BackendwithTool> = chat_app;

        println!("[eval] Initializing Eval harness...");
        let mut harness = Eval::new(backend).await?;

        let mut eval_history: Vec<Message> = Vec::new();
        let mut rounds = Vec::new();
        let mut next_prompt = trimmed;

        for round_num in 0..self.max_round {
            println!("[eval] Starting round {}/{}", round_num + 1, self.max_round);
            println!("[eval] Sending prompt to agent: {}", next_prompt);
            let round = harness.run_round(&next_prompt).await?;
            
            println!("round: {round:?}");
            rounds.push(round);

            if rounds.len() >= self.max_round {
                println!("[eval] Reached max_round limit");
                break;
            }

            println!("[eval] Getting next prompt from evaluation agent...");
            let transcript = format_transcript(&rounds);
            let maybe_prompt = self
                .eval_app
                .next_eval_prompt(&mut eval_history, transcript, rounds.len(), self.max_round)
                .await?;

            match maybe_prompt {
                Some(prompt) => {
                    println!("[eval] Eval agent generated next prompt: {}", prompt);
                    next_prompt = prompt;
                }
                None => {
                    println!("[eval] Eval agent signaled completion (DONE)");
                    break;
                }
            }
        }

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
