use crate::baml::{CodeLine, Import, Interface, ScriptBlock};
use alloy::serde;
// use crate::forge_script_builder::{AssemblyConfig, FundingRequirement};
use anyhow::{anyhow, Result};
use alloy_primitives::Address;
use ::serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

// Newline constants
const NL: &str = "\n";
const NL2: &str = "\n\n";

// Script template constants
const SCRIPT_IMPORT: &str = "import {Script} from \"forge-std/Script.sol\";";
const STD_CHEATS_IMPORT: &str = "import {StdCheats} from \"forge-std/StdCheats.sol\";";

const CONTRACT_HEADER: &str = "contract AomiScript is Script, StdCheats {";
const RUN_FUNCTION_HEADER: &str = "    function run() public {";
const VM_START_BROADCAST: &str = "        vm.startBroadcast();";
const VM_STOP_BROADCAST: &str = "        vm.stopBroadcast();";
const FUNCTION_FOOTER: &str = "    }";
const CONTRACT_FOOTER: &str = "}";

// Indentation constants
const INDENT_L1: &str = "        "; // 8 spaces - inside run() function
/// Script assembler - wraps transaction calls in executable Forge script


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

pub struct ScriptAssembler;

impl ScriptAssembler {
    pub fn assemble(
        contract_definitions: Vec<String>,
        block: &ScriptBlock,
        config: AssemblyConfig,
    ) -> Result<String> {
        let mut script = String::new();

        Self::add_pragma(&mut script, &config);
        Self::add_imports(&mut script, &block.codelines);
        Self::add_inline_interfaces(&mut script, &block.codelines);
        Self::add_additional_contracts(&mut script, contract_definitions);
        Self::add_forge_script_wrapper(&mut script, block, &config)?;

        Ok(script)
    }

    fn add_pragma(script: &mut String, config: &AssemblyConfig) {
        script.push_str(&format!("pragma solidity {};", config.solidity_version));
        script.push_str(NL2);
    }

    fn add_imports(script: &mut String, codelines: &[CodeLine]) {
        script.push_str(SCRIPT_IMPORT);
        script.push_str(NL);
        script.push_str(STD_CHEATS_IMPORT);
        script.push_str(NL);

        let mut imports: HashMap<(String, String), Import> = HashMap::new();
        for line in codelines {
            if let Some(import_spec) = &line.import_spec {
                imports
                    .entry((
                        import_spec.interface_name.clone(),
                        import_spec.source.clone(),
                    ))
                    .or_insert_with(|| import_spec.clone());
            }
        }

        for (_, import_spec) in imports {
            script.push_str(&format!(
                "import {{{}}} from \"{}\";",
                import_spec.interface_name, import_spec.source
            ));
            script.push_str(NL);
        }
        script.push_str(NL);
    }

    fn add_inline_interfaces(script: &mut String, codelines: &[CodeLine]) {
        let mut interfaces: HashMap<String, Interface> = HashMap::new();
        for line in codelines {
            if let Some(interface) = &line.interface {
                interfaces
                    .entry(interface.name.clone())
                    .or_insert_with(|| interface.clone());
            }
        }

        for interface in interfaces.values() {
            script.push_str(&interface.solidity_code);
            script.push_str(NL2);
        }
    }

    fn add_additional_contracts(script: &mut String, contract_definitions: Vec<String>) {
        for contract_def in contract_definitions {
            script.push_str(&contract_def);
            script.push_str(NL2);
        }
    }

    fn add_forge_script_wrapper(
        script: &mut String,
        block: &ScriptBlock,
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
        Self::add_transaction_calls(script, &block.codelines);

        // Close broadcast and function
        script.push_str(VM_STOP_BROADCAST);
        script.push_str(NL);
        script.push_str(FUNCTION_FOOTER);
        script.push_str(NL);
        script.push_str(CONTRACT_FOOTER);
        script.push_str(NL);

        Ok(())
    }

    fn add_transaction_calls(script: &mut String, calls: &[CodeLine]) {
        for code_line in calls {
            for line in code_line.line.lines() {
                let sanitized = Self::checksum_addresses_in_line(line);
                script.push_str(INDENT_L1);
                script.push_str(&sanitized);
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
            alloy_primitives::U256::ZERO
        } else {
            alloy_primitives::U256::from_str_radix(&clean_int, 10)
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
                let frac_value = alloy_primitives::U256::from_str_radix(&frac_digits, 10)
                    .map_err(|_| anyhow!("failed to parse fractional part"))?;
                total = total
                    .checked_add(frac_value)
                    .ok_or_else(|| anyhow!("amount overflow"))?;
            }
        }

        Ok(total.to_string())
    }

    fn pow10(decimals: u8) -> alloy_primitives::U256 {
        let mut result = alloy_primitives::U256::from(1u8);
        let ten = alloy_primitives::U256::from(10u8);
        for _ in 0..decimals {
            result *= ten;
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

    /// Replace any address literals with their EIP-55 checksum form to avoid
    /// Solidity checksum errors when the LLM emits lowercased addresses.
    fn checksum_addresses_in_line(line: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut idx = 0;

        while let Some(rel_pos) = line[idx..].find("0x") {
            let pos = idx + rel_pos;
            out.push_str(&line[idx..pos]);

            if line.len() >= pos + 42 {
                let candidate = &line[pos..pos + 42];
                if Self::looks_like_address(candidate) {
                    if let Some(cs) = Self::checksum_literal(candidate) {
                        out.push_str(&cs);
                        idx = pos + 42;
                        continue;
                    }
                }
            }

            // Not an address literal, keep the original "0x" and continue.
            out.push_str("0x");
            idx = pos + 2;
        }

        out.push_str(&line[idx..]);
        out
    }

    fn looks_like_address(value: &str) -> bool {
        value.len() == 42
            && (value.starts_with("0x") || value.starts_with("0X"))
            && value[2..].chars().all(|c| c.is_ascii_hexdigit())
    }

    fn checksum_literal(value: &str) -> Option<String> {
        Address::from_str(value)
            .ok()
            .map(|addr| addr.to_checksum(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baml::{CodeLine, Import, Interface, ScriptBlock};

    #[test]
    fn assembles_with_imports_and_interfaces() {
        let block = ScriptBlock {
            codelines: vec![
                CodeLine {
                    line: "IERC20 token = IERC20(0x123);".to_string(),
                    import_spec: Some(Import {
                        interface_name: "IERC20".into(),
                        source: "forge-std/interfaces/IERC20.sol".into(),
                    }),
                    interface: None,
                },
                CodeLine {
                    line: "IWETH(0x456).wrap{value: 1 ether}();".to_string(),
                    import_spec: None,
                    interface: Some(Interface {
                        name: "IWETH".into(),
                        solidity_code: "interface IWETH { function wrap() external payable; }"
                            .into(),
                    }),
                },
            ],
        };

        let script = ScriptAssembler::assemble(
            vec![],
            &block,
            AssemblyConfig::default(),
        )
        .expect("assemble");

        println!("script: {}", script);

        assert!(script.contains("import {IERC20} from \"forge-std/interfaces/IERC20.sol\";"));
        assert!(script.contains("interface IWETH"));
        assert!(script.contains("IERC20 token = IERC20(0x123);"));
        assert!(script.contains("IWETH(0x456).wrap"));
        assert!(script.contains("vm.startBroadcast"));
        assert!(script.contains("vm.stopBroadcast"));
    }
}
