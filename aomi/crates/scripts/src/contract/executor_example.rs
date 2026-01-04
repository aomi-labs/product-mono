use anyhow::Result;
use alloy_primitives::{Address, Bytes, U256, hex};
use foundry_evm::{
    backend::Backend,
    fork::CreateFork,
    opts::EvmOpts,
    revm::JournalInner,
    AsEnvMut,
};
use super::{runner::ContractRunner, session::ContractConfig};

/// Example: Create executor and deploy/call a contract
pub async fn basic_executor_example() -> Result<()> {
    // 1. Create config
    let config = ContractConfig::default();
    
    // 2. Create runner (this builds the executor internally)
    let mut runner = ContractRunner::new(&config).await?;
    
    // 3. Deploy a simple contract (returns 42)
    let bytecode = Bytes::from(hex::decode("600a600c600039600a6000f3602a60005260206000f3")?);
    let (contract_addr, deploy_result) = runner.deploy(bytecode)?;
    println!("Deployed contract at: {:?}", contract_addr);
    assert!(deploy_result.success);
    
    // 4. Call the contract
    let call_result = runner.call(contract_addr, Bytes::new(), U256::ZERO)?;
    println!("Call result: {:?}", call_result.returned);
    
    Ok(())
}

/// Example: Switch networks during executor lifecycle
pub async fn executor_network_switching_example() -> Result<()> {
    use foundry_config::Config;
    
    // 1. Create backend with Ethereum fork
    let eth_rpc = std::env::var("ETH_RPC_URL")
        .unwrap_or_else(|_| "https://eth.llamarpc.com".to_string());
    let mut eth_opts = EvmOpts {
        fork_url: Some(eth_rpc.clone()),
        fork_block_number: Some(1001),
        ..Default::default()
    };
    let eth_env = eth_opts.evm_env().await?;
    let eth_fork = CreateFork {
        enable_caching: true,
        url: eth_rpc,
        env: eth_env.clone(),
        evm_opts: eth_opts,
    };
    
    let mut backend = tokio::task::spawn_blocking(move || {
        std::thread::spawn(move || Backend::spawn(Some(eth_fork)))
            .join()
            .unwrap()
    })
    .await??;
    let eth_fork_id = backend.active_fork_id().unwrap();
    
    // 2. Create Optimism fork
    let op_rpc = std::env::var("OP_RPC_URL")
        .unwrap_or_else(|_| "https://mainnet.optimism.io".to_string());
    let mut op_opts = EvmOpts {
        fork_url: Some(op_rpc.clone()),
        fork_block_number: Some(2030),
        ..Default::default()
    };
    let op_env = op_opts.evm_env().await?;
    let op_fork = CreateFork {
        enable_caching: true,
        url: op_rpc,
        env: op_env,
        evm_opts: op_opts,
    };
    let op_fork_id = backend.create_fork(op_fork)?;
    
    // 3. Create config and runner with the multi-fork backend
    let config = ContractConfig::new(Config::default(), None);
    let mut runner = ContractRunner::with_backend(&config, backend).await?;
    
    // 4. Deploy on Ethereum (current fork)
    println!("=== Deploying on Ethereum (Fork A) ===");
    let bytecode = Bytes::from(hex::decode("600a600c600039600a6000f3602a60005260206000f3")?);
    let (eth_contract, _) = runner.deploy(bytecode.clone())?;
    println!("Contract deployed on ETH at: {:?}", eth_contract);
    
    // 5. SWITCH TO OPTIMISM
    println!("\n=== Switching to Optimism (Fork B) ===");
    let mut journaled_state = JournalInner::new();
    runner.executor.backend_mut()
        .select_fork(op_fork_id, &mut runner.executor.env_mut().as_env_mut(), &mut journaled_state)?;
    println!("✓ Switched to Optimism fork");
    
    // 6. Deploy on Optimism (fresh state - ETH contract NOT visible)
    println!("\n=== Deploying on Optimism ===");
    let (op_contract, _) = runner.deploy(bytecode)?;
    println!("Contract deployed on OP at: {:?}", op_contract);
    
    // 7. SWITCH BACK TO ETHEREUM
    println!("\n=== Switching back to Ethereum ===");
    runner.executor.backend_mut()
        .select_fork(eth_fork_id, &mut runner.executor.env_mut().as_env_mut(), &mut journaled_state)?;
    println!("✓ Switched back to Ethereum");
    
    // 8. ETH contract still exists (state preserved)
    println!("\n=== Verifying ETH contract still exists ===");
    let call_result = runner.call(eth_contract, Bytes::new(), U256::ZERO)?;
    println!("ETH contract call result: {:?}", call_result.returned);
    assert!(call_result.success);
    
    Ok(())
}

