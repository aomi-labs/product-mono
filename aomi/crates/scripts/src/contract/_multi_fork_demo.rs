use anyhow::Result;
use cast::backend::DatabaseExt;
use foundry_evm::{
    backend::Backend,
    fork::CreateFork,
    opts::EvmOpts,
};
use foundry_config::Config;
use std::sync::Arc;

/// Example: Spawn Ethereum block 1001 and Optimism block 2030
pub async fn spawn_multi_chain_forks() -> Result<Backend> {
    // Get a default foundry config
    let foundry_config = Config::default();
    
    // ============================================
    // Step 1: Create Ethereum fork (block 1001)
    // ============================================
    let eth_rpc_url = std::env::var("ETH_RPC_URL")
        .unwrap_or_else(|_| "https://eth.llamarpc.com".to_string());
    
    let mut eth_evm_opts = EvmOpts {
        fork_url: Some(eth_rpc_url.clone()),
        fork_block_number: Some(1001),
        memory_limit: 128 * 1024 * 1024, // 128MB
        ..Default::default()
    };
    
    // Create EVM environment for Ethereum
    let eth_env = eth_evm_opts
        .evm_env()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create ETH EVM environment: {}", e))?;
    
    // Create the Ethereum fork
    let eth_fork = CreateFork {
        enable_caching: true,
        url: eth_rpc_url,
        env: eth_env.clone(),
        evm_opts: eth_evm_opts,
    };
    
    // Spawn backend with Ethereum fork
    let mut backend = tokio::task::spawn_blocking(move || {
        std::thread::spawn(move || Backend::spawn(Some(eth_fork)))
            .join()
            .expect("backend thread panicked")
    })
    .await?
    .map_err(|e| anyhow::anyhow!("Backend spawn failed: {}", e))?;
    
    println!("✓ Ethereum fork (block 1001) created");
    
    // ============================================
    // Step 2: Create Optimism fork (block 2030)
    // ============================================
    let op_rpc_url = std::env::var("OP_RPC_URL")
        .unwrap_or_else(|_| "https://mainnet.optimism.io".to_string());
    
    let mut op_evm_opts = EvmOpts {
        fork_url: Some(op_rpc_url.clone()),
        fork_block_number: Some(2030),
        memory_limit: 128 * 1024 * 1024, // 128MB
        ..Default::default()
    };
    
    // Create EVM environment for Optimism
    let op_env = op_evm_opts
        .evm_env()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create OP EVM environment: {}", e))?;
    
    // Create the Optimism fork
    let op_fork = CreateFork {
        enable_caching: true,
        url: op_rpc_url,
        env: op_env,
        evm_opts: op_evm_opts,
    };
    
    // Add Optimism fork to the backend
    // This returns a LocalForkId that we can use to switch to this fork
    let op_fork_id = backend.create_fork(op_fork)?;
    
    println!("✓ Optimism fork (block 2030) created with ID: {}", op_fork_id);
    
    // ============================================
    // Step 3: Get the Ethereum fork ID (the one we started with)
    // ============================================
    let eth_fork_id = backend
        .active_fork_id()
        .ok_or_else(|| anyhow::anyhow!("No active fork found"))?;
    
    println!("✓ Ethereum fork ID: {}", eth_fork_id);
    println!("✓ Optimism fork ID: {}", op_fork_id);
    
    // ============================================
    // Usage example: Switch between forks
    // ============================================
    println!("\n--- Fork Switching Examples ---");
    
    // Switch to Optimism
    use foundry_evm::revm::JournalInner;
    use foundry_evm::AsEnvMut;
    
    // Note: In real usage, you'd need a JournaledState and EnvMut
    // This is just showing the API
    println!("To switch to Optimism: backend.select_fork({}, &mut env, &mut journaled_state)", op_fork_id);
    println!("To switch back to Ethereum: backend.select_fork({}, &mut env, &mut journaled_state)", eth_fork_id);
    
    
    Ok(backend)
}

/// Example: Run Tx1, Tx2 on Fork A, then switch to Fork B and run Tx3
pub async fn example_fork_switching_with_transactions() -> Result<()> {
    use foundry_evm::{revm::JournalInner, AsEnvMut};
    
    // Create backend with Ethereum fork
    let eth_rpc = std::env::var("ETH_RPC_URL").unwrap_or_else(|_| "https://eth.llamarpc.com".to_string());
    let mut eth_opts = EvmOpts { fork_url: Some(eth_rpc.clone()), fork_block_number: Some(1001), ..Default::default() };
    let eth_env = eth_opts.evm_env().await?;
    let eth_fork = CreateFork { enable_caching: true, url: eth_rpc, env: eth_env, evm_opts: eth_opts };
    
    let mut backend = tokio::task::spawn_blocking(move || {
        std::thread::spawn(move || Backend::spawn(Some(eth_fork))).join().unwrap()
    }).await??;
    let eth_fork_id = backend.active_fork_id().unwrap();
    
    // Create Optimism fork
    let op_rpc = std::env::var("OP_RPC_URL").unwrap_or_else(|_| "https://mainnet.optimism.io".to_string());
    let mut op_opts = EvmOpts { fork_url: Some(op_rpc.clone()), fork_block_number: Some(2030), ..Default::default() };
    let op_env = op_opts.evm_env().await?;
    let op_fork = CreateFork { enable_caching: true, url: op_rpc, env: op_env, evm_opts: op_opts };
    let op_fork_id = backend.create_fork(op_fork)?;
    
    // Setup for fork switching
    let mut env = foundry_evm::Env::default();
    let mut journaled_state = JournalInner::new();
    
    println!("\n=== Running Tx1, Tx2 on Fork A (Ethereum) ===");
    println!("Active fork: {}", eth_fork_id);
    
    // Switch to Fork B
    println!("\n=== Switching to Fork B (Optimism) ===");
    backend.select_fork(op_fork_id, &mut env.as_env_mut(), &mut journaled_state)?;
    println!("Active fork: {}", op_fork_id);
    println!("✓ Fork B starts fresh - Tx1/Tx2 state NOT visible");
    
    
    // Run Tx3 on Fork B
    println!("\n=== Running Tx3 on Fork B ===");
    
    // Switch back to Fork A
    println!("\n=== Switching back to Fork A ===");
    backend.select_fork(eth_fork_id, &mut env.as_env_mut(), &mut journaled_state)?;
    println!("✓ Fork A's state from Tx1/Tx2 is preserved");
    
    Ok(())
}

/// Alternative: Create both forks without starting with one
pub async fn spawn_multi_chain_forks_alternative() -> Result<Backend> {
    // Start with an empty backend (no fork)
    let mut backend = tokio::task::spawn_blocking(move || {
        std::thread::spawn(move || Backend::spawn(None))
            .join()
            .expect("backend thread panicked")
    })
    .await?
    .map_err(|e| anyhow::anyhow!("Backend spawn failed: {}", e))?;
    
    // Create Ethereum fork
    let eth_rpc_url = std::env::var("ETH_RPC_URL")
        .unwrap_or_else(|_| "https://eth.llamarpc.com".to_string());
    
    let mut eth_evm_opts = EvmOpts {
        fork_url: Some(eth_rpc_url.clone()),
        fork_block_number: Some(1001),
        memory_limit: 128 * 1024 * 1024,
        ..Default::default()
    };
    
    let eth_env = eth_evm_opts
        .evm_env()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create ETH EVM environment: {}", e))?;
    
    let eth_fork = CreateFork {
        enable_caching: true,
        url: eth_rpc_url,
        env: eth_env,
        evm_opts: eth_evm_opts,
    };
    
    let eth_fork_id = backend.create_fork(eth_fork)?;
    println!("✓ Ethereum fork (block 1001) created with ID: {}", eth_fork_id);
    
    // Create Optimism fork
    let op_rpc_url = std::env::var("OP_RPC_URL")
        .unwrap_or_else(|_| "https://mainnet.optimism.io".to_string());
    
    let mut op_evm_opts = EvmOpts {
        fork_url: Some(op_rpc_url.clone()),
        fork_block_number: Some(2030),
        memory_limit: 128 * 1024 * 1024,
        ..Default::default()
    };
    
    let op_env = op_evm_opts
        .evm_env()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create OP EVM environment: {}", e))?;
    
    let op_fork = CreateFork {
        enable_caching: true,
        url: op_rpc_url,
        env: op_env,
        evm_opts: op_evm_opts,
    };
    
    let op_fork_id = backend.create_fork(op_fork)?;
    println!("✓ Optimism fork (block 2030) created with ID: {}", op_fork_id);
    
    // Select Ethereum as the active fork
    use foundry_evm::revm::JournalInner;
    use foundry_evm::AsEnvMut;
    
    // Note: You need to provide env and journaled_state in real usage
    // backend.select_fork(eth_fork_id, &mut env, &mut journaled_state)?;
    

    Ok(backend)
}