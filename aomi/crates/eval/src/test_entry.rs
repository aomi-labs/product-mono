use std::env;

use anyhow::Result;

use crate::{
    TestResult,
    assertions::{BalanceAsset, BalanceChange},
    eval_app::EVAL_ACCOUNTS,
    harness::{EvalCase, Harness},
};

fn skip_if_missing_anthropic_key() -> Result<bool> {
    if env::var("ANTHROPIC_API_KEY").is_err() {
        println!("Skipping eval tests: ANTHROPIC_API_KEY not set");
        return Ok(true);
    }
    Ok(false)
}

fn bob_address() -> &'static str {
    EVAL_ACCOUNTS
        .get(1)
        .map(|(_, address)| *address)
        .expect("missing Bob account for deterministic eval assertions")
}

fn alice_address() -> &'static str {
    EVAL_ACCOUNTS
        .first()
        .map(|(_, address)| *address)
        .expect("missing Alice account for deterministic eval assertions")
}

async fn run_suite_and_verify(harness: &Harness) -> Result<Vec<TestResult>> {
    let results = harness.run_suites().await?;
    assert_eq!(results.len(), harness.case_count());

    for result in &results {
        let snapshot = harness.result_for(result.test_id)?;
        let case = harness
            .case(result.test_id)
            .expect("missing eval case for snapshot comparison");
        assert_eq!(snapshot.intent, case.intent);
    }

    let expectation_verdicts = harness.verify_case_expectations().await?;
    let assertion_results = harness.verify_assertions().await?;

    harness.print_outcome_table(&expectation_verdicts, &assertion_results)?;
    harness.print_assertions(&assertion_results);
    harness.assert_assertions(&assertion_results)?;

    Ok(results)
}

async fn run_single_case(case: EvalCase, max_round: usize) -> Result<()> {
    let harness = Harness::default_with_cases(vec![case], max_round).await?;

    let results = run_suite_and_verify(&harness).await?;
    for result in &results {
        assert!(result.total_rounds() <= harness.max_round());
    }

    harness.flush()?;
    Ok(())
}

// ============================================================================
// BASIC TESTS (4) - Simple, single-operation tasks
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_check_current_eth_balance() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let case = EvalCase::new("What's my current ETH balance?")
        .with_expectation("Alice's wallet holds about 10000 ETH (10,000 * 10^18 wei).")
        .with_balance_at_least(
            alice_address(),
            BalanceAsset::eth(),
            1,
            "Alice has a positive ETH balance",
        );

    run_single_case(case, 3).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_transfer_eth_to_bob() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let transfer_assertion = BalanceChange::eth_increase(
        bob_address(),
        10,
        "Bob receives 10 ETH from transfer intent",
    )?;
    let case = EvalCase::new("Transfer 10 ETH to Bob")
        .with_expectation("Bob's balance is increased by 10 ETH.")
        .with_balance_change(transfer_assertion);

    run_single_case(case, 3).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_swap_eth_for_usdc() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let case = EvalCase::new("Swap 1 ETH for USDC").with_expectation(
        "Alice's ETH balance is decreased by 1 ETH and USDC balance is increased by at least 1000 USDC.",
    );

    run_single_case(case, 3).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_find_eth_usdt_pool() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let case = EvalCase::new("Find the  ETH/USDT liquidity pool on Uniswap")
        .with_expectation("ETH/USDT liquidity pool on Uniswap exists.");

    run_single_case(case, 3).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_list_available_tools() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let tools = "
    - brave_search
    - send_transaction_to_wallet
    - encode_function_call
    - call_view_function
    - simulate_contract_call
    - get_current_time
    - get_contract_abi
    - get_contract_source_code
    - fetch_contract_from_etherscan
    - get_account_info
    - get_account_transaction_history
";

    let case =
        EvalCase::new("List all tools you can use and what they do.").with_expectation(format!(
            "Agent should list all tools available with short descriptions. The tools are: {}",
            tools
        ));

    run_single_case(case, 2).await
}
