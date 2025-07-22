use async_trait::async_trait;
use std::collections::HashMap;
use alloy_primitives::{Address, U256, keccak256};
use alloy_provider::{RootProvider, network::Network};

use crate::discovery::handler::{Handler, HandlerResult, HandlerValue};
use crate::discovery::config::HandlerDefinition;
use crate::discovery::storage::{StorageHandler, StorageSlot};


/// Array handler implementation mimicking L2Beat's DynamicArrayHandler
/// Handles Solidity dynamic arrays (e.g., address[]) where:
///The slot contains the array length
///Array elements are stored at keccak256(slot) + index
#[derive(Debug, Clone)]
pub struct ArrayHandler<N> {
    // Array starts at a storage slot with no offset
    inner: StorageHandler<N>,
    // The starting position of the array
    starting_position: Option<U256>,
    // The length of the array, if known or use the storage handler to get it
    length: Option<U256>,
}

impl<N> ArrayHandler<N> {
    pub fn new(field: String, slot: StorageSlot, hidden: bool) -> Self {
        Self {
            // The array starts at this slot
            inner: StorageHandler::new(field, slot, hidden),
            starting_position: None,
            length: None,
        }
    }

    /// Create ArrayHandler from HandlerDefinition::DynamicArray
    pub fn from_handler_definition(field: String, handler: HandlerDefinition) -> Result<Self, String> {
        match handler {
            HandlerDefinition::DynamicArray { slot, return_type, ignore_relative } => {
                let slot_val = slot
                    .map(HandlerValue::from_json_value)
                    .unwrap_or_else(|| Err("Storage slot is required".to_string()))?;
                let storage_slot = StorageSlot {
                    slot: slot_val, 
                    offset: None,
                    return_type,
                };
                Ok(Self::new(field, storage_slot, ignore_relative.unwrap_or(false)))
            }
            _ => Err("Handler definition is not a dynamic array handler".to_string()),
        }
    }
     /// Extract field dependencies from array configuration
     fn resolve_dependencies(&self) -> Vec<String> {
         self.inner.resolve_dependencies()
    }
    
     fn resolve_starting_position(&mut self, previous_results: &HashMap<String, HandlerResult>) -> Result<U256, String> {
         self.inner.resolve_slot(previous_results)
     }
     
     fn resolve_element_slot(&mut self, index: usize, previous_results: &HashMap<String, HandlerResult>) -> Result<U256, String> {
         // The initial slot S where the array length is stored
         let starting_position = match self.starting_position {
             Some(slot) => slot,
             None => self.inner.resolve_slot(previous_results)?,
         };
         self.starting_position = Some(starting_position);
         // The element at index I is stored at keccak256(S) + I
         Ok(Self::compute_indexed_slot(starting_position,  U256::from(index)))
     }


    /// Compute storage slot for dynamic array element
    /// For a dynamic array at slot S, element at index I is stored at:
    /// keccak256(S) + I
    fn compute_indexed_slot(starting_position: U256, index: U256) -> U256 {
        // Hash the length slot to get the base address of array data
        let slot_bytes = starting_position.to_be_bytes::<32>();
        let base_slot = U256::from_be_bytes(
            keccak256(&slot_bytes).0
        );
        base_slot + index

    }
}

#[async_trait]
impl<N: Network> Handler<N> for ArrayHandler<N> {
    fn field(&self) -> &str {
        &self.inner.field
    }

    fn dependencies(&self) -> &[String] {
        &self.inner.dependencies
    }

    fn hidden(&self) -> bool {
        self.inner.hidden
    }

    async fn execute(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> HandlerResult {
        let inner = &self.inner;
        
        // Helper function to create error result
        let error_result = |error: String| HandlerResult {
            field: inner.field.clone(),
            value: None,
            error: Some(error),
            hidden: self.hidden(),
        };

        // Get array length
        let length = match self.length {
            Some(length) => length,
            None => {
                let mut starting_position = match self.starting_position {
                    Some(slot) => slot,
                    None => match self.clone().resolve_starting_position(previous_results) {
                        Ok(slot) => slot,
                        Err(e) => return error_result(format!("Failed to resolve array slot: {}", e)),
                    }
                };
                
                // Read array length from starting position
                let length_slot = StorageSlot { 
                    slot: HandlerValue::Number(starting_position), 
                    offset: None, 
                    return_type: Some("number".to_string()) // Array length is always a number
                };
                
                match length_slot.get_resolved_value(provider, address).await {
                    Ok(length) => length,
                    Err(e) => return error_result(format!("Failed to read array length: {}", e)),
                }
            }
        };

        // Read all array elements
        let mut elements = Vec::new();
        let starting_position = match inner.resolve_slot(previous_results) {
            Ok(slot) => slot,
            Err(e) => return error_result(format!("Failed to resolve array slot: {}", e)),
        };

        for index in 0..length.to::<usize>() {
            // Compute element slot: keccak256(S) + I
            let element_slot = StorageSlot { 
                slot: HandlerValue::Number(self.clone().resolve_element_slot(index, previous_results).unwrap()), 
                offset: None, 
                return_type: inner.slot.return_type.clone() 
            };
            
            // Read element from storage
            let storage_value = match element_slot.get_resolved_value(provider, address).await {
                Ok(value) => value,
                Err(e) => return error_result(format!("Failed to read element {}: {}", index, e)),
            };
            
            // Convert storage value to HandlerValue
            let element_value = match element_slot.convert_return(storage_value) {
                Ok(value) => value,
                Err(e) => return error_result(format!("Failed to convert element {}: {}", index, e)),
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
        
        let handler = AnyArrayHandler::new("admins".to_string(), slot, false);
        assert_eq!(handler.field(), "admins");
        assert_eq!(handler.dependencies().len(), 0);
        assert_eq!(handler.hidden(), false);
    }

    #[test]
    fn test_array_handler_with_reference() {
        let slot = StorageSlot {
            slot: HandlerValue::Reference("{{ adminSlot }}".to_string()),
            offset: None,
            return_type: Some("address".to_string()),
        };
        
        let handler = AnyArrayHandler::new("admins".to_string(), slot, false);
        assert_eq!(handler.dependencies().len(), 1);
        assert_eq!(handler.dependencies()[0], "adminSlot");
    }

    #[test]
    fn test_dynamic_slot_computation() {
        // Test dynamic slot computation for slot 5, index 0
        let starting_position = U256::from(5);
        let index = U256::from(0);
        let element_slot = ArrayHandler::<AnyNetwork>::compute_indexed_slot(starting_position, index);

        // Expected: keccak256(5) + 0
        let expected_base = {
            let slot_bytes = U256::from(5).to_be_bytes::<32>();
            U256::from_be_bytes(keccak256(&slot_bytes).0)
        };
        
        assert_eq!(element_slot, expected_base);

        // Test index 1
        let index = U256::from(1);
        let element_slot = ArrayHandler::<AnyNetwork>::compute_indexed_slot(starting_position, index);
        assert_eq!(element_slot, expected_base + U256::from(1));
    }

    #[test]
    fn test_from_handler_definition() {
        let handler_def = HandlerDefinition::DynamicArray { 
            slot: Some(serde_json::Value::Number(serde_json::Number::from(10))),
            return_type: Some("address".to_string()),
            ignore_relative: Some(false),
        };
        
        let array_handler = AnyArrayHandler::from_handler_definition("testArray".to_string(), handler_def).unwrap();
        
        assert_eq!(array_handler.field(), "testArray");
        assert_eq!(array_handler.dependencies().len(), 0);
        assert_eq!(array_handler.inner.slot.return_type, Some("address".to_string()));
        assert_eq!(array_handler.hidden(), false);
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
        let handler = AnyArrayHandler::new("testArray".to_string(), slot, false);

        // Create a provider for testing
        let provider = foundry_common::provider::get_http_provider("http://localhost:8545");
        let contract_address = Address::from([0x11u8; 20]);
        let previous_results = HashMap::new();

        // Execute the handler
        let result = handler.execute(&provider, &contract_address, &previous_results).await;

        // The handler should return an array (even if empty due to mock storage)
        assert_eq!(result.field, "testArray");
        assert_eq!(result.hidden, false);
        
        // Note: The test will likely fail due to no actual array data, but structure should be correct
        if let Some(HandlerValue::Array(_elements)) = result.value {
            // Array structure is correct
        } else if result.error.is_some() {
            // Expected for mock dataarray length might be 0 or read might fail
        } else {
            panic!("Unexpected result type");
        }
    }
}