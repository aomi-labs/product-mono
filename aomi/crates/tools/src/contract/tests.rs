use super::session::{ContractConfig, ContractSession};
use alloy_primitives::{Bytes as AlloyBytes, U256, keccak256};
use anyhow::Result;
use foundry_config::Config;
use foundry_evm::inspectors::cheatcodes::BroadcastableTransactions;
use std::path::PathBuf;

/// Helper function to execute a generated forge script and return broadcastable transactions
async fn execute_forge_script(
    script_source: String,
    broadcast: bool,
) -> Result<BroadcastableTransactions> {
    // 1. Setup contract session with mainnet fork
    let rpc_url = std::env::var("AOMI_FORK_RPC")
        .unwrap_or_else(|_| "https://rpc.ankr.com/eth/2a9a32528f8a70a5b48c57e8fb83b4978f2a25c8368aa6fd9dc2f2321ae53362".to_string());

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let contract_root = manifest_dir.join("src/contract");

    let mut base_config = Config::with_root(&contract_root);
    base_config.libs.push(contract_root.join("lib"));

    let config = ContractConfig {
        foundry_config: std::sync::Arc::new(base_config),
        evm_opts: foundry_evm::opts::EvmOpts {
            fork_url: Some(rpc_url),
            fork_block_number: None,
            memory_limit: 128 * 1024 * 1024,
            ..Default::default()
        },
        initial_balance: Some(U256::from(10u64.pow(18))),
        ..Default::default()
    };

    let mut session = ContractSession::new(config).await?;

    // 2. Compile the forge script
    session.compile_source(
        "forge_script".to_string(),
        PathBuf::from("forge_script.sol"),
        script_source,
    )?;

    // 3. Deploy the forge_script contract
    let script_address = session
        .deploy_contract("forge_script", "forge_script")
        .await?;

    // 4. Execute the script's run() function
    let run_selector = AlloyBytes::from(keccak256("run()".as_bytes())[0..4].to_vec());
    let exec_result = session
        .call_contract(script_address, run_selector, None)
        .await?;

    if !exec_result.success {
        anyhow::bail!("Script execution failed");
    }

    // 5. Extract broadcastable transactions
    let transactions = session
        .get_broadcastable_transactions(&exec_result, broadcast)
        .await?;

    Ok(transactions)
}

/// Smoke-test session setup plus compilation
#[tokio::test]
async fn test_session_construction_and_execution() -> Result<()> {
    // 1. Create a single foundry config and wrap it in ContractConfig
    let foundry_config = Config::default();

    // 2. Build contract config with the foundry config
    let contract_config = ContractConfig::new(foundry_config, Some(String::from("test-session")));

    // 3. Create session
    let mut session = ContractSession::new(contract_config).await?;

    // 4. Compile a simple contract
    let contract_source = r#"
        // SPDX-License-Identifier: MIT
        pragma solidity ^0.8.0;

        contract SimpleStorage {
            uint256 public value;

            function setValue(uint256 _value) public {
                value = _value;
            }

            function getValue() public view returns (uint256) {
                return value;
            }
        }
    "#;

    session.compile_source(
        "test".to_string(),
        PathBuf::from("SimpleStorage.sol"),
        contract_source.to_string(),
    )?;

    // 5. Ensure compilation is cached and ABI accessible
    let compilation = session.get_compilation("test").expect("compilation cached");
    assert!(
        compilation.artifacts().next().is_some(),
        "Compilation output should contain artifacts"
    );

    let abi = session.get_contract_abi("test", "SimpleStorage")?;
    assert!(!abi.is_empty(), "ABI should not be empty");

    assert!(
        session
            .get_deployed_address("test", "SimpleStorage")
            .is_none(),
        "No deployment should be recorded yet"
    );

    Ok(())
}

/// Test that all components share the same foundry config
#[tokio::test]
async fn test_config_sharing() -> Result<()> {
    // Create a config with specific settings
    let foundry_config = Config {
        src: PathBuf::from("custom/src"),
        out: PathBuf::from("custom/out"),
        ..Default::default()
    };

    let mut contract_config =
        ContractConfig::new(foundry_config.clone(), Some(String::from("test-session")));
    contract_config.no_auto_detect = true;

    let session = ContractSession::new(contract_config).await?;

    // Verify compiler has the shared config
    assert_eq!(
        session.compiler.config.foundry_config.src,
        PathBuf::from("custom/src")
    );
    assert!(session.compiler.config.no_auto_detect);

    // Verify the config contents match (both Arc references point to equivalent config data)
    assert_eq!(
        session.compiler.config.foundry_config.as_ref(),
        session.config.foundry_config.as_ref()
    );

    Ok(())
}

/// Test default config construction
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires external RPC access"]
async fn test_session_on_mainnet_fork() -> Result<()> {
    let rpc_url = std::env::var("AOMI_FORK_RPC")
        .unwrap_or_else(|_| "https://rpc.ankr.com/eth/2a9a32528f8a70a5b48c57e8fb83b4978f2a25c8368aa6fd9dc2f2321ae53362".to_string());

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let contract_root = manifest_dir.join("src/contract");

    let mut base_config = Config::with_root(&contract_root);
    base_config.libs.push(contract_root.join("lib"));

    let config = ContractConfig {
        foundry_config: std::sync::Arc::new(base_config),
        evm_opts: foundry_evm::opts::EvmOpts {
            fork_url: Some(rpc_url.clone()),
            fork_block_number: None,
            memory_limit: 128 * 1024 * 1024,
            ..Default::default()
        },
        initial_balance: Some(U256::from(10u64.pow(18))),
        ..Default::default()
    };

    let mut session = ContractSession::new(config).await?;

    let contract_source = r#"
        // SPDX-License-Identifier: MIT
        pragma solidity ^0.8.0;

        contract Counter {
            uint256 public value;

            function setValue(uint256 newValue) external {
                value = newValue;
            }

            function getValue() external view returns (uint256) {
                return value;
            }
        }
    "#;

    session.compile_source(
        "fork_test".to_string(),
        PathBuf::from("Counter.sol"),
        contract_source.to_string(),
    )?;

    let _ = session.get_runner().await?;

    let address = session.deploy_contract("fork_test", "Counter").await?;

    fn encode_selector(sig: &str) -> [u8; 4] {
        let hash = keccak256(sig.as_bytes());
        [hash[0], hash[1], hash[2], hash[3]]
    }

    fn encode_u256(value: U256) -> [u8; 32] {
        value.to_be_bytes()
    }

    let mut set_calldata = Vec::with_capacity(36);
    set_calldata.extend_from_slice(&encode_selector("setValue(uint256)"));
    set_calldata.extend_from_slice(&encode_u256(U256::from(123u64)));

    fn decode_return(bytes: &[u8]) -> U256 {
        let mut buf = [0u8; 32];
        let len = bytes.len().min(32);
        buf[32 - len..].copy_from_slice(&bytes[bytes.len() - len..]);
        U256::from_be_bytes(buf)
    }

    let exec_result = session
        .call_contract(address, AlloyBytes::from(set_calldata), None)
        .await?;
    assert!(exec_result.success);

    let get_calldata = AlloyBytes::from(encode_selector("getValue()").to_vec());
    let static_result = session
        .call_contract_static(address, get_calldata, None)
        .await?;
    assert_eq!(decode_return(&static_result.returned), U256::from(123u64));

    Ok(())
}

/// Test default config construction
#[tokio::test]
async fn test_default_config() -> Result<()> {
    // Test creating a session with default config
    let config = ContractConfig::default();
    assert!(!config.no_auto_detect);
    assert!(!config.traces);
    assert_eq!(config.initial_balance, None);
    assert_eq!(config.id, None);

    let session = ContractSession::new(config).await?;
    assert!(session.get_all_compilations().is_empty());
    assert!(session.get_all_deployed().is_empty());

    Ok(())
}

/// Test generating and executing forge script via BAML with real USDC contract on mainnet fork
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires BAML server and external RPC access"]
async fn test_generate_forge_script_with_usdc_on_fork() -> Result<()> {
    use baml_client::{
        apis::{configuration::Configuration, default_api},
        models::{ContractInfo as BamlContractInfo, GenerateForgeScriptRequest},
    };

    // 1. Setup BAML client configuration
    let baml_config = Configuration {
        base_path: std::env::var("BAML_API_URL")
            .unwrap_or_else(|_| "http://localhost:2024".to_string()),
        ..Configuration::default()
    };

    // 2. Create contract context for USDC
    let contract_info = BamlContractInfo {
        description: Some("USDC stablecoin contract - USD Coin".to_string()),
        address: Some("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string()),
        abi: Some(r#"[
            {"constant":true,"inputs":[],"name":"name","outputs":[{"name":"","type":"string"}],"payable":false,"stateMutability":"view","type":"function"},
            {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"amount","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"payable":false,"stateMutability":"nonpayable","type":"function"},
            {"constant":true,"inputs":[],"name":"totalSupply","outputs":[{"name":"","type":"uint256"}],"payable":false,"stateMutability":"view","type":"function"},
            {"constant":false,"inputs":[{"name":"from","type":"address"},{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"name":"transferFrom","outputs":[{"name":"","type":"bool"}],"payable":false,"stateMutability":"nonpayable","type":"function"},
            {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"payable":false,"stateMutability":"view","type":"function"},
            {"constant":true,"inputs":[{"name":"owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"","type":"uint256"}],"payable":false,"stateMutability":"view","type":"function"},
            {"constant":true,"inputs":[],"name":"symbol","outputs":[{"name":"","type":"string"}],"payable":false,"stateMutability":"view","type":"function"},
            {"constant":false,"inputs":[{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"name":"transfer","outputs":[{"name":"","type":"bool"}],"payable":false,"stateMutability":"nonpayable","type":"function"},
            {"constant":true,"inputs":[{"name":"owner","type":"address"},{"name":"spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"payable":false,"stateMutability":"view","type":"function"}
        ]"#.to_string()),
        source_code: None,
    };

    // 3. Generate forge script from intent
    let user_intent = "Create a forge script that reads the total supply of USDC tokens";
    let request = GenerateForgeScriptRequest::new(contract_info, user_intent.to_string());

    let generated_script = default_api::generate_forge_script(&baml_config, request)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to generate forge script: {:?}", e))?;

    println!("Generated forge script:\n{}", generated_script);

    // 4. Execute the generated script using helper function
    let transactions = execute_forge_script(generated_script, false).await?;

    println!(
        "Successfully executed forge script and generated {} broadcastable transactions",
        transactions.len()
    );

    // 5. Display transaction details
    if !transactions.is_empty() {
        for (idx, tx) in transactions.iter().enumerate() {
            println!("  Transaction {}: {:?}", idx + 1, tx.transaction.to());
        }
    }

    Ok(())
}
