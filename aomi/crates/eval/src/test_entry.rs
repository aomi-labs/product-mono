use crate::harness::Harness;
use anyhow::Result;
use std::sync::Arc;

// ============================================================================
// BASIC TESTS (5) - Simple, single-operation tasks
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_basic_operations() -> Result<()> {
    let intents = vec![
        // Basic swap
        "Swap 0.1 ETH for USDC".to_string(),
        // Balance check
        "What's my current ETH balance?".to_string(),
        // Simple pool query
        "Find the ETH/USDC liquidity pool on Uniswap".to_string(),
        // Memecoin info
        "Tell me about the PEPE token".to_string(),
        // Basic bridge query
        "How do I bridge ETH from Ethereum to Arbitrum?".to_string(),
    ];

    let harness = Arc::new(Harness::default(intents.clone(), 3).await?);
    harness.run_suites().await?;
    harness.flush()?;

    Ok(())
}

// ============================================================================
// MEDIUM TESTS (5) - Multi-step operations with risk assessment
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_medium_operations() -> Result<()> {
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
    harness.run_suites().await?;
    harness.flush()?;

    Ok(())
}

// ============================================================================
// HARD TESTS (5) - Complex multi-protocol strategies
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_hard_operations() -> Result<()> {
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
    harness.run_suites().await?;
    harness.flush()?;

    Ok(())
}

// ============================================================================
// COMPREHENSIVE TEST - All 15 tests in parallel
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_comprehensive_eval_suite() -> Result<()> {
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
    harness.run_suites().await?;
    harness.flush()?;

    Ok(())
}

// ============================================================================
// LEGACY TEST - Original test kept for backwards compatibility
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_general_eval_with_agent_concurrency() -> Result<()> {
    let intents = vec!["find the best Defi pool with ETH and put 0.5 ETH in".to_string()];
    let harness = Arc::new(Harness::default(intents.clone(), 3).await?);

    harness.run_suites().await?;
    harness.flush()?;

    Ok(())
}
