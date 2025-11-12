use anyhow::{anyhow, ensure, Result};
use crate::{eval_with_agent, RoundResult};
use futures::stream::{self, StreamExt};
use std::env;

const TEST_INTENTS: [&str; 1] = [
    "find the best Defi pool with ETH and put 0.5 ETH in",
    // "Wrap all my BTC for me",
    // "Bridge my token to current network to Arbitrum",
];

async fn run_eval_suite(
    intents: &[&str],
    concurrency_limit: usize,
    max_round: usize,
) -> Result<Vec<(String, Vec<RoundResult>)>> {
    ensure!(concurrency_limit > 0, "concurrency limit must be at least 1");

    if intents.is_empty() {
        return Ok(Vec::new());
    }

    let mut ordered_results: Vec<Option<(String, Vec<RoundResult>)>> = vec![None; intents.len()];

    let intent_stream = stream::iter(
        intents
            .iter()
            .enumerate()
            .map(|(idx, intent)| (idx, intent.trim().to_string())),
    );

    let mut buffered = intent_stream
        .map(|(idx, intent)| async move {
            let rounds = eval_with_agent(intent.clone(), max_round).await?;
            Ok::<_, anyhow::Error>((idx, intent, rounds))
        })
        .buffer_unordered(concurrency_limit);

    while let Some(result) = buffered.next().await {
        let (idx, intent, rounds) = result?;
        ordered_results[idx] = Some((intent, rounds));
    }

    ordered_results
        .into_iter()
        .map(|entry| entry.ok_or_else(|| anyhow!("missing eval result for intent")))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_general_eval_with_agent_concurrency() -> Result<()> {
    if !eval_tests_enabled() {
        eprintln!(
            "skipping eval_with_agent concurrency test (set RUN_EVAL_WITH_AGENT_TESTS=1 to enable)"
        );
        return Ok(());
    }

    let concurrency_limit = 2;
    let max_rounds = 3;

    let results = run_eval_suite(&TEST_INTENTS, concurrency_limit, max_rounds).await?;
    assert_eq!(results.len(), TEST_INTENTS.len());

    for (intent, rounds) in results {
        assert!(
            !rounds.is_empty(),
            "intent `{intent}` produced no agent rounds"
        );

        for (round_idx, round) in rounds.iter().enumerate() {
            assert!(
                !round.is_empty(),
                "intent `{intent}` round {round_idx} captured no agent actions"
            );
        }
    }

    Ok(())
}

fn eval_tests_enabled() -> bool {
    matches!(
        env::var("RUN_EVAL_WITH_AGENT_TESTS")
            .unwrap_or_default()
            .as_str(),
        "1" | "true" | "TRUE" | "yes" | "YES"
    )
}
