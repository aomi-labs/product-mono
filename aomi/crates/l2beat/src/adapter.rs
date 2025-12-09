use super::handlers::config::{EventOperation as HandlerEventOperation, HandlerDefinition};
use anyhow::Result;
use aomi_tools::db::Contract;
use baml_client::models::{
    AbiAnalysisResult, ContractInfo, EventActionHandler, EventAnalyzeResult,
    EventOperation as BamlEventOperation, LayoutAnalysisResult,
};
use serde_json::json;

/// Convert BAML ABI analysis result to HandlerDefinitions
/// Creates Call handler definitions for all callable view/pure functions
pub fn abi_analysis_to_call_handlers(
    result: AbiAnalysisResult,
) -> Vec<(String, HandlerDefinition)> {
    let mut handlers = Vec::new();

    for retrieval in result.retrievals {
        let args = if retrieval.requires_parameters {
            retrieval.parameter_types.map(|types| {
                types
                    .iter()
                    .enumerate()
                    .map(|(i, _param_type)| json!(format!("{{{{ param{} }}}}", i)))
                    .collect()
            })
        } else {
            None
        };

        let handler_def = HandlerDefinition::Call {
            method: retrieval.function_signature,
            args,
            expect_revert: None,
            address: None,
            ignore_relative: Some(false),
        };

        handlers.push((retrieval.name, handler_def));
    }

    handlers
}

/// Convert layout analysis output into storage/dynamic array handler definitions.
pub fn layout_analysis_to_storage_handlers(
    result: LayoutAnalysisResult,
) -> Result<Vec<(String, HandlerDefinition)>, String> {
    let mut handlers = Vec::new();

    for slot in result.slots {
        let base_slot = slot
            .base_slot
            .clone()
            .ok_or_else(|| format!("Missing base_slot for storage variable '{}'", slot.name))?;

        let solidity_type = slot.r#type.trim().to_string();

        let handler_def = if is_mapping_type(&solidity_type) {
            create_mapping_handler(&base_slot, &solidity_type)?
        } else if is_array_type(&solidity_type) {
            create_array_handler(&base_slot, &solidity_type)?
        } else {
            HandlerDefinition::Storage {
                slot: Some(json!(base_slot)),
                offset: slot.offset.map(|o| o as u64),
                return_type: Some(solidity_type.clone()),
                ignore_relative: Some(false),
            }
        };

        handlers.push((slot.name.clone(), handler_def));
    }

    Ok(handlers)
}

// #[rig_tool(
//     description: "this prints handler definition"
// )]
// fn abi_to_call_agent_tool() {
//     let req  = baml_client::models::AnalyzeAbiRequest {

//     }
//     let res: AbiAnalysisResult = baml_client.call(ABIRequlte).await;
//     let handler = abi_analysis_to_call_handlers(res);
//     static HANDLER: HashMap<HandlerDefinition> = HashMap::new();
//     HANDLER.push(handler); // name -> def
//     return handler.to_json() // AI: "i identify the following field: ["validatorsForChain325", "Admin", "TotalAmount"] related to this contract"
// }

// #[rig_tool(
//     description: "this execute the handler"
// )]
// fn execute_handlers(names: Vec<String>) { // these are the ones user ask for

//     // AI suppose to make up the prams from history, it basically has intrinsic state
//     // Risk: might be wrong
//     let handler_defs: Vec<HandlerDefinition> = vec![]; // get this from somewhere
//     for def in handler_defs {

//     }
// }

/// Convert event analysis output into event/access-control handler definitions.
pub fn event_analysis_to_event_handlers(
    result: EventAnalyzeResult,
) -> Vec<(String, HandlerDefinition)> {
    result
        .event_actions
        .into_iter()
        .map(|action| {
            let handler = match action.handler {
                EventActionHandler::EventHandlerConfig(config) => HandlerDefinition::Event {
                    event: config.event_signature.clone(),
                    return_type: Some(config.return_type.clone()),
                    select: Some(json!(config.select_field.clone())),
                    add: config.add.map(convert_event_operation),
                    remove: config.remove.map(convert_event_operation),
                    set: None,
                    group_by: None,
                    ignore_relative: Some(false),
                },
                EventActionHandler::AccessControlConfig(config) => {
                    HandlerDefinition::AccessControl {
                        role_names: config.role_names.clone(),
                        pick_role_members: config.pick_role_members.clone(),
                        ignore_relative: Some(false),
                        extra: None,
                    }
                }
            };

            (action.field_name, handler)
        })
        .collect()
}

fn is_mapping_type(solidity_type: &str) -> bool {
    solidity_type.trim_start().starts_with("mapping(")
}

fn is_array_type(solidity_type: &str) -> bool {
    solidity_type.contains('[') && solidity_type.ends_with(']')
}

fn create_mapping_handler(
    base_slot: &str,
    solidity_type: &str,
) -> Result<HandlerDefinition, String> {
    let depth = solidity_type.matches("mapping").count();
    if depth == 0 {
        return Err(format!("Invalid mapping type: {}", solidity_type));
    }

    let mut slot_components: Vec<serde_json::Value> = Vec::with_capacity(depth + 1);
    slot_components.push(json!(base_slot));

    for idx in 0..depth {
        let placeholder = if depth == 1 {
            "{{ key }}".to_string()
        } else {
            format!("{{{{ key{} }}}}", idx)
        };
        slot_components.push(json!(placeholder));
    }

    let value_type = extract_mapping_value_type(solidity_type)?;

    Ok(HandlerDefinition::Storage {
        slot: Some(json!(slot_components)),
        offset: None,
        return_type: Some(value_type),
        ignore_relative: Some(false),
    })
}

fn create_array_handler(base_slot: &str, solidity_type: &str) -> Result<HandlerDefinition, String> {
    let element_type = extract_array_element_type(solidity_type)?;

    Ok(HandlerDefinition::DynamicArray {
        slot: Some(json!(base_slot)),
        return_type: Some(element_type),
        ignore_relative: Some(false),
    })
}

fn convert_event_operation(operation: BamlEventOperation) -> HandlerEventOperation {
    HandlerEventOperation {
        event: vec![operation.event_signature],
        where_clause: operation.r#where.map(|clause| json!(clause)),
    }
    .sanitize()
}

/// Extract value type from mapping signature
/// "mapping(address => uint256)" -> "uint256"
/// "mapping(address => mapping(address => uint256))" -> "uint256"
fn extract_mapping_value_type(mapping_type: &str) -> Result<String, String> {
    if let Some(arrow_pos) = mapping_type.rfind("=>") {
        let value_part = &mapping_type[arrow_pos + 2..];
        let value_trimmed = value_part.trim().trim_end_matches(')').trim();
        if value_trimmed.is_empty() {
            return Err(format!("Invalid mapping type: {}", mapping_type));
        }
        return Ok(value_trimmed.to_string());
    }
    Err(format!("Invalid mapping type: {}", mapping_type))
}

/// Extract element type from array signature
/// "address[]" -> "address"
/// "uint256[10]" -> "uint256"
fn extract_array_element_type(array_type: &str) -> Result<String, String> {
    if let Some(bracket_pos) = array_type.find('[') {
        let element = array_type[..bracket_pos].trim();
        if element.is_empty() {
            return Err(format!("Invalid array type: {}", array_type));
        }
        return Ok(element.to_string());
    }
    Err(format!("Invalid array type: {}", array_type))
}

/// Convert a fetched contract into ContractInfo for BAML processing
pub fn etherscan_to_contract_info(
    contract: Contract,
    description: Option<String>,
) -> Result<ContractInfo, String> {
    let abi = serde_json::to_string(&contract.abi)
        .map_err(|e| format!("Failed to serialize ABI: {}", e))?;

    let source_code = if contract.source_code.trim().is_empty() {
        None
    } else {
        Some(contract.source_code.clone())
    };

    Ok(ContractInfo {
        description,
        address: Some(contract.address),
        abi: Some(abi),
        source_code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_client::models::{
        AccessControlConfig as BamlAccessControlConfig, EventAction, EventHandlerConfig, SlotInfo,
    };
    use std::collections::HashMap;

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

    #[test]
    fn test_layout_analysis_to_storage_handlers() {
        let layout = LayoutAnalysisResult {
            contract_name: "ValidatorManager".to_string(),
            detected_constants: None,
            inheritance: "ValidatorManager".to_string(),
            proxy_pattern: None,
            slots: vec![
                SlotInfo {
                    base_slot: Some("0x0".to_string()),
                    getter_signature: Some("owner()".to_string()),
                    name: "owner".to_string(),
                    notes: None,
                    offset: Some(0),
                    r#type: "address".to_string(),
                },
                SlotInfo {
                    base_slot: Some("0x1".to_string()),
                    getter_signature: Some("validators(uint256,address)".to_string()),
                    name: "validators".to_string(),
                    notes: None,
                    offset: None,
                    r#type: "mapping(uint256 => mapping(address => bool))".to_string(),
                },
                SlotInfo {
                    base_slot: Some("0x2".to_string()),
                    getter_signature: None,
                    name: "validatorList".to_string(),
                    notes: Some("Dynamic list of validators".to_string()),
                    offset: None,
                    r#type: "address[]".to_string(),
                },
            ],
            solidity_version: Some("^0.8.0".to_string()),
            summary: "Validator layout".to_string(),
            warnings: None,
        };

        let handlers =
            layout_analysis_to_storage_handlers(layout).expect("should convert layout analysis");

        assert_eq!(handlers.len(), 3);

        let (_, owner_handler) = handlers
            .iter()
            .find(|(name, _)| name == "owner")
            .expect("missing owner handler");

        match owner_handler {
            HandlerDefinition::Storage {
                slot,
                offset,
                return_type,
                ..
            } => {
                assert_eq!(slot.as_ref(), Some(&json!("0x0")));
                assert_eq!(offset, &Some(0));
                assert_eq!(return_type.as_deref(), Some("address"));
            }
            _ => panic!("expected storage handler for owner"),
        }

        let (_, mapping_handler) = handlers
            .iter()
            .find(|(name, _)| name == "validators")
            .expect("missing validators handler");

        match mapping_handler {
            HandlerDefinition::Storage {
                slot,
                offset,
                return_type,
                ..
            } => {
                assert!(offset.is_none());
                assert_eq!(return_type.as_deref(), Some("bool"));
                let expected = json!(["0x1", "{{ key0 }}", "{{ key1 }}"]);
                assert_eq!(slot.as_ref(), Some(&expected));
            }
            _ => panic!("expected storage handler for validators mapping"),
        }

        let (_, array_handler) = handlers
            .iter()
            .find(|(name, _)| name == "validatorList")
            .expect("missing validatorList handler");

        match array_handler {
            HandlerDefinition::DynamicArray {
                slot,
                return_type,
                ignore_relative,
            } => {
                assert_eq!(slot.as_ref(), Some(&json!("0x2")));
                assert_eq!(return_type.as_deref(), Some("address"));
                assert_eq!(ignore_relative, &Some(false));
            }
            _ => panic!("expected dynamic array handler"),
        }
    }

    #[test]
    fn test_event_analysis_to_event_handlers() {
        let event_actions = vec![
            EventAction {
                field_name: "validatorsByChain".to_string(),
                action_description: "Track validators per chain".to_string(),
                handler: EventActionHandler::EventHandlerConfig(EventHandlerConfig {
                    event_signature: Some(
                        "ValidatorAdded(uint256 indexed chainId,address indexed validator)"
                            .to_string(),
                    ),
                    select_field: "validator".to_string(),
                    return_type: "address".to_string(),
                    add: Some(BamlEventOperation {
                        event_signature:
                            "ValidatorAdded(uint256 indexed chainId,address indexed validator)"
                                .to_string(),
                        r#where: Some(vec!["=".to_string(), "#chainId".to_string(), "325".to_string()]),
                    }),
                    remove: Some(BamlEventOperation {
                        event_signature:
                            "ValidatorRemoved(uint256 indexed chainId,address indexed validator)"
                                .to_string(),
                        r#where: Some(vec!["=".to_string(), "#chainId".to_string(), "325".to_string()]),
                    }),
                }),
            },
            EventAction {
                field_name: "l2Senders".to_string(),
                action_description: "Track role members".to_string(),
                handler: EventActionHandler::AccessControlConfig(BamlAccessControlConfig {
                    event_signature:
                        "RoleGranted(bytes32 indexed role,address indexed account,address indexed sender)"
                            .to_string(),
                    pick_role_members: Some("L2_TX_SENDER_ROLE".to_string()),
                    role_names: Some(HashMap::from([(
                        "0x1234".to_string(),
                        "L2_TX_SENDER_ROLE".to_string()
                    )])),
                }),
            },
        ];

        let event_result = EventAnalyzeResult {
            event_actions,
            detected_constants: None,
            proxy_pattern: None,
            summary: "Validator and role events".to_string(),
            warnings: None,
        };

        let handlers = event_analysis_to_event_handlers(event_result);
        assert_eq!(handlers.len(), 2);

        let (_, validator_handler) = handlers
            .iter()
            .find(|(name, _)| name == "validatorsByChain")
            .expect("missing validators handler");

        match validator_handler {
            HandlerDefinition::Event {
                event,
                return_type,
                select,
                add,
                remove,
                ignore_relative,
                ..
            } => {
                assert_eq!(
                    event.as_deref(),
                    Some("ValidatorAdded(uint256 indexed chainId,address indexed validator)")
                );
                assert_eq!(return_type.as_deref(), Some("address"));
                assert_eq!(select.as_ref(), Some(&json!("validator")));
                assert_eq!(ignore_relative, &Some(false));

                let add = add.as_ref().expect("expected add operation");
                assert_eq!(
                    add.events(),
                    &[
                        "ValidatorAdded(uint256 indexed chainId,address indexed validator)"
                            .to_string()
                    ]
                );
                assert_eq!(add.where_clause, Some(json!(["=", "chainId", "325"])));

                let remove = remove.as_ref().expect("expected remove operation");
                assert_eq!(
                    remove.events(),
                    &[
                        "ValidatorRemoved(uint256 indexed chainId,address indexed validator)"
                            .to_string()
                    ]
                );
                assert_eq!(remove.where_clause, Some(json!(["=", "chainId", "325"])));
            }
            _ => panic!("expected event handler"),
        }

        let (_, access_control_handler) = handlers
            .iter()
            .find(|(name, _)| name == "l2Senders")
            .expect("missing access control handler");

        match access_control_handler {
            HandlerDefinition::AccessControl {
                role_names,
                pick_role_members,
                ignore_relative,
                extra,
            } => {
                assert_eq!(
                    role_names
                        .as_ref()
                        .and_then(|map| map.get("0x1234"))
                        .map(|s| s.as_str()),
                    Some("L2_TX_SENDER_ROLE")
                );
                assert_eq!(pick_role_members.as_deref(), Some("L2_TX_SENDER_ROLE"));
                assert_eq!(ignore_relative, &Some(false));
                assert!(extra.is_none());
            }
            _ => panic!("expected access control handler"),
        }
    }

    #[test]
    fn test_etherscan_to_contract_info_with_abi_string() {
        let contract = Contract {
            address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            chain: "ethereum".to_string(),
            chain_id: 1,
            source_code: "".to_string(),
            abi: serde_json::from_str(r#"[{"type":"function","name":"totalSupply"}]"#).unwrap(),
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: None,
            implementation_address: None,
            created_at: None,
            updated_at: None,
        };

        let contract_info =
            etherscan_to_contract_info(contract, Some("USDC Proxy".to_string())).unwrap();

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
        let contract = Contract {
            address: "0x123".to_string(),
            chain: "ethereum".to_string(),
            chain_id: 1,
            source_code: "contract MyContract { uint256 public value; }".to_string(),
            abi: serde_json::json!([{"type":"function"}]),
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: None,
            implementation_address: None,
            created_at: None,
            updated_at: None,
        };

        let contract_info = etherscan_to_contract_info(contract, None).unwrap();

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
        let contract = Contract {
            address: "0xabc".to_string(),
            chain: "ethereum".to_string(),
            chain_id: 1,
            source_code: "   ".to_string(),
            abi: serde_json::json!([]),
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: None,
            implementation_address: None,
            created_at: None,
            updated_at: None,
        };

        let contract_info =
            etherscan_to_contract_info(contract, Some("Empty contract".to_string())).unwrap();

        assert_eq!(contract_info.address, Some("0xabc".to_string()));
        assert_eq!(
            contract_info.description,
            Some("Empty contract".to_string())
        );
        assert!(contract_info.abi.is_some());
        assert!(contract_info.source_code.is_none());
    }
}
