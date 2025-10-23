use super::etherscan::EtherscanResults;
use super::handlers::config::HandlerDefinition;
use anyhow::Result;
use baml_client::models::{ContractAnalysis, ContractInfo};
use serde_json::json;

/// Convert BAML ContractAnalysis result to HandlerDefinitions
/// Handles polymorphic handler configs from the unified analysis function
pub fn to_handler_definition(result: ContractAnalysis) -> Result<Vec<(String, HandlerDefinition)>> {
    use baml_client::models::FieldHandlerConfig;

    let mut handlers = Vec::new();

    for field_handler in result.handlers {
        let field_name = field_handler.field_name;

        let handler_def = match field_handler.config {
            FieldHandlerConfig::CallHandlerConfig(config) => {
                // Convert args from Vec<String> to Vec<serde_json::Value>
                let args = config
                    .args
                    .map(|args_vec| args_vec.into_iter().map(|s| json!(s)).collect());

                HandlerDefinition::Call {
                    method: config.method,
                    args,
                    ignore_relative: config.ignore_relative,
                    expect_revert: config.expect_revert,
                    address: config.address,
                }
            }

            FieldHandlerConfig::StorageHandlerConfig(config) => HandlerDefinition::Storage {
                slot: Some(json!(config.slot)),
                offset: config.offset.map(|o| o as u64),
                return_type: config.return_type,
                ignore_relative: config.ignore_relative,
            },

            FieldHandlerConfig::EventHandlerConfig(config) => {
                use super::handlers::config::EventOperation;

                // Convert EventOperation from BAML to our type
                let add = Some(EventOperation {
                    event: config.add.event.clone(),
                    where_clause: config.add.r#where.map(|w| json!(w)),
                });

                let remove = config.remove.map(|baml_op| EventOperation {
                    event: baml_op.event,
                    where_clause: baml_op.r#where.map(|w| json!(w)),
                });

                HandlerDefinition::Event {
                    event: Some(config.add.event), // Populate with the add event name
                    return_type: None,
                    select: Some(json!(config.selected)),
                    add,
                    remove,
                    ignore_relative: config.ignore_relative,
                }
            }

            FieldHandlerConfig::AccessControlHandlerConfig(config) => {
                use std::collections::HashMap;

                // Convert role_names from BAML map to HashMap
                let role_names = config
                    .role_names
                    .map(|map| map.into_iter().collect::<HashMap<String, String>>());

                HandlerDefinition::AccessControl {
                    role_names,
                    pick_role_members: config.pick_role_members,
                    ignore_relative: config.ignore_relative,
                    extra: None,
                }
            }

            FieldHandlerConfig::DynamicArrayHandlerConfig(config) => {
                HandlerDefinition::DynamicArray {
                    slot: Some(json!(config.slot)),
                    return_type: config.return_type,
                    ignore_relative: config.ignore_relative,
                }
            }
        };

        handlers.push((field_name, handler_def));
    }

    Ok(handlers)
}

/// Extract value type from mapping signature
/// "mapping(address => uint256)" -> "uint256"
/// "mapping(address => mapping(address => uint256))" -> "uint256"
fn extract_mapping_value_type(mapping_type: &str) -> Result<String, String> {
    // Find the last occurrence of " => " to get the final value type
    if let Some(arrow_pos) = mapping_type.rfind(" => ") {
        let value_start = arrow_pos + " => ".len();
        let value_part = &mapping_type[value_start..];

        // Remove all trailing ")" characters
        let value_trimmed = value_part.trim_end_matches(')').trim();

        return Ok(value_trimmed.to_string());
    }
    Err(format!("Invalid mapping type: {}", mapping_type))
}

/// Extract element type from array signature
/// "address[]" -> "address"
/// "uint256[10]" -> "uint256"
fn extract_array_element_type(array_type: &str) -> Result<String, String> {
    if let Some(bracket_pos) = array_type.find('[') {
        Ok(array_type[..bracket_pos].trim().to_string())
    } else {
        Err(format!("Invalid array type: {}", array_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_mapping_value_type() {
        assert_eq!(
            extract_mapping_value_type("mapping(address => uint256)").unwrap(),
            "uint256"
        );
        assert_eq!(
            extract_mapping_value_type("mapping(address => mapping(address => uint256))").unwrap(),
            "uint256"
        );
    }

    #[test]
    fn test_extract_array_element_type() {
        assert_eq!(extract_array_element_type("address[]").unwrap(), "address");
        assert_eq!(
            extract_array_element_type("uint256[10]").unwrap(),
            "uint256"
        );
    }
}

/// Convert EtherscanResults to ContractInfo for BAML processing
pub fn etherscan_to_contract_info(
    results: EtherscanResults,
    description: Option<String>,
) -> Result<ContractInfo, String> {
    // Extract ABI as string
    let abi = results.abi.map(|v| match v {
        serde_json::Value::String(s) => s,
        serde_json::Value::Array(_) => v.to_string(),
        _ => v.to_string(),
    });

    // Extract source code from Etherscan response
    // Etherscan returns an array with contract details
    let source_code = results.source_code.and_then(|v| {
        if let Some(arr) = v.as_array() {
            if let Some(first) = arr.first() {
                if let Some(source) = first.get("SourceCode") {
                    return source.as_str().map(|s| s.to_string());
                }
            }
        }
        None
    });

    Ok(ContractInfo {
        description,
        address: Some(results.address),
        abi,
        source_code,
    })
}

#[cfg(test)]
mod etherscan_conversion_tests {
    use super::*;

    #[test]
    fn test_etherscan_to_contract_info_with_abi_string() {
        let results = EtherscanResults {
            address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            abi: Some(json!(r#"[{"type":"function","name":"totalSupply"}]"#)),
            source_code: None,
        };

        let contract_info =
            etherscan_to_contract_info(results, Some("USDC Proxy".to_string())).unwrap();

        assert_eq!(
            contract_info.address,
            Some("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string())
        );
        assert_eq!(contract_info.description, Some("USDC Proxy".to_string()));
        assert!(contract_info.abi.is_some());
        assert!(contract_info.source_code.is_none());
    }

    #[test]
    fn test_etherscan_to_contract_info_with_source_code() {
        let source_response = json!([{
            "SourceCode": "contract MyContract { uint256 public value; }",
            "ABI": "[{\"type\":\"function\"}]",
            "ContractName": "MyContract",
            "CompilerVersion": "v0.8.0+commit.c7dfd78e"
        }]);

        let results = EtherscanResults {
            address: "0x123".to_string(),
            abi: Some(json!("[{\"type\":\"function\"}]")),
            source_code: Some(source_response),
        };

        let contract_info = etherscan_to_contract_info(results, None).unwrap();

        assert_eq!(contract_info.address, Some("0x123".to_string()));
        assert!(contract_info.abi.is_some());
        assert!(contract_info.source_code.is_some());
        assert_eq!(
            contract_info.source_code.unwrap(),
            "contract MyContract { uint256 public value; }"
        );
    }

    #[test]
    fn test_etherscan_to_contract_info_empty_results() {
        let results = EtherscanResults {
            address: "0xabc".to_string(),
            abi: None,
            source_code: None,
        };

        let contract_info =
            etherscan_to_contract_info(results, Some("Empty contract".to_string())).unwrap();

        assert_eq!(contract_info.address, Some("0xabc".to_string()));
        assert_eq!(
            contract_info.description,
            Some("Empty contract".to_string())
        );
        assert!(contract_info.abi.is_none());
        assert!(contract_info.source_code.is_none());
    }
}
