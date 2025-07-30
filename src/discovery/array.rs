use alloy_primitives::{Address, U256, keccak256};
use alloy_provider::{RootProvider, network::Network};
use async_trait::async_trait;
use std::collections::HashMap;

use crate::discovery::call::{CallConfig, CallHandler};
use crate::discovery::config::HandlerDefinition;
use crate::discovery::handler::{Handler, HandlerResult, HandlerValue};
use crate::discovery::storage::{StorageHandler, StorageSlot};

/// Unified array handler for both dynamic and static Solidity arrays.
/// Dynamic arrays: reads length from storage slot, elements at keccak256(slot) + index
/// Static arrays: calls contract method with index parameters
#[derive(Debug, Clone, Default)]
pub struct ArrayHandler<N> {
    // Array starts at a storage slot with no offset
    dyn_slot: Option<StorageHandler<N>>,
    // Call get method for static array optionally
    static_call: Option<CallHandler<N>>,
    // The starting position of the array
    starting_position: Option<U256>,
    // The length of the dynamic array, if known or use the storage handler to get it
    dyn_length: Option<U256>,
    // The indices of the array elements to target
    target_indices: Option<Vec<usize>>,
    // The range of the array elements to target
    target_range: Option<(usize, usize)>,
}

impl<N> ArrayHandler<N> {
    pub fn new_dynamic(field: String, slot: StorageSlot, hidden: bool) -> Self {
        Self {
            // The array starts at this slot
            dyn_slot: Some(StorageHandler::new(field, slot, hidden)),
            starting_position: None,
            dyn_length: None,
            static_call: None,
            target_indices: None,
            target_range: None,
        }
    }

    pub fn new_static(
        field: String,
        method: Option<String>,
        target_indices: Option<Vec<usize>>,
        target_range: Option<(usize, usize)>,
        hidden: bool,
    ) -> Self {
        let call_config = CallConfig {
            method: method.unwrap_or(field.clone()),
            params: None,
            address: None,
            expect_revert: None,
        };
        Self {
            dyn_slot: None,
            starting_position: None,
            dyn_length: None,
            static_call: Some(CallHandler::new(field, call_config, hidden)),
            target_indices,
            target_range,
        }
    }

    /// Create ArrayHandler from HandlerDefinition::DynamicArray
    pub fn from_handler_definition(field: String, handler: HandlerDefinition) -> Result<Self, String> {
        match handler {
            HandlerDefinition::DynamicArray {
                slot,
                return_type,
                ignore_relative,
            } => {
                let slot_val = slot
                    .map(HandlerValue::from_json_value)
                    .unwrap_or_else(|| Err("Storage slot is required".to_string()))?;
                let storage_slot = StorageSlot {
                    slot: slot_val,
                    offset: None,
                    return_type,
                };
                Ok(Self::new_dynamic(field, storage_slot, ignore_relative.unwrap_or(false)))
            }
            HandlerDefinition::Array {
                method,
                max_length,
                indices,
                length,
                start_index,
                return_type: _,
                ignore_relative,
            } => {
                let target_indecies = if let Some(indices) = indices {
                    let indices = match indices {
                        serde_json::Value::Array(indices) => {
                            indices.iter().map(|v| v.as_u64().unwrap() as usize).collect()
                        }
                        serde_json::Value::Number(n) => vec![n.as_u64().unwrap() as usize],
                        _ => return Err("Indices must be an array of numbers".to_string()),
                    };
                    Some(indices)
                } else {
                    None
                };

                let target_range = if target_indecies.is_none() {
                    let upper_bound =
                        max_length.unwrap_or(u64::MAX).min(length.map(|l| l.as_u64().unwrap()).unwrap_or(u64::MAX));
                    let lower_bound = start_index.unwrap_or(0);
                    Some((lower_bound as usize, upper_bound as usize))
                } else {
                    None
                };

                Ok(Self::new_static(field, method, target_indecies, target_range, ignore_relative.unwrap_or(false)))
            }
            _ => Err("Handler definition is not a dynamic array handler".to_string()),
        }
    }
    /// Extract field dependencies from array configuration
    fn resolve_dependencies(&self) -> Vec<String> {
        let mut dependencies = Vec::new();
        if let Some(dyn_slot) = &self.dyn_slot {
            dependencies.extend(dyn_slot.resolve_dependencies());
        }
        if let Some(static_call) = &self.static_call {
            dependencies.extend(static_call.resolve_dependencies());
        }
        dependencies
    }

    fn resolve_starting_position(&mut self, previous_results: &HashMap<String, HandlerResult>) -> Result<U256, String> {
        if let Some(dyn_slot) = &self.dyn_slot {
            return dyn_slot.resolve_slot(previous_results);
        }
        Err("Can't fetch starting position for static array".to_string())
    }

    fn resolve_dynamic_slot(
        &mut self,
        index: usize,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<U256, String> {
        if self.dyn_slot.is_none() {
            return Err("Can't fetch dynamic slot for static array".to_string());
        }
        // The initial slot S where the array length is stored
        let starting_position = match self.starting_position {
            Some(slot) => slot,
            None => self.dyn_slot.as_ref().unwrap().resolve_slot(previous_results)?,
        };
        self.starting_position = Some(starting_position);
        // The element at index I is stored at keccak256(S) + I
        Ok(Self::compute_dynamic_slot(starting_position, U256::from(index)))
    }

    /// Compute storage slot for dynamic array element
    /// For a dynamic array at slot S, element at index I is stored at:
    /// keccak256(S) + I
    fn compute_dynamic_slot(starting_position: U256, index: U256) -> U256 {
        // Hash the length slot to get the base address of array data
        let slot_bytes = starting_position.to_be_bytes::<32>();
        let base_slot = U256::from_be_bytes(keccak256(&slot_bytes).0);
        base_slot + index
    }
}

impl<N: Network> ArrayHandler<N> {
    async fn execute_dynamic(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> HandlerResult {
        // Helper function to create error result
        let error_result = |error: String| HandlerResult {
            field: self
                .clone()
                .dyn_slot
                .map(|s| s.field)
                .unwrap_or_else(|| self.clone().static_call.map(|c| c.field).unwrap_or_default()),
            value: None,
            error: Some(error),
            hidden: self.hidden(),
        };

        let inner = match &self.dyn_slot {
            Some(dyn_slot) => dyn_slot,
            None => return error_result("Dynamic slot is not set".to_string()),
        };

        // Get array length
        let length = match self.dyn_length {
            Some(length) => length,
            None => {
                let starting_position = match self.starting_position {
                    Some(slot) => slot,
                    None => match self.clone().resolve_starting_position(previous_results) {
                        Ok(slot) => slot,
                        Err(e) => {
                            return error_result(format!("Failed to resolve array slot: {}", e));
                        }
                    },
                };

                // Read array length from starting position
                let length_slot = StorageSlot {
                    slot: HandlerValue::Number(starting_position),
                    offset: None,
                    return_type: Some("number".to_string()), // Array length is always a number
                };

                match length_slot.get_resolved_value(provider, address).await {
                    Ok(length) => length,
                    Err(e) => return error_result(format!("Failed to read array length: {}", e)),
                }
            }
        };

        // Read all array elements
        let mut elements = Vec::new();
        for index in 0..length.to::<usize>() {
            // Compute element slot: keccak256(S) + I
            let element_slot = StorageSlot {
                slot: HandlerValue::Number(self.clone().resolve_dynamic_slot(index, previous_results).unwrap()),
                offset: None,
                return_type: inner.slot.return_type.clone(),
            };

            // Read element from storage
            let storage_value = match element_slot.get_resolved_value(provider, address).await {
                Ok(value) => value,
                Err(e) => return error_result(format!("Failed to read element {}: {}", index, e)),
            };

            // Convert storage value to HandlerValue
            let element_value = match element_slot.convert_return(storage_value) {
                Ok(value) => value,
                Err(e) => {
                    return error_result(format!("Failed to convert element {}: {}", index, e));
                }
            };

            elements.push(element_value);
        }

        HandlerResult {
            field: inner.field.clone(),
            value: Some(HandlerValue::Array(elements)),
            error: None,
            hidden: self.hidden(),
        }
    }

    async fn execute_static(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> HandlerResult {
        // Helper function to create error result
        let error_result = |error: String| HandlerResult {
            field: self.clone().static_call.as_ref().map(|c| c.field.clone()).unwrap_or_default(),
            value: None,
            error: Some(error),
            hidden: self.hidden(),
        };

        let static_call = match &self.static_call {
            Some(call) => call,
            None => return error_result("Static call is not set".to_string()),
        };

        let mut elements = Vec::new();

        // Determine which indices to call
        if let Some(indices) = &self.target_indices {
            // Call specific indices
            for &index in indices {
                // Create a call with the index parameter
                let mut indexed_call = static_call.clone();
                indexed_call.call.params = Some(vec![HandlerValue::Number(U256::from(index))]);

                // Execute the call
                let result = indexed_call.execute(provider, address, previous_results).await;

                if let Some(error) = result.error {
                    // Check if it's a revert due to out-of-bounds access
                    if error.contains("Execution reverted") || error.contains("revert") {
                        // For static arrays, out-of-bounds is expected and we should stop
                        break;
                    } else {
                        return error_result(format!("Failed to call index {}: {}", index, error));
                    }
                }

                if let Some(value) = result.value {
                    elements.push(value);
                }
            }
        } else if let Some((start, end)) = &self.target_range {
            // Call range of indices
            for index in *start..*end {
                // Create a call with the index parameter
                let mut indexed_call = static_call.clone();
                indexed_call.call.params = Some(vec![HandlerValue::Number(U256::from(index))]);

                // Execute the call
                let result = indexed_call.execute(provider, address, previous_results).await;

                if let Some(error) = result.error {
                    // Check if it's a revert due to out-of-bounds access
                    if error.contains("Execution reverted") || error.contains("revert") {
                        // For static arrays, out-of-bounds is expected and we should stop
                        break;
                    } else {
                        return error_result(format!("Failed to call index {}: {}", index, error));
                    }
                }

                if let Some(value) = result.value {
                    elements.push(value);
                } else {
                    // No value returned, might be end of array
                    break;
                }
            }
        } else {
            return error_result("No target indices or range specified for static array".to_string());
        }

        // Check if we hit the max length limit
        if let Some((_, end)) = &self.target_range {
            if elements.len() == (*end - self.target_range.unwrap().0) {
                return HandlerResult {
                    field: static_call.field.clone(),
                    value: Some(HandlerValue::Array(elements)),
                    error: Some("Too many values. Array might be longer than expected range".to_string()),
                    hidden: self.hidden(),
                };
            }
        }

        HandlerResult {
            field: static_call.field.clone(),
            value: Some(HandlerValue::Array(elements)),
            error: None,
            hidden: self.hidden(),
        }
    }
}

#[async_trait]
impl<N: Network> Handler<N> for ArrayHandler<N> {
    fn field(&self) -> &str {
        if let Some(dyn_slot) = &self.dyn_slot {
            return dyn_slot.field();
        } else if let Some(static_call) = &self.static_call {
            return static_call.field();
        } else {
            panic!("No field found for array handler");
        }
    }

    fn dependencies(&self) -> &[String] {
        if let Some(dyn_slot) = &self.dyn_slot {
            return dyn_slot.dependencies();
        } else if let Some(static_call) = &self.static_call {
            return static_call.dependencies();
        } else {
            panic!("No dependencies found for array handler");
        }
    }

    fn hidden(&self) -> bool {
        if let Some(dyn_slot) = &self.dyn_slot {
            return dyn_slot.hidden();
        } else if let Some(static_call) = &self.static_call {
            return static_call.hidden();
        } else {
            panic!("No hidden value found for array handler");
        }
    }

    async fn execute(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> HandlerResult {
        // Branch based on whether this is a dynamic or static array handler
        if self.dyn_slot.is_some() {
            // Dynamic array: read from storage
            self.execute_dynamic(provider, address, previous_results).await
        } else if self.static_call.is_some() {
            // Static array: call method with indices
            self.execute_static(provider, address, previous_results).await
        } else {
            // Error: neither dynamic nor static configuration is set
            HandlerResult {
                field: "unknown".to_string(),
                value: None,
                error: Some("ArrayHandler has neither dynamic slot nor static call configured".to_string()),
                hidden: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_provider::network::AnyNetwork;

    type AnyArrayHandler = ArrayHandler<AnyNetwork>;

    #[test]
    fn test_array_handler_creation() {
        let slot = StorageSlot {
            slot: HandlerValue::Number(U256::from(5)),
            offset: None,
            return_type: Some("address".to_string()),
        };

        let handler = AnyArrayHandler::new_dynamic("admins".to_string(), slot, false);
        // Basic functionality test
        assert_eq!(handler.field(), "admins");
    }

    #[test]
    fn test_array_handler_with_reference() {
        let slot = StorageSlot {
            slot: HandlerValue::Reference("{{ adminSlot }}".to_string()),
            offset: None,
            return_type: Some("address".to_string()),
        };

        let handler = AnyArrayHandler::new_dynamic("admins".to_string(), slot, false);
        // Test dependency resolution - core functionality
        assert_eq!(handler.dependencies()[0], "adminSlot");
    }

    #[test]
    fn test_dynamic_slot_computation() {
        // Test dynamic slot computation for slot 5, index 0
        let starting_position = U256::from(5);
        let index = U256::from(0);
        let element_slot = ArrayHandler::<AnyNetwork>::compute_dynamic_slot(starting_position, index);

        // Expected: keccak256(5) + 0
        let expected_base = {
            let slot_bytes = U256::from(5).to_be_bytes::<32>();
            U256::from_be_bytes(keccak256(&slot_bytes).0)
        };

        assert_eq!(element_slot, expected_base);

        // Test index 1
        let index = U256::from(1);
        let element_slot = ArrayHandler::<AnyNetwork>::compute_dynamic_slot(starting_position, index);
        assert_eq!(element_slot, expected_base + U256::from(1));
    }

    #[test]
    fn test_from_handler_definition_dynamic() {
        let handler_def = HandlerDefinition::DynamicArray {
            slot: Some(serde_json::Value::Number(serde_json::Number::from(10))),
            return_type: Some("address".to_string()),
            ignore_relative: Some(false),
        };

        let array_handler = AnyArrayHandler::from_handler_definition("testArray".to_string(), handler_def).unwrap();

        // Focus on core functionality: dynamic slot configuration
        assert!(array_handler.dyn_slot.is_some());
        assert!(array_handler.static_call.is_none());
    }

    #[test]
    fn test_from_handler_definition_static() {
        let handler_def = HandlerDefinition::Array {
            method: Some("getAdmin".to_string()),
            max_length: Some(10),
            return_type: Some("address".to_string()),
            indices: None,
            length: None,
            start_index: Some(0),
            ignore_relative: Some(false),
        };

        let array_handler = AnyArrayHandler::from_handler_definition("admins".to_string(), handler_def).unwrap();

        // Focus on core functionality: static call configuration
        assert!(array_handler.dyn_slot.is_none());
        assert!(array_handler.static_call.is_some());
        assert_eq!(array_handler.target_range, Some((0, 10)));
    }

    #[test]
    fn test_static_array_with_indices() {
        let handler_def = HandlerDefinition::Array {
            method: Some("admin".to_string()),
            max_length: None,
            return_type: Some("address".to_string()),
            indices: Some(serde_json::Value::Array(vec![
                serde_json::Value::Number(serde_json::Number::from(0)),
                serde_json::Value::Number(serde_json::Number::from(2)),
                serde_json::Value::Number(serde_json::Number::from(5)),
            ])),
            length: None,
            start_index: None,
            ignore_relative: Some(false),
        };

        let array_handler =
            AnyArrayHandler::from_handler_definition("specificAdmins".to_string(), handler_def).unwrap();

        // Focus on core functionality: specific indices configuration
        assert_eq!(array_handler.target_indices, Some(vec![0, 2, 5]));
        assert!(array_handler.target_range.is_none());
    }

    #[test]
    fn test_wrong_handler_definition_type() {
        let handler_def = HandlerDefinition::Storage {
            slot: Some(serde_json::Value::Number(serde_json::Number::from(3))),
            offset: None,
            return_type: Some("address".to_string()),
            ignore_relative: None,
        };

        let result = AnyArrayHandler::from_handler_definition("test".to_string(), handler_def);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Handler definition is not a dynamic array handler"));
    }

    #[tokio::test]
    async fn test_array_handler_execution() {
        // Create a mock array handler
        let slot = StorageSlot {
            slot: HandlerValue::Number(U256::from(10)),
            offset: None,
            return_type: Some("address".to_string()),
        };
        let handler = AnyArrayHandler::new_dynamic("testArray".to_string(), slot, false);

        // Create a provider for testing
        let provider = foundry_common::provider::get_http_provider("http://localhost:8545");
        let contract_address = Address::from([0x11u8; 20]);
        let previous_results = HashMap::new();

        // Execute the handler
        let result = handler.execute(&provider, &contract_address, &previous_results).await;

        // Focus on end result: handler executes without panicking
        assert_eq!(result.field, "testArray");
    }

    #[test]
    fn test_e2e_sharp_verifier_dynamic_array() {
        use crate::discovery::config::parse_config_file;
        use std::path::Path;

        // Test E2E parsing of the SHARP verifier template for dynamic array
        let template_path =
            Path::new("src/discovery/projects/_templates/shared-sharp-verifier/SHARPVerifier/template.jsonc");
        let contract_config = parse_config_file(template_path).expect("Failed to parse template file");

        // Test cpuFrilessVerifiers dynamic array field
        let fields = contract_config.fields.expect("Contract should have fields");
        let cpu_verifiers_field = fields.get("cpuFrilessVerifiers").expect("cpuFrilessVerifiers field not found");
        let handler_def = cpu_verifiers_field.handler.as_ref().expect("Handler definition not found");

        let array_handler =
            AnyArrayHandler::from_handler_definition("cpuFrilessVerifiers".to_string(), handler_def.clone())
                .expect("Failed to create ArrayHandler");

        // Focus on end result: dynamic array configuration works
        assert!(array_handler.dyn_slot.is_some());
        assert!(array_handler.static_call.is_none());
    }

    #[test]
    fn test_e2e_dispute_game_factory_static_array() {
        use crate::discovery::config::parse_config_file;
        use std::path::Path;

        // Test E2E parsing of the DisputeGameFactory template for static array
        let template_path = Path::new(
            "/Users/ceciliazhang/Code/l2beat/packages/config/src/projects/_templates/opstack/DisputeGameFactory/template.jsonc",
        );
        let contract_config = parse_config_file(template_path).expect("Failed to parse template file");

        // Test gameImpls static array field
        let fields = contract_config.fields.expect("Contract should have fields");
        let game_impls_field = fields.get("gameImpls").expect("gameImpls field not found");
        let handler_def = game_impls_field.handler.as_ref().expect("Handler definition not found");

        let array_handler = AnyArrayHandler::from_handler_definition("gameImpls".to_string(), handler_def.clone())
            .expect("Failed to create ArrayHandler");

        // Focus on end result: static array configuration works
        assert!(array_handler.dyn_slot.is_none());
        assert!(array_handler.static_call.is_some());
        assert_eq!(array_handler.target_range, Some((0, 5)));

        // Test initBonds static array field
        let init_bonds_field = fields.get("initBonds").expect("initBonds field not found");
        let handler_def = init_bonds_field.handler.as_ref().expect("Handler definition not found");

        let array_handler = AnyArrayHandler::from_handler_definition("initBonds".to_string(), handler_def.clone())
            .expect("Failed to create ArrayHandler");

        // Focus on end result: static array configuration works
        assert!(array_handler.dyn_slot.is_none());
        assert!(array_handler.static_call.is_some());
        assert_eq!(array_handler.target_range, Some((0, 5)));
    }
}
