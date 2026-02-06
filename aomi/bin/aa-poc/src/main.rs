use alloy_primitives::FixedBytes;
use aomi_aa::AAPocRunner;
use eyre::Result;
use std::env;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("aa_poc=info".parse()?)
                .add_directive("aomi_aa=info".parse()?)
                .add_directive("aomi_scripts=info".parse()?),
        )
        .init();

    info!("🚀 ERC-4337 Alto POC Runner");
    info!("====================================");

    // Configuration
    let bundler_rpc =
        env::var("BUNDLER_RPC_URL").unwrap_or_else(|_| "http://localhost:4337".to_string());
    let fork_url = env::var("FORK_URL")
        .expect("FORK_URL environment variable must be set (e.g., https://eth-mainnet.g.alchemy.com/v2/YOUR_KEY)");

    // Anvil test account #0 private key (publicly known, for testing only!)
    let owner_key = FixedBytes::from_slice(&hex::decode(
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    )?);

    info!("Configuration:");
    info!("  Bundler RPC: {}", bundler_rpc);
    info!("  Fork URL: {}", &fork_url);
    info!("");

    // Create runner (chain ID will be detected automatically)
    let mut runner = AAPocRunner::new(bundler_rpc, fork_url).await?;

    // Execute POC phases
    let contracts = runner.deploy_contracts().await?;
    info!("");

    runner.verify_bundler().await?;
    info!("");

    let receipt = runner.execute_user_operation(&contracts, owner_key).await?;
    info!("");

    runner.verify_execution(&contracts).await?;
    info!("");

    // Summary
    info!("====================================");
    info!("🎉 POC Complete!");
    info!("  UserOp Hash: {:?}", receipt.user_op_hash);
    info!("  Success: {}", receipt.success);
    info!("  Gas Used: {}", receipt.actual_gas_used);
    info!("  Actual Cost: {} wei", receipt.actual_gas_cost);

    Ok(())
}
