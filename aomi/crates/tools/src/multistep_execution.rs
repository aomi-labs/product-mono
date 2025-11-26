use alloy_primitives::{Bytes as AlloyBytes, U256, keccak256};
use anyhow::Result;
use baml_client::{
    apis::{configuration::Configuration, default_api},
    models::{ContractInfo as BamlContractInfo, GenerateForgeScriptRequest},
};
use foundry_config::Config;
use foundry_evm::inspectors::cheatcodes::BroadcastableTransactions;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::contract::session::{ContractConfig, ContractSession};

/// Parameters for executing a multi-step intent
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecuteMultiStepIntentParameters {
    /// Short description of what this execution is for
    pub topic: String,
    /// The user's intent describing what they want to accomplish
    pub user_intent: String,
    /// Optional contract context (address, ABI, source code)
    pub contract_context: Option<ContractContext>,
    /// RPC URL for forking (defaults to Ankr public RPC)
    pub fork_url: Option<String>,
    /// Optional fork block number (defaults to latest)
    pub fork_block_number: Option<u64>,
}

/// Contract context for multi-step execution
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractContext {
    /// Description of the contract
    pub description: Option<String>,
    /// Contract address
    pub address: Option<String>,
    /// Contract ABI as JSON string
    pub abi: Option<String>,
    /// Contract source code
    pub source_code: Option<String>,
}

/// Result of executing a multi-step intent
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MultiStepExecutionResult {
    /// Whether the execution was successful
    pub success: bool,
    /// Generated forge script
    pub generated_script: String,
    /// Number of broadcastable transactions
    pub transaction_count: usize,
    /// Serialized broadcastable transactions
    pub transactions: Vec<TransactionInfo>,
    /// Any error message if execution failed
    pub error: Option<String>,
}

/// Information about a broadcastable transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionInfo {
    /// Transaction index
    pub index: usize,
    /// From address
    pub from: String,
    /// To address (None for contract creation)
    pub to: Option<String>,
    /// Transaction value in wei
    pub value: String,
    /// Input data (calldata)
    pub data: String,
    /// Gas limit (if available)
    pub gas_limit: Option<String>,
    /// RPC URL (if specified)
    pub rpc: Option<String>,
}

impl From<ContractContext> for BamlContractInfo {
    fn from(ctx: ContractContext) -> Self {
        BamlContractInfo {
            description: ctx.description,
            address: ctx.address,
            abi: ctx.abi,
            source_code: ctx.source_code,
        }
    }
}

/// Tool struct for multi-step intent execution
pub struct ExecuteMultiStepIntent;

impl ExecuteMultiStepIntent {
    /// Execute a multi-step intent and return broadcastable transactions
    pub async fn execute(
        params: ExecuteMultiStepIntentParameters,
    ) -> Result<MultiStepExecutionResult> {
        // 1. Setup BAML client configuration
        let baml_config = Configuration {
            base_path: std::env::var("BAML_API_URL")
                .unwrap_or_else(|_| "http://localhost:2024".to_string()),
            ..Configuration::default()
        };

        // 2. Create contract context for BAML (or use empty context)
        let baml_contract_info = params
            .contract_context
            .clone()
            .map(BamlContractInfo::from)
            .unwrap_or_else(|| BamlContractInfo {
                description: Some("No specific contract context provided".to_string()),
                address: None,
                abi: None,
                source_code: None,
            });

        // 3. Generate forge script from intent
        let request =
            GenerateForgeScriptRequest::new(baml_contract_info, params.user_intent.clone());
        let generated_script = match default_api::generate_forge_script(&baml_config, request).await
        {
            Ok(script) => script,
            Err(e) => {
                return Ok(MultiStepExecutionResult {
                    success: false,
                    generated_script: String::new(),
                    transaction_count: 0,
                    transactions: vec![],
                    error: Some(format!("Failed to generate forge script: {:?}", e)),
                });
            }
        };

        // 4. Execute the generated script
        match Self::execute_forge_script(
            generated_script.clone(),
            params.fork_url,
            params.fork_block_number,
        )
        .await
        {
            Ok(transactions) => {
                let transaction_info = Self::serialize_transactions(&transactions);
                Ok(MultiStepExecutionResult {
                    success: true,
                    generated_script,
                    transaction_count: transactions.len(),
                    transactions: transaction_info,
                    error: None,
                })
            }
            Err(e) => Ok(MultiStepExecutionResult {
                success: false,
                generated_script,
                transaction_count: 0,
                transactions: vec![],
                error: Some(format!("Script execution failed: {}", e)),
            }),
        }
    }

    /// Execute a generated forge script and return broadcastable transactions
    async fn execute_forge_script(
        script_source: String,
        fork_url: Option<String>,
        fork_block_number: Option<u64>,
    ) -> Result<BroadcastableTransactions> {
        // 1. Setup contract session with mainnet fork
        let rpc_url = fork_url.unwrap_or_else(|| {
            std::env::var("AOMI_FORK_RPC")
                .unwrap_or_else(|_| "https://rpc.ankr.com/eth/2a9a32528f8a70a5b48c57e8fb83b4978f2a25c8368aa6fd9dc2f2321ae53362".to_string())
        });

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let contract_root = manifest_dir.join("src/contract");

        let mut base_config = Config::with_root(&contract_root);
        base_config.libs.push(contract_root.join("lib"));

        let config = ContractConfig {
            foundry_config: std::sync::Arc::new(base_config),
            evm_opts: foundry_evm::opts::EvmOpts {
                fork_url: Some(rpc_url),
                fork_block_number,
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

        // 5. Extract broadcastable transactions (without broadcasting)
        let transactions = session
            .get_broadcastable_transactions(&exec_result, false)
            .await?;

        Ok(transactions)
    }

    /// Serialize broadcastable transactions into a more portable format
    fn serialize_transactions(transactions: &BroadcastableTransactions) -> Vec<TransactionInfo> {
        transactions
            .iter()
            .enumerate()
            .map(|(idx, btx)| {
                let from = btx
                    .transaction
                    .from()
                    .map(|addr| format!("{:?}", addr))
                    .unwrap_or_else(|| "unknown".to_string());

                let to = btx.transaction.to().and_then(|to_kind| match to_kind {
                    alloy_primitives::TxKind::Call(addr) => Some(format!("{:?}", addr)),
                    alloy_primitives::TxKind::Create => None,
                });

                let value = btx
                    .transaction
                    .value()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "0".to_string());

                let data = btx
                    .transaction
                    .input()
                    .map(|input| format!("0x{}", alloy_primitives::hex::encode(input)))
                    .unwrap_or_else(|| "0x".to_string());

                // Note: gas_limit is not directly available on TransactionMaybeSigned
                // We'll set it to None for now
                let gas_limit: Option<String> = None;

                TransactionInfo {
                    index: idx,
                    from,
                    to,
                    value,
                    data,
                    gas_limit,
                    rpc: btx.rpc.clone(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_contract_context_conversion() {
        let ctx = ContractContext {
            description: Some("Test contract".to_string()),
            address: Some("0x123".to_string()),
            abi: Some("[]".to_string()),
            source_code: Some("contract Test {}".to_string()),
        };

        let baml_ctx: BamlContractInfo = ctx.clone().into();
        assert_eq!(baml_ctx.description, ctx.description);
        assert_eq!(baml_ctx.address, ctx.address);
        assert_eq!(baml_ctx.abi, ctx.abi);
        assert_eq!(baml_ctx.source_code, ctx.source_code);
    }

    #[tokio::test]
    async fn test_empty_transaction_serialization() {
        use std::collections::VecDeque;

        let transactions = VecDeque::new();
        let serialized = ExecuteMultiStepIntent::serialize_transactions(&transactions);

        assert_eq!(serialized.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires BAML server and external RPC access"]
    async fn test_execute_multistep_intent() -> Result<()> {
        let params = ExecuteMultiStepIntentParameters {
            topic: "Test USDC total supply".to_string(),
            user_intent: "Create a forge script that reads the total supply of USDC tokens".to_string(),
            contract_context: Some(ContractContext {
                description: Some("USDC stablecoin contract".to_string()),
                address: Some("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string()),
                abi: Some(r#"[
                    {"constant":true,"inputs":[],"name":"totalSupply","outputs":[{"name":"","type":"uint256"}],"payable":false,"stateMutability":"view","type":"function"}
                ]"#.to_string()),
                source_code: None,
            }),
            fork_url: None,
            fork_block_number: None,
        };

        let result = ExecuteMultiStepIntent::execute(params).await?;
        assert!(result.success, "Execution should succeed");
        assert!(
            !result.generated_script.is_empty(),
            "Script should be generated"
        );
        println!("Generated script:\n{}", result.generated_script);
        println!("Transaction count: {}", result.transaction_count);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires BAML server and external RPC access"]
    async fn test_deploy_erc20_with_uniswap_liquidity() -> Result<()> {
        // ERC20 token source code with Ownable
        let erc20_source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract AomiCoin {
    string public name = "AomiCoin";
    string public symbol = "AOM";
    uint8 public decimals = 18;
    uint256 public totalSupply;
    address public owner;

    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    modifier onlyOwner() {
        require(msg.sender == owner, "Not owner");
        _;
    }

    constructor(uint256 _initialSupply) {
        owner = msg.sender;
        totalSupply = _initialSupply * 10**decimals;
        balanceOf[msg.sender] = totalSupply;
        emit Transfer(address(0), msg.sender, totalSupply);
    }

    function transfer(address to, uint256 value) public returns (bool) {
        require(balanceOf[msg.sender] >= value, "Insufficient balance");
        balanceOf[msg.sender] -= value;
        balanceOf[to] += value;
        emit Transfer(msg.sender, to, value);
        return true;
    }

    function approve(address spender, uint256 value) public returns (bool) {
        allowance[msg.sender][spender] = value;
        emit Approval(msg.sender, spender, value);
        return true;
    }

    function transferFrom(address from, address to, uint256 value) public returns (bool) {
        require(balanceOf[from] >= value, "Insufficient balance");
        require(allowance[from][msg.sender] >= value, "Insufficient allowance");
        balanceOf[from] -= value;
        balanceOf[to] += value;
        allowance[from][msg.sender] -= value;
        emit Transfer(from, to, value);
        return true;
    }

    function transferOwnership(address newOwner) public onlyOwner {
        require(newOwner != address(0), "New owner is zero address");
        address oldOwner = owner;
        owner = newOwner;
        emit OwnershipTransferred(oldOwner, newOwner);
    }
}
        "#;

        let erc20_abi = r#"[
            {"inputs":[{"internalType":"uint256","name":"_initialSupply","type":"uint256"}],"stateMutability":"nonpayable","type":"constructor"},
            {"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"owner","type":"address"},{"indexed":true,"internalType":"address","name":"spender","type":"address"},{"indexed":false,"internalType":"uint256","name":"value","type":"uint256"}],"name":"Approval","type":"event"},
            {"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"previousOwner","type":"address"},{"indexed":true,"internalType":"address","name":"newOwner","type":"address"}],"name":"OwnershipTransferred","type":"event"},
            {"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"from","type":"address"},{"indexed":true,"internalType":"address","name":"to","type":"address"},{"indexed":false,"internalType":"uint256","name":"value","type":"uint256"}],"name":"Transfer","type":"event"},
            {"inputs":[{"internalType":"address","name":"","type":"address"},{"internalType":"address","name":"","type":"address"}],"name":"allowance","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},
            {"inputs":[{"internalType":"address","name":"spender","type":"address"},{"internalType":"uint256","name":"value","type":"uint256"}],"name":"approve","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},
            {"inputs":[{"internalType":"address","name":"","type":"address"}],"name":"balanceOf","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},
            {"inputs":[],"name":"decimals","outputs":[{"internalType":"uint8","name":"","type":"uint8"}],"stateMutability":"view","type":"function"},
            {"inputs":[],"name":"name","outputs":[{"internalType":"string","name":"","type":"string"}],"stateMutability":"view","type":"function"},
            {"inputs":[],"name":"owner","outputs":[{"internalType":"address","name":"","type":"address"}],"stateMutability":"view","type":"function"},
            {"inputs":[],"name":"symbol","outputs":[{"internalType":"string","name":"","type":"string"}],"stateMutability":"view","type":"function"},
            {"inputs":[],"name":"totalSupply","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},
            {"inputs":[{"internalType":"address","name":"to","type":"address"},{"internalType":"uint256","name":"value","type":"uint256"}],"name":"transfer","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},
            {"inputs":[{"internalType":"address","name":"from","type":"address"},{"internalType":"address","name":"to","type":"address"},{"internalType":"uint256","name":"value","type":"uint256"}],"name":"transferFrom","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},
            {"inputs":[{"internalType":"address","name":"newOwner","type":"address"}],"name":"transferOwnership","outputs":[],"stateMutability":"nonpayable","type":"function"}
        ]"#;

        // Realistic user intent - just what a user would actually say
        let user_intent = "Deploy an ERC20 token AomiCoin (AOM) with 1,000,000 supply, add 10 ETH liquidity on Uniswap V2, and transfer ownership to 0x1234567890123456789012345678901234567890.".to_string();

        let params = ExecuteMultiStepIntentParameters {
            topic: "Deploy AomiCoin and add Uniswap liquidity".to_string(),
            user_intent,
            contract_context: Some(ContractContext {
                description: Some("AomiCoin ERC20 token with ownership functionality".to_string()),
                address: None, // No address yet since we're deploying it
                abi: Some(erc20_abi.to_string()),
                source_code: Some(erc20_source.to_string()),
            }),
            fork_url: None,          // Will use default mainnet fork
            fork_block_number: None, // Will use latest block
        };

        println!("\n=== Starting Multi-Step ERC20 + Uniswap Test ===\n");
        println!("Intent: Deploy AomiCoin, add Uniswap liquidity, transfer ownership");

        let result = ExecuteMultiStepIntent::execute(params).await?;

        println!("\n=== Execution Result ===");
        println!("Success: {}", result.success);
        println!("Transaction count: {}", result.transaction_count);

        if let Some(ref error) = result.error {
            println!("Error: {}", error);
        }

        println!("\n=== Generated Forge Script ===");
        println!("{}", result.generated_script);

        println!("\n=== Broadcastable Transactions ===");
        for (idx, tx) in result.transactions.iter().enumerate() {
            println!("\nTransaction {}:", idx + 1);
            println!("  From: {}", tx.from);
            println!(
                "  To: {}",
                tx.to.as_ref().unwrap_or(&"[Contract Creation]".to_string())
            );
            println!("  Value: {} wei", tx.value);
            println!("  Data length: {} bytes", tx.data.len());
            if let Some(ref rpc) = tx.rpc {
                println!("  RPC: {}", rpc);
            }
            // Print first few bytes of data for contract calls
            if tx.data.len() > 10 {
                let data_preview = &tx.data[..std::cmp::min(42, tx.data.len())];
                println!("  Data (first bytes): {}...", data_preview);
            }
        }

        // Assertions
        assert!(
            result.success,
            "Multi-step execution should succeed. Error: {:?}",
            result.error
        );
        assert!(
            !result.generated_script.is_empty(),
            "Script should be generated"
        );

        // We expect multiple transactions:
        // 1. Deploy AomiCoin contract
        // 2. Approve Uniswap router
        // 3. Add liquidity (with ETH value)
        // 4. Transfer ownership
        assert!(
            result.transaction_count >= 4,
            "Expected at least 4 transactions (deploy, approve, addLiquidity, transferOwnership), got {}",
            result.transaction_count
        );

        // Verify at least one transaction has ETH value (the addLiquidity call)
        let has_eth_value = result.transactions.iter().any(|tx| tx.value != "0");
        assert!(
            has_eth_value,
            "Expected at least one transaction with ETH value for addLiquidity"
        );

        println!("\n=== Test Complete ===");
        println!("✓ Successfully generated and simulated multi-step transactions");
        println!("✓ Ready for broadcasting via account abstraction");

        Ok(())
    }
}
