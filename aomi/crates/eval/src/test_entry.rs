use std::env;

use anyhow::Result;

use crate::{
    TestResult,
    assertions::{BalanceAsset, BalanceChange},
    eval_app::EVAL_ACCOUNTS,
    harness::{EvalCase, Harness},
};

const USDC_MAINNET: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const STETH_MAINNET: &str = "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84";
const WSTETH_MAINNET: &str = "0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0";
const AAVE_AUSDC_MAINNET: &str = "0x98C23E9d8f34FEFb1B7BD6a91B7FF122F4e16F5c";
const AAVE_VARIABLE_DEBT_USDC_MAINNET: &str = "0x72E95b8931767C79bA4EeE721354d6E99a61D004";

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

fn usdc_asset() -> Result<BalanceAsset> {
    BalanceAsset::usdc(USDC_MAINNET)
}

fn steth_asset() -> Result<BalanceAsset> {
    BalanceAsset::erc20("stETH", STETH_MAINNET, 18)
}

fn wsteth_asset() -> Result<BalanceAsset> {
    BalanceAsset::erc20("wstETH", WSTETH_MAINNET, 18)
}

fn aave_ausdc_asset() -> Result<BalanceAsset> {
    BalanceAsset::erc20("aEthUSDC", AAVE_AUSDC_MAINNET, 6)
}

fn aave_variable_debt_usdc_asset() -> Result<BalanceAsset> {
    BalanceAsset::erc20("variableDebtEthUSDC", AAVE_VARIABLE_DEBT_USDC_MAINNET, 6)
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
        .with_expectation("Alice's wallet holds at least 1 ETH.")
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
        .with_expectation("The transaction of transferring 10 ETH to Bob has been executed successfully.")
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

// ============================================================================
// DEFI TESTS (10) - Core onchain interactions for ERC-20s, DEXes, and lending
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_approve_usdc_spender() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let case = EvalCase::new(
        "Approve the Uniswap V3 router (0xE592427A0AEce92De3Edee1F18E0157C05861564) to spend 1000 USDC from my wallet.",
    )
    .with_expectation(
        "Alice authorizes the Uniswap V3 router to spend at least 1000 USDC from her account.",
    );

    run_single_case(case, 4).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_transfer_usdc_to_bob() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let usdc = usdc_asset()?;
    let bob_receives = BalanceChange::token_increase(
        bob_address(),
        usdc,
        25_000_000,
        "Bob receives 25 USDC via ERC-20 transfer",
    )?;
    let case = EvalCase::new(
        "Send 25 USDC to Bob at 0x8D343ba80a4cD896e3e5ADFF32F9cF339A697b28 (swap from ETH first if needed).",
    )
    .with_expectation("Bob's USDC balance rises by exactly 25 tokens once the transfer is complete.")
    .with_balance_change(bob_receives);

    run_single_case(case, 5).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_add_liquidity_on_uniswap() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let case = EvalCase::new(
        "Add ETH/USDC liquidity on Uniswap using roughly 0.25 ETH plus the matching USDC.",
    )
    .with_expectation(
        "Alice successfully provides ETH/USDC liquidity on Uniswap and receives confirmation (LP tokens or position details).",
    );

    run_single_case(case, 6).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_remove_liquidity_on_uniswap() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let case = EvalCase::new(
        "Remove a small amount of ETH/USDC liquidity from Uniswap; create a minimal position first if none exists.",
    )
    .with_expectation(
        "Liquidity is withdrawn from Uniswap and Alice recovers the underlying assets, closing or shrinking the position.",
    );

    run_single_case(case, 6).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_stake_eth_for_steth() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let steth = steth_asset()?;
    let steth_gain = BalanceChange::asset_delta(
        alice_address(),
        steth,
        1_000_000_000_000_000_000i128,
        500_000_000_000_000_000u128,
        "Alice receives roughly 1 stETH from Lido staking 1 ETH",
    );
    let case = EvalCase::new("Stake 1 ETH in Lido to mint stETH.")
        .with_expectation(
            "Alice uses Lido's submit to stake 1 ETH and ends the flow with fresh stETH in her wallet.",
        )
        .with_balance_change(steth_gain);

    run_single_case(case, 5).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_wrap_and_unwrap_steth() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let wsteth = wsteth_asset()?;
    let wrapped = BalanceChange::asset_delta(
        alice_address(),
        wsteth,
        450_000_000_000_000_000i128,
        300_000_000_000_000_000u128,
        "Alice wraps stETH into wstETH and keeps a positive wrapped balance",
    );
    let case = EvalCase::new(
        "Wrap 0.5 stETH into wstETH (stake for stETH first if needed) and keep the wstETH; optionally unwrap a small portion to show it works.",
    )
    .with_expectation(
        "Alice demonstrates stETH→wstETH conversion and retains wrapped tokens, proving the direction works (a small unwrap demo is optional).",
    )
    .with_balance_change(wrapped);

    run_single_case(case, 5).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_supply_usdc_to_aave() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let ausdc = aave_ausdc_asset()?;
    let ausdc_receipt = BalanceChange::asset_delta(
        alice_address(),
        ausdc,
        50_000_000i128,
        25_000_000u128,
        "Alice receives aEthUSDC after depositing ~50 USDC to Aave",
    );
    let case = EvalCase::new("Supply 50 USDC into Aave as collateral (swap from ETH first if needed).")
        .with_expectation(
            "Alice deposits USDC into the Aave pool and ends up holding the matching aEthUSDC receipt tokens.",
        )
        .with_balance_change(ausdc_receipt);

    run_single_case(case, 6).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_borrow_usdc_from_aave() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let variable_debt = aave_variable_debt_usdc_asset()?;
    let borrowed = BalanceChange::asset_delta(
        alice_address(),
        variable_debt,
        10_000_000i128,
        5_000_000u128,
        "Variable USDC debt increases after borrowing from Aave",
    );
    let case = EvalCase::new("Borrow 10 USDC from Aave after posting collateral.")
        .with_expectation(
            "Alice supplies collateral and leaves with about 10 USDC borrowed on Aave, reflected in her variableDebtEthUSDC balance.",
        )
        .with_balance_change(borrowed);

    run_single_case(case, 6).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_repay_aave_loan() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let case = EvalCase::new(
        "Repay a small USDC loan on Aave (open a 10 USDC borrow first if nothing is outstanding).",
    )
    .with_expectation(
        "Alice repays her Aave USDC debt fully, confirming the repayment by showing the debt token balance returns to zero.",
    );

    run_single_case(case, 6).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_withdraw_aave_deposit() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let case = EvalCase::new(
        "Withdraw a portion of an Aave USDC deposit (make a small supply first if needed).",
    )
    .with_expectation(
        "Alice successfully pulls USDC back out of Aave, demonstrating the withdrawal path from an existing deposit.",
    );

    run_single_case(case, 6).await
}
