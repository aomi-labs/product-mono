use anyhow::Result;

use crate::harness::{EvalCase, Harness};
use crate::skip_if_baml_unavailable;
use crate::skip_if_missing_anthropic_key;

// ============================================================================
// SCRIPTER TESTS - Forge script generation via LLM agent
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Scripter eval requires BAML server and ANTHROPIC_API_KEY"]
async fn test_simple_eth_transfer_script() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }
    if skip_if_baml_unavailable() {
        println!("Skipping scripter tests: BAML server not available at localhost:2024");
        return Ok(());
    }

    let cases = vec![
        EvalCase::new(
            "Create a forge script to send 1 ETH to 0x1234567890123456789012345678901234567890",
        )
        .with_expectation(
            "A valid Forge script was generated that transfers 1 ETH to the specified address.",
        ),
    ];

    let harness = Harness::for_scripter(cases, 3).await?;
    let results = harness.run_suites().await?;

    // Verify ForgeScriptBuilder was called
    for result in &results {
        assert!(
            result.has_tool_call("ForgeScriptBuilder"),
            "Expected ForgeScriptBuilder tool call, got: {:?}",
            result.rounds
        );
    }

    let verdicts = harness.verify_case_expectations().await?;
    for (i, verdict) in verdicts.iter().enumerate() {
        if let Some(v) = verdict {
            assert!(v.satisfied, "Test {}: {}", i, v.explanation);
        }
    }

    harness.flush()?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Scripter eval requires BAML server and ANTHROPIC_API_KEY"]
async fn test_erc20_approval_script() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }
    if skip_if_baml_unavailable() {
        println!("Skipping scripter tests: BAML server not available at localhost:2024");
        return Ok(());
    }

    let cases = vec![
        EvalCase::new(
            "Create a forge script to approve 1000 USDC (address 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48) \
             for the Uniswap V2 router (address 0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D). \
             Use the IERC20 interface from forge-std.",
        )
        .with_expectation(
            "A valid Forge script was generated with an ERC20 approve call for USDC to the Uniswap router.",
        ),
    ];

    let harness = Harness::for_scripter(cases, 4).await?;
    let results = harness.run_suites().await?;

    // Verify ForgeScriptBuilder was called
    for result in &results {
        assert!(
            result.has_tool_call("ForgeScriptBuilder"),
            "Expected ForgeScriptBuilder tool call"
        );
    }

    let verdicts = harness.verify_case_expectations().await?;
    for (i, verdict) in verdicts.iter().enumerate() {
        if let Some(v) = verdict {
            assert!(v.satisfied, "Test {}: {}", i, v.explanation);
        }
    }

    harness.flush()?;
    Ok(())
}
