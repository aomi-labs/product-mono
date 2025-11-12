use crate::harness::Harness;
use anyhow::Result;


#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_general_eval_with_agent_concurrency() -> Result<()> {

    let intents = vec!["find the best Defi pool with ETH and put 0.5 ETH in".to_string()];
    let harness = Harness::headless(intents.clone(), 3, 2).await?;

    let results = harness.run_eval_suite().await?;
    assert_eq!(results.len(), intents.len());

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

