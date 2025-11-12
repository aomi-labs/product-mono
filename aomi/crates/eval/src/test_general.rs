use crate::harness::Harness;
use anyhow::Result;
use std::env;

const TEST_INTENTS: [&str; 3] = [
    "find the best Defi pool with ETH and put 0.5 ETH in",
    "Wrap all my BTC for me",
    "Bridge my token to current network to Arbitrum",
];

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
    let intents = TEST_INTENTS
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let harness = Harness::headless(intents, max_rounds, concurrency_limit).await?;

    let results = harness.run_eval_suite().await?;
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
