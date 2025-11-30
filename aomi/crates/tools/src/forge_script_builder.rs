use alloy_primitives::{Address, Bytes, U256, keccak256};
use anyhow::{Result, anyhow};
use baml_client::{
    apis::{configuration::Configuration, default_api},
    models::{
        GenerateTransactionCallsRequest, GeneratedScript, InterfaceDefinition, InterfaceSource,
        Operation, Parameter,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use crate::contract::session::{ContractConfig, ContractSession};
use foundry_config::Config as FoundryConfig;
use foundry_evm::opts::EvmOpts;

// Newline constants
const NL: &str = "\n";
const NL2: &str = "\n\n";

// Script template constants
const SCRIPT_IMPORT: &str = "import {Script} from \"forge-std/Script.sol\";";
const STD_CHEATS_IMPORT: &str = "import {StdCheats} from \"forge-std/StdCheats.sol\";";

const CONTRACT_HEADER: &str = "contract forge_script is Script, StdCheats {";
const RUN_FUNCTION_HEADER: &str = "    function run() public {";
const VM_START_BROADCAST: &str = "        vm.startBroadcast();";
const VM_STOP_BROADCAST: &str = "        vm.stopBroadcast();";
const FUNCTION_FOOTER: &str = "    }";
const CONTRACT_FOOTER: &str = "}";

// Indentation constants
const INDENT_L1: &str = "        "; // 8 spaces - inside run() function
const INDENT_COMMENT: &str = "        // ";

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
        let mut script = String::new();

        Self::add_pragma(&mut script, &config);
        Self::add_imports(&mut script, &generated.interfaces_needed);
        Self::add_inline_interfaces(&mut script, &generated.interfaces_needed);
        Self::add_contract_definitions(&mut script, contract_definitions);
        Self::add_forge_script_wrapper(&mut script, &generated, &config)?;

        Ok(script)
    }

    fn add_pragma(script: &mut String, config: &AssemblyConfig) {
        script.push_str(&format!("pragma solidity {};", config.solidity_version));
        script.push_str(NL2);
    }

    fn add_imports(script: &mut String, interfaces: &[InterfaceDefinition]) {
        script.push_str(SCRIPT_IMPORT);
        script.push_str(NL);
        script.push_str(STD_CHEATS_IMPORT);
        script.push_str(NL);

        for interface in interfaces {
            if matches!(interface.source, InterfaceSource::ForgeStd) {
                script.push_str(&format!(
                    "import {{{}}} from \"forge-std/interfaces/{}.sol\";",
                    interface.name, interface.name
                ));
                script.push_str(NL);
            }
        }
        script.push_str(NL);
    }

    fn add_inline_interfaces(script: &mut String, interfaces: &[InterfaceDefinition]) {
        for interface in interfaces {
            if matches!(interface.source, InterfaceSource::Inline) {
                if let Some(code) = &interface.solidity_code {
                    script.push_str(code);
                    script.push_str(NL2);
                }
            }
        }
    }

    fn add_contract_definitions(script: &mut String, contract_definitions: Vec<String>) {
        for contract_def in contract_definitions {
            script.push_str(&contract_def);
            script.push_str(NL2);
        }
    }

    fn add_forge_script_wrapper(
        script: &mut String,
        generated: &GeneratedScript,
        config: &AssemblyConfig,
    ) -> Result<()> {
        // Contract header
        script.push_str(CONTRACT_HEADER);
        script.push_str(NL);
        script.push_str(NL);

        // Run function header
        script.push_str(RUN_FUNCTION_HEADER);
        script.push_str(NL);

        // Setup: fund sender
        Self::add_funding_setup(script, &config.funding_requirements)?;
        script.push_str(NL);
        script.push_str(VM_START_BROADCAST);
        script.push_str(NL2);

        // Transaction calls
        Self::add_transaction_calls(script, &generated.transaction_calls);

        // Close broadcast and function
        script.push_str(VM_STOP_BROADCAST);
        script.push_str(NL);
        script.push_str(FUNCTION_FOOTER);
        script.push_str(NL);
        script.push_str(CONTRACT_FOOTER);
        script.push_str(NL);

        Ok(())
    }

    fn add_transaction_calls(script: &mut String, calls: &[baml_client::models::TransactionCall]) {
        for tx_call in calls {
            // Add comment
            script.push_str(INDENT_COMMENT);
            script.push_str(&tx_call.description);
            script.push_str(NL);

            // Add indented solidity code
            for line in tx_call.solidity_code.lines() {
                script.push_str(INDENT_L1);
                script.push_str(line);
                script.push_str(NL);
            }
            script.push_str(NL);
        }
    }

    fn add_funding_setup(script: &mut String, funding: &[FundingRequirement]) -> Result<()> {
        if funding.is_empty() {
            script.push_str("        deal(msg.sender, 10 ether);");
            script.push_str(NL);
            return Ok(());
        }

        for requirement in funding {
            match requirement {
                FundingRequirement::Eth { amount } => {
                    let sanitized = Self::sanitize_eth_amount(amount)
                        .map_err(|e| anyhow!("Invalid ETH funding amount {}: {}", amount, e))?;
                    script.push_str(&format!("        deal(msg.sender, {} ether);", sanitized));
                    script.push_str(NL);
                }
                FundingRequirement::Erc20 {
                    token_address,
                    amount,
                    decimals,
                } => {
                    let amount_wei = Self::format_erc20_amount(amount, *decimals).map_err(|e| {
                        anyhow!(
                            "Invalid ERC20 funding amount {} (decimals {}): {}",
                            amount,
                            decimals,
                            e
                        )
                    })?;
                    script.push_str(&format!(
                        "        deal({}, msg.sender, {});",
                        token_address, amount_wei
                    ));
                    script.push_str(NL);
                }
            }
        }

        Ok(())
    }

    fn format_erc20_amount(amount: &str, decimals: u8) -> Result<String> {
        let trimmed = amount.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("amount cannot be empty"));
        }

        let mut parts = trimmed.split('.');
        let int_part = parts.next().unwrap_or("");
        let frac_part = parts.next();
        if parts.next().is_some() {
            return Err(anyhow!("amount has more than one decimal point"));
        }

        if decimals == 0 && frac_part.is_some() {
            return Err(anyhow!("token does not support fractional amounts"));
        }

        let clean_int = int_part.replace('_', "");
        if !clean_int.is_empty() && !clean_int.chars().all(|c| c.is_ascii_digit()) {
            return Err(anyhow!("invalid characters in integer part"));
        }

        let int_value = if clean_int.is_empty() {
            U256::ZERO
        } else {
            U256::from_str_radix(&clean_int, 10)
                .map_err(|_| anyhow!("failed to parse integer part"))?
        };

        let pow = Self::pow10(decimals);
        let mut total = int_value
            .checked_mul(pow)
            .ok_or_else(|| anyhow!("amount overflow"))?;

        if let Some(frac_raw) = frac_part {
            let clean_frac = frac_raw.replace('_', "");
            if !clean_frac.chars().all(|c| c.is_ascii_digit()) {
                return Err(anyhow!("invalid characters in fractional part"));
            }
            if clean_frac.len() > decimals as usize {
                return Err(anyhow!(
                    "fractional precision {} exceeds token decimals {}",
                    clean_frac.len(),
                    decimals
                ));
            }
            let mut frac_digits = clean_frac;
            let pad_len = decimals as usize - frac_digits.len();
            for _ in 0..pad_len {
                frac_digits.push('0');
            }
            if !frac_digits.is_empty() {
                let frac_value = U256::from_str_radix(&frac_digits, 10)
                    .map_err(|_| anyhow!("failed to parse fractional part"))?;
                total = total
                    .checked_add(frac_value)
                    .ok_or_else(|| anyhow!("amount overflow"))?;
            }
        }

        Ok(total.to_string())
    }

    fn pow10(decimals: u8) -> U256 {
        let mut result = U256::from(1u8);
        let ten = U256::from(10u8);
        for _ in 0..decimals {
            result = result * ten;
        }
        result
    }

    fn sanitize_eth_amount(amount: &str) -> Result<String> {
        let trimmed = amount.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("amount cannot be empty"));
        }
        if trimmed.matches('.').count() > 1 {
            return Err(anyhow!("multiple decimal points not allowed"));
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '_')
        {
            return Err(anyhow!("invalid characters in amount"));
        }
        Ok(trimmed.to_string())
    }
}

/// Parameters for ForgeScriptBuilder tool
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForgeScriptBuilderParameters {
    pub operations: Vec<Operation>,
    pub available_interfaces: Vec<InterfaceDefinition>,
    pub funding_requirements: Option<Vec<FundingRequirement>>,
}

/// Serializable transaction data
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionData {
    pub from: Option<String>,
    pub to: Option<String>,
    pub value: String,
    pub data: String,
    pub rpc_url: Option<String>,
}

/// Result from ForgeScriptBuilder tool
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForgeScriptBuilderResult {
    pub success: bool,
    pub script: String,
    pub transactions: Vec<TransactionData>,
    pub error: Option<String>,
}

/// Tool struct for building forge scripts
#[derive(Debug, Clone)]
pub struct ForgeScriptBuilder;

impl ForgeScriptBuilder {
    /// Build and execute a forge script from structured operations
    pub async fn execute(params: ForgeScriptBuilderParameters) -> Result<ForgeScriptBuilderResult> {
        // 1. Setup BAML client configuration
        let baml_config = Configuration {
            base_path: std::env::var("BAML_API_URL")
                .unwrap_or_else(|_| "http://localhost:2024".to_string()),
            ..Configuration::default()
        };

        // 2. Prepare deployed addresses mapping (for reference resolution)
        let deployed_addresses: HashMap<String, String> = HashMap::new();

        // 3. Sanitize operations before generating transaction calls
        let sanitized_operations = Self::sanitize_operations(&params.operations);

        let mut assembly_config = AssemblyConfig::default();
        if let Some(funding) = params.funding_requirements.as_ref() {
            let sanitized_funding = Self::sanitize_funding_requirements(funding);
            if !sanitized_funding.is_empty() {
                assembly_config.funding_requirements = sanitized_funding;
            }
        }

        // 4. Call BAML to generate transaction calls
        let generate_request = GenerateTransactionCallsRequest {
            operations: sanitized_operations,
            available_interfaces: params.available_interfaces,
            deployed_addresses,
            __baml_options__: None,
        };

        let generated = default_api::generate_transaction_calls(&baml_config, generate_request)
            .await
            .map_err(|e| anyhow::anyhow!("BAML generation failed: {}", e))?;

        // 5. Assemble the complete script
        let contract_definitions = vec![]; // Agent provides source code directly
        let script = ScriptAssembler::assemble(contract_definitions, generated, assembly_config)?;

        let rpc_url = std::env::var("AOMI_FORK_RPC")
            .unwrap_or_else(|_| "https://rpc.ankr.com/eth/2a9a32528f8a70a5b48c57e8fb83b4978f2a25c8368aa6fd9dc2f2321ae53362".to_string());

        // 6. Execute the script and extract broadcastable transactions
        match Self::execute_script(&script, Some(rpc_url), None).await {
            Ok(transactions) => Ok(ForgeScriptBuilderResult {
                success: true,
                script: script.clone(),
                transactions,
                error: None,
            }),
            Err(e) => Ok(ForgeScriptBuilderResult {
                success: false,
                script: script.clone(),
                transactions: vec![],
                error: Some(e.to_string()),
            }),
        }
    }

    fn sanitize_operations(operations: &[Operation]) -> Vec<Operation> {
        operations
            .iter()
            .map(|op| {
                let mut sanitized = op.clone();
                if let Some(checksummed) = Self::checksum_literal(&sanitized.contract_address) {
                    sanitized.contract_address = checksummed;
                }
                sanitized.parameters = op.parameters.iter().map(Self::sanitize_parameter).collect();
                sanitized
            })
            .collect()
    }

    fn sanitize_funding_requirements(
        requirements: &[FundingRequirement],
    ) -> Vec<FundingRequirement> {
        requirements
            .iter()
            .map(|req| match req {
                FundingRequirement::Eth { amount } => FundingRequirement::Eth {
                    amount: amount.clone(),
                },
                FundingRequirement::Erc20 {
                    token_address,
                    amount,
                    decimals,
                } => {
                    let sanitized_address = Self::checksum_literal(token_address)
                        .unwrap_or_else(|| token_address.clone());
                    FundingRequirement::Erc20 {
                        token_address: sanitized_address,
                        amount: amount.clone(),
                        decimals: *decimals,
                    }
                }
            })
            .collect()
    }

    fn sanitize_parameter(param: &Parameter) -> Parameter {
        let mut sanitized = param.clone();
        if Self::is_address_type(&sanitized.param_type) {
            sanitized.value = Self::sanitize_address_value(&sanitized.param_type, &sanitized.value);
        }
        sanitized
    }

    fn sanitize_address_value(param_type: &str, value: &str) -> String {
        if Self::is_address_array(param_type) {
            Self::sanitize_address_array(value).unwrap_or_else(|| value.to_string())
        } else {
            Self::checksum_literal_preserve_wrapping(value).unwrap_or_else(|| value.to_string())
        }
    }

    fn sanitize_address_array(value: &str) -> Option<String> {
        if let Ok(mut arr) = serde_json::from_str::<Vec<String>>(value) {
            for item in &mut arr {
                if let Some(cs) = Self::checksum_literal(item) {
                    *item = cs;
                }
            }
            return serde_json::to_string(&arr).ok();
        }

        let trimmed = value.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
            let inner = &trimmed[1..trimmed.len() - 1];
            let mut tokens: Vec<String> = Vec::new();
            for part in inner.split(',') {
                let raw = part.trim().trim_matches('"').trim_matches('\'');
                if raw.is_empty() {
                    continue;
                }
                let sanitized = if let Some(cs) = Self::checksum_literal(raw) {
                    cs
                } else {
                    raw.to_string()
                };
                tokens.push(format!("\"{}\"", sanitized));
            }
            if !tokens.is_empty() {
                return Some(format!("[{}]", tokens.join(",")));
            }
        }

        None
    }

    fn checksum_literal_preserve_wrapping(value: &str) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 1 {
            let inner = &trimmed[1..trimmed.len() - 1];
            Self::checksum_literal(inner).map(|cs| format!("\"{}\"", cs))
        } else {
            Self::checksum_literal(trimmed)
        }
    }

    fn checksum_literal(value: &str) -> Option<String> {
        if value.len() == 42 && (value.starts_with("0x") || value.starts_with("0X")) {
            Address::from_str(value)
                .ok()
                .map(|addr| addr.to_checksum(None))
        } else {
            None
        }
    }

    fn is_address_type(param_type: &str) -> bool {
        param_type.trim().to_lowercase().starts_with("address")
    }

    fn is_address_array(param_type: &str) -> bool {
        param_type.contains('[')
    }

    /// Execute a forge script and return broadcastable transactions
    async fn execute_script(
        script: &str,
        fork_url: Option<String>,
        fork_block_number: Option<u64>,
    ) -> Result<Vec<TransactionData>> {
        // 1. Setup contract config - following the pattern from execute_forge_script in tests
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let contract_root = manifest_dir.join("src/contract");

        let mut base_config = FoundryConfig::with_root(&contract_root);
        base_config.libs.push(contract_root.join("lib"));

        let contract_config = ContractConfig {
            foundry_config: std::sync::Arc::new(base_config),
            evm_opts: EvmOpts {
                fork_url,
                fork_block_number,
                memory_limit: 128 * 1024 * 1024,
                ..Default::default()
            },
            initial_balance: Some(U256::from(10u64.pow(18))),
            ..Default::default()
        };

        // 2. Create contract session
        let mut session = ContractSession::new(contract_config).await?;

        // 3. Compile the script
        let script_path = PathBuf::from("forge_script.sol");
        session.compile_source("forge_script".to_string(), script_path, script.to_string())?;

        // 4. Get the deployed bytecode for the forge_script contract
        let compilation = session
            .get_compilation("forge_script")
            .ok_or_else(|| anyhow::anyhow!("Compilation not found"))?;

        let bytecode_vec = session
            .compiler
            .get_contract_bytecode(compilation, "forge_script")?;
        let bytecode = Bytes::from(bytecode_vec);

        // 5. Deploy the script contract
        let runner = session.get_runner().await?;
        let (script_address, _) = runner.deploy(bytecode)?;

        // 6. Call the run() function
        let run_selector = Bytes::from(keccak256("run()".as_bytes())[0..4].to_vec());
        let execution_result = runner.call(script_address, run_selector, U256::ZERO)?;

        if !execution_result.success {
            let revert_hex = if execution_result.returned.is_empty() {
                "0x".to_string()
            } else {
                format!(
                    "0x{}",
                    alloy_primitives::hex::encode(&execution_result.returned)
                )
            };
            anyhow::bail!("Script execution failed: {}", revert_hex);
        }

        // 7. Convert broadcastable transactions to serializable format
        let transactions = execution_result
            .broadcastable_transactions
            .iter()
            .map(|btx| TransactionData {
                from: btx.transaction.from().map(|addr| format!("{:?}", addr)),
                to: btx.transaction.to().and_then(|kind| match kind {
                    alloy_primitives::TxKind::Call(addr) => Some(format!("{:?}", addr)),
                    alloy_primitives::TxKind::Create => None,
                }),
                value: btx.transaction.value().unwrap_or(U256::ZERO).to_string(),
                data: alloy_primitives::hex::encode(
                    btx.transaction.input().unwrap_or(&Default::default()),
                ),
                rpc_url: btx.rpc.clone(),
            })
            .collect();

        Ok(transactions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_client::models::TransactionCall;

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
    async fn test_execute_simple_script() {
        // Test that execute_script can compile and run a simple script
        let simple_script = r#"
pragma solidity ^0.8.20;

import {Script} from "forge-std/Script.sol";
import {StdCheats} from "forge-std/StdCheats.sol";

contract forge_script is Script, StdCheats {

    function run() public {
        deal(msg.sender, 10 ether);
        vm.startBroadcast();

        // Simple operation: send ETH to an address
        payable(address(0x1234567890123456789012345678901234567890)).transfer(1 ether);

        vm.stopBroadcast();
    }
}
        "#;

        let result = ForgeScriptBuilder::execute_script(simple_script, None, None).await;

        // We expect this to succeed and return transactions
        if let Err(e) = &result {
            eprintln!("Error: {:?}", e);
        }
        assert!(result.is_ok(), "Script execution should succeed");
        let transactions = result.unwrap();
        assert_eq!(transactions.len(), 1, "Should have exactly one transaction");

        // Verify transaction structure
        let tx = &transactions[0];
        assert!(tx.from.is_some(), "Transaction should have a from address");
        assert!(tx.to.is_some(), "Transaction should have a to address");
        assert_ne!(tx.value, "0", "Transaction should have a non-zero value");
    }
}
