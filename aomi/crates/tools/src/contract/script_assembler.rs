use alloy_primitives::utils::parse_units;
use anyhow::{Result, anyhow};
use baml_client::models::{GeneratedScript, InterfaceSource};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use baml_client::models::InterfaceDefinition;

/// Funding required before executing operations
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "asset_type", rename_all = "snake_case")]
pub enum FundingRequirement {
    Eth {
        /// Amount in ether units (e.g., "10")
        amount: String,
    },
    Erc20 {
        /// Token contract address
        token_address: String,
        /// Human-readable amount (e.g., "1000.5")
        amount: String,
        /// Token decimals to convert the amount to base units
        decimals: u8,
    },
}

/// Configuration for script assembly
#[derive(Clone, Debug)]
pub struct AssemblyConfig {
    pub funding_requirements: Vec<FundingRequirement>,
    pub solidity_version: String, // Default: "^0.8.20"
}

impl Default for AssemblyConfig {
    fn default() -> Self {
        Self {
            funding_requirements: vec![FundingRequirement::Eth {
                amount: "10".to_string(),
            }],
            solidity_version: "^0.8.20".to_string(),
        }
    }
}

/// Script assembler - wraps transaction calls in executable Forge script
pub struct ScriptAssembler;

impl ScriptAssembler {
    pub fn assemble(
        contract_definitions: Vec<String>,
        generated: GeneratedScript,
        config: AssemblyConfig,
    ) -> Result<String> {
        let imports = Self::generate_imports(&generated.interfaces_needed);
        let inline_interfaces = Self::generate_inline_interfaces(&generated.interfaces_needed);
        let contract_defs = Self::generate_contract_definitions(&contract_definitions);
        let funding_setup = Self::generate_funding_setup(&config.funding_requirements)?;
        let transaction_calls = Self::generate_transaction_calls(&generated.transaction_calls);

        Ok(format!(
            r#"pragma solidity {version};
            
import {{Script}} from "forge-std/Script.sol";
import {{StdCheats}} from "forge-std/StdCheats.sol";
{imports}
{inline_interfaces}{contract_defs}
contract forge_script is Script, StdCheats {{

    function run() public {{
{funding_setup}
        vm.startBroadcast();

{transaction_calls}
        vm.stopBroadcast();
    }}
}}
"#,
            version = config.solidity_version,
            imports = imports,
            inline_interfaces = inline_interfaces,
            contract_defs = contract_defs,
            funding_setup = funding_setup,
            transaction_calls = transaction_calls,
        ))
    }

    fn generate_imports(interfaces: &[baml_client::models::InterfaceDefinition]) -> String {
        interfaces
            .iter()
            .filter_map(|interface| {
                if matches!(interface.source, InterfaceSource::ForgeStd) {
                    Some(format!(
                        "import {{{}}} from \"forge-std/interfaces/{}.sol\";",
                        interface.name, interface.name
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn generate_inline_interfaces(
        interfaces: &[baml_client::models::InterfaceDefinition],
    ) -> String {
        let code = interfaces
            .iter()
            .filter_map(|interface| {
                if matches!(interface.source, InterfaceSource::Inline) {
                    interface.solidity_code.clone()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        Self::wrap_with_newlines(&code)
    }

    fn generate_contract_definitions(definitions: &[String]) -> String {
        Self::wrap_with_newlines(&definitions.join("\n\n"))
    }

    fn generate_funding_setup(requirements: &[FundingRequirement]) -> Result<String> {
        if requirements.is_empty() {
            return Ok("        deal(msg.sender, 10 ether);".to_string());
        }

        requirements
            .iter()
            .map(|req| match req {
                FundingRequirement::Eth { amount } => {
                    let sanitized = Self::sanitize_eth_amount(amount)?;
                    Ok(format!("        deal(msg.sender, {} ether);", sanitized))
                }
                FundingRequirement::Erc20 {
                    token_address,
                    amount,
                    decimals,
                } => {
                    let amount_wei = Self::format_erc20_amount(amount, *decimals)?;
                    Ok(format!(
                        "        deal({}, msg.sender, {});",
                        token_address, amount_wei
                    ))
                }
            })
            .collect::<Result<Vec<_>>>()
            .map(|lines| lines.join("\n"))
    }

    fn generate_transaction_calls(calls: &[baml_client::models::TransactionCall]) -> String {
        calls
            .iter()
            .map(|call| {
                let indented_code = Self::indent_lines(&call.solidity_code, 8);
                format!("        // {}\n{}", call.description, indented_code)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Indent each line of code by a specified number of spaces
    fn indent_lines(code: &str, spaces: usize) -> String {
        let indent = " ".repeat(spaces);
        code.lines()
            .map(|line| format!("{}{}", indent, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Wrap non-empty string with newlines, return empty string if empty
    fn wrap_with_newlines(s: &str) -> String {
        if s.is_empty() {
            String::new()
        } else {
            format!("\n{}\n", s)
        }
    }

    /// Parse ERC20 token amount with decimals to base units (wei equivalent)
    fn format_erc20_amount(amount: &str, decimals: u8) -> Result<String> {
        let parsed = parse_units(amount, decimals)
            .map_err(|e| anyhow!("Invalid ERC20 amount '{}': {}", amount, e))?;
        Ok(parsed.to_string())
    }

    /// Validate and sanitize ETH amount string
    fn sanitize_eth_amount(amount: &str) -> Result<String> {
        let trimmed = amount.trim();
        // Validate by attempting to parse (ETH has 18 decimals, but we just check validity)
        parse_units(trimmed, 18).map_err(|e| anyhow!("Invalid ETH amount '{}': {}", trimmed, e))?;
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::session::{ContractConfig, ContractSession};
    use alloy_primitives::{Bytes, U256, keccak256};
    use baml_client::models::TransactionCall;
    use foundry_config::Config as FoundryConfig;
    use foundry_evm::opts::EvmOpts;
    use std::path::PathBuf;

    #[test]
    fn test_script_assembler() {
        let generated = GeneratedScript {
            transaction_calls: vec![TransactionCall {
                solidity_code: "SimpleToken token = new SimpleToken(\"Test\", \"TST\", 1000000);"
                    .to_string(),
                description: "Deploy SimpleToken".to_string(),
            }],
            interfaces_needed: vec![],
        };

        let script =
            ScriptAssembler::assemble(vec![], generated, AssemblyConfig::default()).unwrap();

        assert!(script.contains("pragma solidity"));
        assert!(script.contains("import {Script} from \"forge-std/Script.sol\""));
        assert!(script.contains("import {StdCheats} from \"forge-std/StdCheats.sol\""));
        assert!(script.contains("contract forge_script"));
        assert!(script.contains("deal(msg.sender, 10 ether);"));
        assert!(script.contains("vm.startBroadcast()"));
        assert!(script.contains("vm.stopBroadcast()"));
        assert!(script.contains("SimpleToken token"));
    }

    #[test]
    fn test_script_assembler_with_interfaces() {
        let generated = GeneratedScript {
            transaction_calls: vec![TransactionCall {
                solidity_code: "token.approve(address(router), 1000 ether);".to_string(),
                description: "Approve router".to_string(),
            }],
            interfaces_needed: vec![
                InterfaceDefinition {
                    name: "IERC20".to_string(),
                    functions: vec![],
                    source: InterfaceSource::ForgeStd,
                    solidity_code: None,
                },
                InterfaceDefinition {
                    name: "IUniswapV2Router02".to_string(),
                    functions: vec![],
                    source: InterfaceSource::Inline,
                    solidity_code: Some(
                        "interface IUniswapV2Router02 {\n    function addLiquidityETH(address token, uint amountTokenDesired, uint amountTokenMin, uint amountETHMin, address to, uint deadline) external payable returns (uint, uint, uint);\n}".to_string()
                    ),
                },
            ],
        };

        let script =
            ScriptAssembler::assemble(vec![], generated, AssemblyConfig::default()).unwrap();

        assert!(script.contains("import {IERC20} from \"forge-std/interfaces/IERC20.sol\""));
        assert!(script.contains("interface IUniswapV2Router02"));
        assert!(script.contains("function addLiquidityETH"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_full_script_execution() {
        // 1. Generate a simple script using ScriptAssembler
        let generated = baml_client::models::GeneratedScript {
            transaction_calls: vec![TransactionCall {
                solidity_code: "payable(address(0x1234567890123456789012345678901234567890)).transfer(1 ether);"
                    .to_string(),
                description: "Send 1 ETH to test address".to_string(),
            }],
            interfaces_needed: vec![],
        };

        let script = ScriptAssembler::assemble(vec![], generated, AssemblyConfig::default())
            .expect("Failed to assemble script");

        // Verify script was generated correctly
        assert!(script.contains("pragma solidity"));
        assert!(script.contains("vm.startBroadcast()"));
        assert!(script.contains("transfer(1 ether)"));

        // 2. Setup contract session (in-memory EVM)
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let contract_root = manifest_dir.join("src/contract");

        let mut base_config = FoundryConfig::with_root(&contract_root);
        base_config.libs.push(contract_root.join("lib"));

        let contract_config = ContractConfig {
            foundry_config: std::sync::Arc::new(base_config),
            evm_opts: EvmOpts {
                fork_url: None, // In-memory EVM, no fork
                fork_block_number: None,
                memory_limit: 128 * 1024 * 1024,
                ..Default::default()
            },
            initial_balance: Some(U256::from(10u64.pow(18))),
            ..Default::default()
        };

        let mut session = ContractSession::new(contract_config)
            .await
            .expect("Failed to create contract session");

        // 3. Compile the generated script
        let script_path = PathBuf::from("forge_script.sol");
        session
            .compile_source("forge_script".to_string(), script_path, script.clone())
            .expect("Failed to compile script");

        let compilation = session
            .get_compilation("forge_script")
            .expect("Compilation not found");

        let bytecode_vec = session
            .compiler
            .get_contract_bytecode(compilation, "forge_script")
            .expect("Failed to get bytecode");
        let bytecode = Bytes::from(bytecode_vec);

        // 4. Deploy and execute the script
        let runner = session.get_runner().await.expect("Failed to get runner");
        let (script_address, _) = runner.deploy(bytecode).expect("Failed to deploy script");

        // 5. Call the run() function
        let run_selector = Bytes::from(keccak256("run()".as_bytes())[0..4].to_vec());
        let execution_result = runner
            .call(script_address, run_selector, U256::ZERO)
            .expect("Failed to execute script");

        // 6. Verify execution succeeded
        assert!(execution_result.success, "Script execution should succeed");
        assert_eq!(
            execution_result.broadcastable_transactions.len(),
            1,
            "Should have exactly one transaction"
        );

        // 7. Verify the transaction details
        let tx = &execution_result.broadcastable_transactions[0];
        assert!(
            tx.transaction.from().is_some(),
            "Transaction should have a from address"
        );
        assert!(
            tx.transaction.to().is_some(),
            "Transaction should have a to address"
        );

        // Verify value is 1 ether
        let value = tx.transaction.value().unwrap_or(U256::ZERO);
        assert_eq!(
            value,
            U256::from(10u64.pow(18)),
            "Transaction value should be 1 ether"
        );

        println!("✅ Full integration test passed: assemble → compile → deploy → execute");
    }
}
