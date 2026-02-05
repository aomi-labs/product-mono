//! Generalized ABI encoding tool for any contract function
use alloy::{
    dyn_abi::{DynSolType, DynSolValue},
    hex::ToHexExt,
    primitives::{Address, FixedBytes, I256, U256},
};
use eyre::{Context, Result};
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::str::FromStr;
use tracing::{debug, info, warn};

use crate::{AomiTool, AomiToolArgs, ToolCallCtx, with_topic};

/// Parameters for EncodeFunctionCall
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeFunctionCallParameters {
    /// The function signature, e.g., 'transfer(address,uint256)' or 'balanceOf(address)'
    pub function_signature: String,
    /// Array of argument values. For simple types pass strings, for array types pass arrays directly
    pub arguments: Vec<serde_json::Value>,
}

impl AomiToolArgs for EncodeFunctionCallParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "function_signature": {
                    "type": "string",
                    "description": "The function signature, e.g., 'transfer(address,uint256)' or 'balanceOf(address)'"
                },
                "arguments": {
                    "type": "array",
                    "description": "Array of argument values as strings. For simple types pass strings directly. For array types, pass as JSON-encoded strings, e.g., for swapExactETHForTokens(uint256,address[],address,uint256) pass: [\"0\", \"[\\\"0xC02...WETH\\\", \\\"0xA0b...USDC\\\"]\", \"0x7099...\", \"1716302400\"]",
                    "items": {"type": "string"}
                }
            },
            "required": ["function_signature", "arguments"]
        }))
    }
}

/// Tool for encoding function calls into hex calldata
#[derive(Debug, Clone)]
pub struct EncodeFunctionCall;

/// Parse a function signature like "transfer(address,uint256)" into name and param types
fn parse_function_signature(signature: &str) -> Result<(String, Vec<String>)> {
    debug!(signature = %signature, "Parsing function signature");
    // Find the opening parenthesis
    let paren_pos = signature
        .find('(')
        .ok_or_else(|| eyre::eyre!("Invalid function signature: missing '('"))?;

    // Extract function name
    let function_name = signature[..paren_pos].trim().to_string();

    // Extract parameters part (between parentheses)
    let params_end = signature
        .rfind(')')
        .ok_or_else(|| eyre::eyre!("Invalid function signature: missing ')'"))?;
    let params_str = &signature[paren_pos + 1..params_end];

    // Parse parameter types
    let param_types: Vec<String> = if params_str.trim().is_empty() {
        vec![]
    } else {
        params_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    debug!(
        function = %function_name,
        param_count = param_types.len(),
        "Parsed function signature successfully"
    );
    Ok((function_name, param_types))
}

/// Convert a parameter value string to a DynSolValue based on its type
fn parse_param_value(param_type: &str, value: &str) -> Result<DynSolValue> {
    debug!(param_type = %param_type, "Parsing ABI parameter value");
    match param_type {
        "address" => Ok(DynSolValue::Address(
            Address::from_str(value).wrap_err_with(|| format!("Invalid address: {value}"))?,
        )),
        "uint256" | "uint" => {
            let num = U256::from_str(value)
                .wrap_err_with(|| format!("Invalid uint256 value: {value}"))?;
            Ok(DynSolValue::Uint(num, 256))
        }
        "int256" | "int" => {
            let num =
                I256::from_str(value).wrap_err_with(|| format!("Invalid int256 value: {value}"))?;
            Ok(DynSolValue::Int(num, 256))
        }
        "bool" => {
            let b = value
                .parse::<bool>()
                .wrap_err_with(|| format!("Invalid bool value: {value}"))?;
            Ok(DynSolValue::Bool(b))
        }
        "string" => Ok(DynSolValue::String(value.to_string())),
        "bytes" => {
            let bytes = if let Some(stripped) = value.strip_prefix("0x") {
                hex::decode(stripped).wrap_err("Invalid hex bytes")?
            } else {
                hex::decode(value).wrap_err("Invalid hex bytes")?
            };
            Ok(DynSolValue::Bytes(bytes))
        }
        "bytes32" => {
            let bytes = if let Some(stripped) = value.strip_prefix("0x") {
                hex::decode(stripped).wrap_err("Invalid hex bytes32")?
            } else {
                hex::decode(value).wrap_err("Invalid hex bytes32")?
            };
            if bytes.len() != 32 {
                return Err(eyre::eyre!("bytes32 must be exactly 32 bytes"));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(DynSolValue::FixedBytes(FixedBytes::from(arr), 32))
        }
        // Handle arrays like address[], uint256[], etc. - check this first before type-specific checks
        s if s.ends_with("[]") => {
            let inner_type = &s[..s.len() - 2];
            // Parse JSON array
            let values: Vec<String> = serde_json::from_str(value).wrap_err("Invalid array JSON")?;
            let parsed_values: Result<Vec<DynSolValue>> = values
                .iter()
                .map(|v| parse_param_value(inner_type, v))
                .collect();
            Ok(DynSolValue::Array(parsed_values?))
        }
        // Handle uint8, uint16, etc.
        s if s.starts_with("uint") => {
            let bits = s[4..]
                .parse::<usize>()
                .map_err(|_| eyre::eyre!("Invalid uint type: {s}"))?;
            if bits % 8 != 0 || bits == 0 || bits > 256 {
                return Err(eyre::eyre!("Invalid uint size: {bits}"));
            }
            let num =
                U256::from_str(value).wrap_err_with(|| format!("Invalid {s} value: {value}"))?;
            Ok(DynSolValue::Uint(num, bits))
        }
        // Handle int8, int16, etc.
        s if s.starts_with("int") => {
            let bits = s[3..]
                .parse::<usize>()
                .map_err(|_| eyre::eyre!("Invalid int type: {s}"))?;
            if bits % 8 != 0 || bits == 0 || bits > 256 {
                return Err(eyre::eyre!("Invalid int size: {bits}"));
            }
            let num =
                I256::from_str(value).wrap_err_with(|| format!("Invalid {s} value: {value}"))?;
            Ok(DynSolValue::Int(num, bits))
        }
        _ => Err(eyre::eyre!("Unsupported parameter type: {param_type}")),
    }
}

pub async fn execute_call(args: EncodeFunctionCallParameters) -> Result<String, ToolError> {
    info!(
        signature = %args.function_signature,
        arg_count = args.arguments.len(),
        "Encoding function call request"
    );

    // Parse the function signature
    let (function_name, param_types) = match parse_function_signature(&args.function_signature) {
        Ok(result) => result,
        Err(e) => {
            warn!(
                error = %e,
                signature = %args.function_signature,
                "Failed to parse function signature"
            );
            return Err(ToolError::ToolCallError(e.to_string().into()));
        }
    };

    // Check argument count matches
    if args.arguments.len() != param_types.len() {
        warn!(
            expected = param_types.len(),
            got = args.arguments.len(),
            "Argument count mismatch for ABI encoding"
        );
        return Err(ToolError::ToolCallError(
            format!(
                "Argument count mismatch: expected {} arguments, got {}",
                param_types.len(),
                args.arguments.len()
            )
            .into(),
        ));
    }

    // Parse the parameter values
    let mut values = Vec::new();
    for (i, (param_type, arg_value)) in param_types.iter().zip(args.arguments.iter()).enumerate() {
        debug!(index = i, param_type = %param_type, "Parsing function argument");
        // Convert serde_json::Value to string for parsing
        let arg_str = match arg_value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(_) => {
                // For arrays, convert to JSON string
                serde_json::to_string(arg_value).map_err(|e| {
                    ToolError::ToolCallError(
                        format!("Error serializing array argument {i}: {e}").into(),
                    )
                })?
            }
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => {
                return Err(ToolError::ToolCallError(
                    format!("Unsupported argument type at position {i}: {arg_value:?}").into(),
                ));
            }
        };

        match parse_param_value(param_type, &arg_str) {
            Ok(value) => {
                debug!(index = i, "Argument parsed successfully");
                values.push(value);
            }
            Err(e) => {
                warn!(
                    index = i,
                    param_type = %param_type,
                    error = %e,
                    "Failed to parse ABI argument"
                );
                return Err(ToolError::ToolCallError(
                    format!("Error parsing argument {i} ({param_type}): {e}").into(),
                ));
            }
        }
    }

    // Create function selector (first 4 bytes of keccak256 hash)
    let signature_string = format!("{}({})", function_name, param_types.join(","));
    debug!(
        canonical_signature = %signature_string,
        "Computed canonical function signature"
    );
    let selector = alloy::primitives::keccak256(signature_string.as_bytes());
    let selector_bytes = &selector[..4];
    let selector_hex = format!("0x{}", selector_bytes.encode_hex());
    debug!(selector = %selector_hex, "Generated function selector");

    // Encode the arguments
    let encoded_args = if values.is_empty() {
        vec![]
    } else {
        // Create DynSolType array for encoding
        let types: Result<Vec<DynSolType>, _> =
            param_types.iter().map(|t| DynSolType::parse(t)).collect();

        let _types = types
            .map_err(|e| ToolError::ToolCallError(format!("Error parsing types: {e:#?}").into()))?;

        // Encode all values together
        DynSolValue::Tuple(values).abi_encode_params().to_vec()
    };
    debug!(
        encoded_arg_bytes = encoded_args.len(),
        "ABI arguments encoded"
    );

    // Combine selector and encoded arguments
    let mut calldata = selector_bytes.to_vec();
    calldata.extend_from_slice(&encoded_args);

    let encoded_hex = format!("0x{}", calldata.encode_hex());
    info!(
        total_bytes = encoded_hex.len(),
        "Function call encoded successfully"
    );
    Ok(encoded_hex)
}

impl AomiTool for EncodeFunctionCall {
    const NAME: &'static str = "encode_function_call";

    type Args = EncodeFunctionCallParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Encodes a function call into hex calldata for any contract function."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_call(args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_encode_transfer() {
        let args = EncodeFunctionCallParameters {
            function_signature: "transfer(address,uint256)".to_string(),
            arguments: vec![
                serde_json::Value::String("0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string()),
                serde_json::Value::String("1000000000000000000".to_string()),
            ],
        };

        let result = execute_call(args).await.unwrap();

        // Function selector for transfer(address,uint256) is 0xa9059cbb
        assert!(result.starts_with("0xa9059cbb"));
        // Full length should be 10 (0x prefix + 8 selector) + 128 (2 * 32 bytes for params)
        assert_eq!(result.len(), 138);
    }

    #[tokio::test]
    async fn test_encode_balance_of() {
        let args = EncodeFunctionCallParameters {
            function_signature: "balanceOf(address)".to_string(),
            arguments: vec![serde_json::Value::String(
                "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            )],
        };

        let result = execute_call(args).await.unwrap();

        // Function selector for balanceOf(address) is 0x70a08231
        assert!(result.starts_with("0x70a08231"));
        assert_eq!(result.len(), 74); // 10 + 64
    }

    #[tokio::test]
    async fn test_encode_no_params() {
        let args = EncodeFunctionCallParameters {
            function_signature: "totalSupply()".to_string(),
            arguments: vec![],
        };

        let result = execute_call(args).await.unwrap();

        // Function selector for totalSupply() is 0x18160ddd
        assert!(result.starts_with("0x18160ddd"));
        assert_eq!(result.len(), 10); // Just the selector
    }

    #[tokio::test]
    async fn test_encode_with_array() {
        let args = EncodeFunctionCallParameters {
            function_signature: "batchTransfer(address[],uint256[])".to_string(),
            arguments: vec![
                serde_json::json!([
                    "0x742d35Cc6634C0532925a3b844Bc9e7595f33749",
                    "0x8626f6940E2eb28930eFb4CeF49B2d1F2C9C1199"
                ]),
                serde_json::json!(["1000000000000000000", "2000000000000000000"]),
            ],
        };

        match execute_call(args).await {
            Ok(encoded) => {
                println!("Encoded: {}", encoded);
                assert!(encoded.starts_with("0x"));
            }
            Err(e) => panic!("Failed to encode: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_encode_swap_exact_eth_for_tokens() {
        // Test the exact scenario from the error message
        let args = EncodeFunctionCallParameters {
            function_signature: "swapExactETHForTokens(uint256,address[],address,uint256)"
                .to_string(),
            arguments: vec![
                serde_json::Value::String("0".to_string()),
                serde_json::json!([
                    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083c756Cc2",
                    "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
                ]),
                serde_json::Value::String("0x70997970C51812dc3A010C7d01b50e0d17dc79C8".to_string()),
                serde_json::Value::String("1716302400".to_string()),
            ],
        };

        match execute_call(args).await {
            Ok(encoded) => {
                println!("Encoded swapExactETHForTokens: {}", encoded);
                // Function selector for swapExactETHForTokens(uint256,address[],address,uint256)
                assert!(encoded.starts_with("0x"));
                // Should have selector (4 bytes) + 4 * 32 bytes for offset pointers + array data
                assert!(encoded.len() > 10);
            }
            Err(e) => panic!("Failed to encode swapExactETHForTokens: {:?}", e),
        }
    }
}
