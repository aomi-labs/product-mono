use anyhow::Result;

use crate::{
    TestResult,
    assertions::{BalanceAsset, BalanceChange, BalanceCheck, WEI_PER_ETH},
    eval_app::EVAL_ACCOUNTS,
    harness::{EvalCase, Harness},
    skip_if_missing_anthropic_key,
};

const USDC_MAINNET: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const STETH_MAINNET: &str = "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84";
const WSTETH_MAINNET: &str = "0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0";
const AAVE_AUSDC_MAINNET: &str = "0x98C23E9d8f34FEFb1B7BD6a91B7FF122F4e16F5c";
const AAVE_VARIABLE_DEBT_USDC_MAINNET: &str = "0x72E95b8931767C79bA4EeE721354d6E99a61D004";
const UNIV2_ETH_USDC_LP: &str = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc";

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

fn univ2_eth_usdc_lp_asset() -> Result<BalanceAsset> {
    BalanceAsset::erc20("USDC/WETH LP", UNIV2_ETH_USDC_LP, 18)
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

async fn run_cases(cases: Vec<EvalCase>, max_round: usize) -> Result<()> {
    let harness = Harness::default_with_cases(cases, max_round).await?;
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
#[ignore = "run via scripts/run-eval-tests.sh"]
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
#[ignore = "run via scripts/run-eval-tests.sh"]
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
        .with_expectation(
            "The transaction of transferring 10 ETH to Bob has been executed successfully.",
        )
        .with_balance_change(transfer_assertion);

    run_single_case(case, 3).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run via scripts/run-eval-tests.sh"]
async fn test_swap_eth_for_usdc() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let usdc = usdc_asset()?;
    let eth_spent = BalanceChange::eth_delta(
        alice_address(),
        -(WEI_PER_ETH as i128),
        50_000_000_000_000_000, // 0.05 ETH tolerance for gas/slippage
        "Alice spends about 1 ETH for the swap",
    );
    let usdc_gain = BalanceChange::asset_delta(
        alice_address(),
        usdc,
        1_000_000, // 1,000 USDC (6 decimals)
        0,
        "USDC balance increases by at least 1,000 tokens",
    );
    let case = EvalCase::new("Swap 1 ETH for USDC")
        .with_expectation(
            "Alice's ETH balance decreases by roughly 1 ETH and her USDC increases by at least 1,000.",
        )
        .with_balance_change(eth_spent)
        .with_balance_change_at_least(usdc_gain);

    run_single_case(case, 3).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run via scripts/run-eval-tests.sh"]
async fn test_find_eth_usdt_pool() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let case = EvalCase::new("Find the  ETH/USDT liquidity pool on Uniswap")
        .with_expectation("ETH/USDT liquidity pool on Uniswap exists.");

    run_single_case(case, 3).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run via scripts/run-eval-tests.sh"]
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
    - get_erc20_balance
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
#[ignore = "run via scripts/run-eval-tests.sh"]
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
#[ignore = "run via scripts/run-eval-tests.sh"]
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
#[ignore = "run via scripts/run-eval-tests.sh"]
async fn test_add_and_remove_liquidity_on_uniswap() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    // Case 1: Add liquidity and ensure LP is minted
    let lp_token = univ2_eth_usdc_lp_asset()?;
    let lp_minted = BalanceChange::asset_delta(
        alice_address(),
        lp_token.clone(),
        1, // any positive mint proves LP position
        0,
        "LP tokens minted after adding ETH/USDC liquidity",
    );
    let add_case = EvalCase::new(
        "Add ETH/USDC liquidity on Uniswap V2 using roughly 0.25 ETH plus the matching USDC.",
    )
    .with_expectation(
        "Alice successfully provides ETH/USDC liquidity on Uniswap and receives LP tokens.",
    )
    .with_balance_change_at_least(lp_minted);

    // Case 2: Remove some liquidity and verify LP decreases + assets increase
    let lp_burned = BalanceChange::asset_delta(
        alice_address(),
        lp_token,
        -1, // require at least some LP burn
        0,
        "LP tokens decrease after removing liquidity",
    );
    let eth_redeemed = BalanceChange::asset_delta(
        alice_address(),
        BalanceAsset::eth(),
        100_000_000_000_000, // ≥0.0001 ETH reclaimed
        0,
        "ETH increases after withdrawing some liquidity",
    );
    let usdc_redeemed = BalanceChange::asset_delta(
        alice_address(),
        usdc_asset()?,
        1_000_000, // ≥1 USDC reclaimed
        0,
        "USDC increases after withdrawing some liquidity",
    );
    let remove_case = EvalCase::new(
        "Remove all my ETH/USDC liquidity from Uniswap V2",
    )
    .with_expectation(
        "Liquidity is withdrawn from Uniswap V2 and Alice recovers underlying assets, shrinking the position.",
    )
    .with_balance_change_at_most(lp_burned)
    .with_balance_change_at_least(eth_redeemed)
    .with_balance_change_at_least(usdc_redeemed);

    run_cases(vec![add_case, remove_case], 6).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run via scripts/run-eval-tests.sh"]
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
#[ignore = "run via scripts/run-eval-tests.sh"]
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
#[ignore = "run via scripts/run-eval-tests.sh"]
async fn test_supply_and_withdraw_usdc_to_aave() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let ausdc = aave_ausdc_asset()?;
    let ausdc_receipt = BalanceChange::asset_delta(
        alice_address(),
        ausdc.clone(),
        25_000_000i128,
        0,
        "Alice receives aEthUSDC after depositing ~25+ USDC to Aave",
    );
    let supply_case = EvalCase::new(
        "Supply 50 USDC into Aave as collateral.",
    )
    .with_expectation(
        "Alice deposits USDC into the Aave pool and ends up holding the matching aEthUSDC receipt tokens.",
    )
    .with_balance_change_at_least(ausdc_receipt);

    let ausdc_burn = BalanceChange::asset_delta(
        alice_address(),
        ausdc,
        -10_000_000i128,
        0,
        "aEthUSDC decreases after withdrawing from Aave",
    );
    let usdc_returned = BalanceChange::asset_delta(
        alice_address(),
        usdc_asset()?,
        10_000_000i128,
        0,
        "USDC increases after withdrawing collateral",
    );
    let withdraw_case = EvalCase::new(
        "Withdraw all my USDC from Aave.",
    )
    .with_expectation(
        "Alice successfully pulls USDC back out of Aave, demonstrating the withdrawal path from an existing deposit.",
    )
    .with_balance_change_at_most(ausdc_burn)
    .with_balance_change_at_least(usdc_returned);

    run_cases(vec![supply_case, withdraw_case], 6).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run via scripts/run-eval-tests.sh"]
async fn test_borrow_and_repay_aave_loan() -> Result<()> {
    if skip_if_missing_anthropic_key()? {
        return Ok(());
    }

    let variable_debt = aave_variable_debt_usdc_asset()?;
    let debt_increase = BalanceChange::asset_delta(
        alice_address(),
        variable_debt.clone(),
        5_000_000i128,
        0,
        "Variable USDC debt increases after borrowing from Aave",
    );
    let borrow_case = EvalCase::new("Borrow 10 USDC from Aave after posting collateral.")
        .with_expectation(
            "Alice supplies collateral and leaves with about 10 USDC borrowed on Aave, reflected in her variableDebtEthUSDC balance.",
        )
        .with_balance_change_at_least(debt_increase);

    let debt_cleared = BalanceCheck::new(
        alice_address(),
        variable_debt,
        0,
        1, // allow tiny dust after repayment
        "Aave variable USDC debt returns to zero after borrow and repay",
    );
    let repay_case = EvalCase::new(
        "Repay a small USDC loan on Aave (open a 10 USDC borrow first if nothing is outstanding).",
    )
    .with_expectation(
        "Alice repays her Aave USDC debt fully, confirming the repayment by showing the debt token balance returns to zero.",
    )
    .with_balance_check(debt_cleared);

    run_cases(vec![borrow_case, repay_case], 8).await
}
