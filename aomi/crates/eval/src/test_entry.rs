use std::{env, sync::Arc};

use crate::{TestResult, harness::Harness};
use anyhow::Result;

fn skip_if_missing_anthropic_key() -> Result<bool> {
    if env::var("ANTHROPIC_API_KEY").is_err() {
        println!("Skipping eval tests: ANTHROPIC_API_KEY not set");
        return Ok(true);
    }
    Ok(false)
}

async fn run_suite_and_verify(
    harness: &Arc<Harness>,
    intents: &[String],
) -> Result<Vec<TestResult>> {
    let results = harness.run_suites().await?;
    assert_eq!(results.len(), intents.len());

    for result in &results {
        let snapshot = harness.result_for(result.test_id)?;
        assert_eq!(snapshot.intent, intents[result.test_id]);
    }

    Ok(results)
}

// ============================================================================
// BASIC TESTS (5) - Simple, single-operation tasks
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_basic_operations() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let intents = vec![
        // Balance check
        "What's my current ETH balance?".to_string(),
        // Transfer ETH to Bob
        "Transfer 10 ETH to Bob".to_string(),
        // Swap ETH for USDC
        "Swap 1 ETH for USDC".to_string(),
        // Simple pool query
        "Find the  ETH/USDT liquidity pool on Uniswap".to_string(),
        // Memecoin info
        //"Tell me about the PEPE token".to_string(),
        // Basic bridge query
        //"How do I bridge ETH from Ethereum to Arbitrum?".to_string(),
    ];

    let expectations = vec![
        "Alice's wallet holds about 10000 ETH (10,000 * 10^18 wei).",
        "Bob's balance is increased by 10 ETH.",
        "Alice's ETH balance is decreased by 1 ETH and USDC balance is increased by at least 1000 USDC.",
        "ETH/USDT liquidity pool on Uniswap exists.",
        //"PEPE token is a memecoin with a purpose of being a meme token.",
    ];

    let harness = Arc::new(Harness::default(intents.clone(), 3).await?);
    let results = run_suite_and_verify(&harness, &intents).await?;
    for result in &results {
        assert!(result.total_rounds() <= harness.max_round());
    }
    let verdicts = harness.verify_expectations(&expectations.as_slice()).await?;
    assert!(
        verdicts.iter().all(|pass| *pass),
        "Basic expectation verification failed: {:?}",
        verdicts
    );
    harness.flush()?;

    Ok(())
}

// ============================================================================
// MEDIUM TESTS (5) - Multi-step operations with risk assessment
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_medium_operations() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let intents = vec![
        // Add liquidity with risk assessment
        "Add 1 ETH and equivalent USDC to a liquidity pool with the best APY. What are the risks?"
            .to_string(),
        // Multi-step swap with slippage
        "Swap 500 USDC for ETH then to WBTC. Set slippage to 0.5%.".to_string(),
        // Bridge with fee comparison
        "Bridge 0.5 ETH from Ethereum to Base. Compare fees across different bridges.".to_string(),
        // Prediction market bet
        "Place a bet on Polymarket for the next US election. Show me the current odds.".to_string(),
        // Memecoin analysis with rug pull detection
        "Analyze BONK token for potential red flags and tell me if it's safe to invest 100 USDC"
            .to_string(),
    ];

    let harness = Arc::new(Harness::default(intents.clone(), 4).await?);
    let results = run_suite_and_verify(&harness, &intents).await?;
    for result in &results {
        assert!(result.total_rounds() <= harness.max_round());
    }
    harness.flush()?;

    Ok(())
}

// ============================================================================
// HARD TESTS (5) - Complex multi-protocol strategies
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_hard_operations() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let intents = vec![
        // Complex DeFi yield farming strategy
        "I have 10 ETH. Create a yield farming strategy across Aave, Compound, and Uniswap to maximize returns. Include risk breakdown.".to_string(),

        // Proxy contract interaction and upgrade analysis
        "Check if the USDC contract on Ethereum is upgradeable. If it is, show me the implementation contract and recent upgrades.".to_string(),

        // Cross-chain arbitrage
        "Find arbitrage opportunities for ETH/USDC across Ethereum, Arbitrum, and Optimism. Calculate profit after gas and bridge fees.".to_string(),

        // Multi-market prediction portfolio
        "Create a diversified prediction market portfolio on Polymarket with 1000 USDC. Balance between politics, sports, and crypto events.".to_string(),

        // Memecoin portfolio rebalancing
        "I hold PEPE, DOGE, SHIB, and BONK worth 5000 USDC total. Rebalance my portfolio based on recent performance and risk metrics.".to_string(),
    ];

    let harness = Arc::new(Harness::default(intents.clone(), 5).await?);
    let results = run_suite_and_verify(&harness, &intents).await?;
    for result in &results {
        assert!(result.total_rounds() <= harness.max_round());
    }
    harness.flush()?;

    Ok(())
}

// ============================================================================
// COMPREHENSIVE TEST - All 15 tests in parallel
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_comprehensive_eval_suite() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let intents = vec![
        // === BASIC (5) ===
        "Swap 0.1 ETH for USDC".to_string(),
        "What's my current ETH balance?".to_string(),
        "Find the ETH/USDC liquidity pool on Uniswap".to_string(),
        "Tell me about the PEPE token".to_string(),
        "How do I bridge ETH from Ethereum to Arbitrum?".to_string(),

        // === MEDIUM (5) ===
        "Add 1 ETH and equivalent USDC to a liquidity pool with the best APY. What are the risks?".to_string(),
        "Swap 500 USDC for ETH then to WBTC. Set slippage to 0.5%.".to_string(),
        "Bridge 0.5 ETH from Ethereum to Base. Compare fees across different bridges.".to_string(),
        "Place a bet on Polymarket for the next US election. Show me the current odds.".to_string(),
        "Analyze BONK token for potential red flags and tell me if it's safe to invest 100 USDC".to_string(),

        // === HARD (5) ===
        "I have 10 ETH. Create a yield farming strategy across Aave, Compound, and Uniswap to maximize returns. Include risk breakdown.".to_string(),
        "Check if the USDC contract on Ethereum is upgradeable. If it is, show me the implementation contract and recent upgrades.".to_string(),
        "Find arbitrage opportunities for ETH/USDC across Ethereum, Arbitrum, and Optimism. Calculate profit after gas and bridge fees.".to_string(),
        "Create a diversified prediction market portfolio on Polymarket with 1000 USDC. Balance between politics, sports, and crypto events.".to_string(),
        "I hold PEPE, DOGE, SHIB, and BONK worth 5000 USDC total. Rebalance my portfolio based on recent performance and risk metrics.".to_string(),
    ];

    let harness = Arc::new(Harness::default(intents.clone(), 4).await?);
    let results = run_suite_and_verify(&harness, &intents).await?;
    for result in &results {
        assert!(result.total_rounds() <= harness.max_round());
    }
    harness.flush()?;

    Ok(())
}

// ============================================================================
// LEGACY TEST - Original test kept for backwards compatibility
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_general_eval_with_agent_concurrency() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let intents = vec!["find the best Defi pool with ETH and put 0.5 ETH in".to_string()];
    let harness = Arc::new(Harness::default(intents.clone(), 3).await?);

    let results = run_suite_and_verify(&harness, &intents).await?;
    for result in &results {
        assert!(result.total_rounds() <= harness.max_round());
    }
    harness.flush()?;

    Ok(())
}
