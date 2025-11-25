use super::session::{ContractConfig, ContractSession};
use alloy_primitives::{Bytes as AlloyBytes, U256, keccak256};
use anyhow::Result;
use foundry_config::Config;
use std::path::PathBuf;

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
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();

    let mut base_config = Config::with_root(&repo_root);
    base_config
        .libs
        .push(repo_root.join("crates/tools/src/contract/lib"));

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
