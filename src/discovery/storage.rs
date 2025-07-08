use async_trait::async_trait;
use std::collections::HashMap;
use alloy_primitives::{Address, U256, keccak256, Bytes};
use serde::{Deserialize, Serialize};

use crate::discovery::handler::{Handler, HandlerResult, HandlerValue};



/// Storage return type using discriminant for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageReturnType {
    Address,
    Bytes,
    Number,
    #[serde(rename = "uint8")]
    Uint8,
}

/// Storage slot configuration, similar to L2Beat's StorageHandler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSlot {
    pub slot: SlotValue,
    pub offset: Option<u32>,
    pub return_type: Option<StorageReturnType>,
}

/// Storage slot value - can be a direct number or computed from other values
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SlotValue {
    Direct(U256),
    Array(Vec<SlotValue>),
    Reference(String), // For references like "{{ admin }}"
}

/// Storage handler implementation mimicking L2Beat's StorageHandler
#[derive(Debug, Clone)]
pub struct StorageHandler {
    pub field: String,
    pub dependencies: Vec<String>,
    pub slot: StorageSlot,
}

impl StorageHandler {
    pub fn new(field: String, slot: StorageSlot) -> Self {
        let dependencies = Self::extract_dependencies(&slot);
        Self {
            field,
            dependencies,
            slot,
        }
    }

    /// Extract field dependencies from slot configuration
    fn extract_dependencies(slot: &StorageSlot) -> Vec<String> {
        let mut deps = Vec::new();
        Self::extract_slot_deps(&slot.slot, &mut deps);
        deps
    }

    /// Recursively extract dependencies from slot values
    fn extract_slot_deps(slot_value: &SlotValue, deps: &mut Vec<String>) {
        match slot_value {
            SlotValue::Reference(ref_str) => {
                // Extract field name from reference like "{{ admin }}"
                if let Some(field_name) = Self::parse_reference(ref_str) {
                    deps.push(field_name);
                }
            }
            SlotValue::Array(values) => {
                for value in values {
                    Self::extract_slot_deps(value, deps);
                }
            }
            SlotValue::Direct(_) => {}
        }
    }

    /// Parse reference string like "{{ admin }}" to extract field name
    fn parse_reference(ref_str: &str) -> Option<String> {
        let trimmed = ref_str.trim();
        if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
            let field = trimmed[2..trimmed.len()-2].trim();
            if field.is_empty() {
                None
            } else {
                Some(field.to_string())
            }
        } else {
            None
        }
    }

    /// Compute storage slot based on configuration and previous results
    fn compute_slot(
        &self,
        slot_value: &SlotValue,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<U256, String> {
        match slot_value {
            SlotValue::Direct(slot) => Ok(*slot),
            SlotValue::Array(values) => {
                // For arrays, compute keccak256 hash of all values
                let mut data = Vec::new();
                for value in values {
                    let computed = self.compute_slot(value, previous_results)?;
                    data.extend_from_slice(&computed.to_be_bytes::<32>());
                }
                let hash = keccak256(&data);
                Ok(U256::from_be_bytes(hash.0))
            }
            SlotValue::Reference(ref_str) => {
                if let Some(field_name) = Self::parse_reference(ref_str) {
                    if let Some(result) = previous_results.get(&field_name) {
                        if let Some(value) = &result.value {
                            // Convert HandlerValue to U256 for slot computation
                            value.to_u256()
                                .map_err(|e| format!("Failed to convert reference '{}' to U256: {}", field_name, e))
                        } else {
                            Err(format!("Reference '{}' has no value", field_name))
                        }
                    } else {
                        Err(format!("Reference '{}' not found in previous results", field_name))
                    }
                } else {
                    Err(format!("Invalid reference format: {}", ref_str))
                }
            }
        }
    }


    /// Convert storage value to proper Alloy type based on return type
    fn convert_storage_value(
        &self,
        storage_value: &[u8],
        return_type: &Option<StorageReturnType>,
    ) -> Result<HandlerValue, String> {
        // Ensure we have 32 bytes for storage values
        if storage_value.len() != 32 {
            return Err(format!("Invalid storage value length: expected 32 bytes, got {}", storage_value.len()));
        }

        match return_type {
            Some(StorageReturnType::Address) => {
                let addr_bytes = &storage_value[12..32]; // Address is last 20 bytes
                let address = Address::from_slice(addr_bytes);
                Ok(HandlerValue::Address(address))
            }
            Some(StorageReturnType::Bytes) => {
                Ok(HandlerValue::Bytes(Bytes::copy_from_slice(storage_value)))
            }
            Some(StorageReturnType::Number) => {
                let u256_val = U256::from_be_bytes::<32>(storage_value.try_into().unwrap());
                Ok(HandlerValue::Number(u256_val))
            }
            Some(StorageReturnType::Uint8) => {
                if let Some(offset) = self.slot.offset {
                    let byte_index = offset as usize;
                    if byte_index < storage_value.len() {
                        Ok(HandlerValue::Number(U256::from(storage_value[byte_index])))
                    } else {
                        Err("Offset out of bounds for uint8".to_string())
                    }
                } else {
                    Ok(HandlerValue::Number(U256::from(storage_value[31]))) // Last byte
                }
            }
            None => {
                // Default to bytes representation
                Ok(HandlerValue::Bytes(Bytes::copy_from_slice(storage_value)))
            }
        }
    }
}

#[async_trait]
impl Handler for StorageHandler {
    fn field(&self) -> &str {
        &self.field
    }

    fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    async fn execute(
        &self,
        provider: &(dyn Send + Sync), // TODO: replace with actual provider trait
        address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> HandlerResult {
        // Compute the storage slot
        let slot = match self.compute_slot(&self.slot.slot, previous_results) {
            Ok(slot) => slot,
            Err(error) => {
                return HandlerResult {
                    field: self.field.clone(),
                    value: None,
                    error: Some(format!("Failed to compute storage slot: {}", error)),
                    ignore_relative: None,
                };
            }
        };

        // TODO: Replace with actual provider call
        // For now, we'll simulate the storage read with a placeholder
        let storage_result = self.simulate_storage_read(provider, address, &slot).await;

        match storage_result {
            Ok(storage_value) => {
                // Convert the storage value to proper Alloy type
                match self.convert_storage_value(&storage_value, &self.slot.return_type) {
                    Ok(converted_value) => HandlerResult {
                        field: self.field.clone(),
                        value: Some(converted_value),
                        error: None,
                        ignore_relative: None,
                    },
                    Err(error) => HandlerResult {
                        field: self.field.clone(),
                        value: None,
                        error: Some(format!("Failed to convert storage value: {}", error)),
                        ignore_relative: None,
                    },
                }
            }
            Err(error) => HandlerResult {
                field: self.field.clone(),
                value: None,
                error: Some(format!("Failed to read storage: {}", error)),
                ignore_relative: None,
            },
        }
    }
}

impl StorageHandler {
    /// Simulate storage read - this should be replaced with actual provider call
    async fn simulate_storage_read(
        &self,
        _provider: &(dyn Send + Sync),
        _address: &Address,
        _slot: &U256,
    ) -> Result<Vec<u8>, String> {
        // TODO: Replace with actual provider.get_storage(address, slot) call
        // For now, return a placeholder 32-byte storage value
        Ok(vec![0u8; 32])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_handler_creation() {
        let slot = StorageSlot {
            slot: SlotValue::Direct(U256::from(0)),
            offset: None,
            return_type: Some(StorageReturnType::Address),
        };
        
        let handler = StorageHandler::new("admin".to_string(), slot);
        assert_eq!(handler.field(), "admin");
        assert_eq!(handler.dependencies().len(), 0);
    }

    #[test]
    fn test_reference_extraction() {
        let slot = StorageSlot {
            slot: SlotValue::Reference("{{ owner }}".to_string()),
            offset: None,
            return_type: Some(StorageReturnType::Address),
        };
        
        let handler = StorageHandler::new("admin".to_string(), slot);
        assert_eq!(handler.dependencies().len(), 1);
        assert_eq!(handler.dependencies()[0], "owner");
    }

    #[test]
    fn test_array_slot_dependencies() {
        let slot = StorageSlot {
            slot: SlotValue::Array(vec![
                SlotValue::Direct(U256::from(1)),
                SlotValue::Reference("{{ token }}".to_string()),
            ]),
            offset: None,
            return_type: Some(StorageReturnType::Number),
        };
        
        let handler = StorageHandler::new("balance".to_string(), slot);
        assert_eq!(handler.dependencies().len(), 1);
        assert_eq!(handler.dependencies()[0], "token");
    }

    #[test]
    fn test_parse_reference() {
        // Test cases using json! macro for cleaner organization
        let test_cases = serde_json::json!({
            "valid": [
                {"input": "{{ admin }}", "expected": "admin"},
                {"input": "{{owner}}", "expected": "owner"},
                {"input": "{{ balance_slot }}", "expected": "balance_slot"}
            ],
            "invalid": ["admin", "{{ }}", "", "{ admin }", "{{}}"]
        });

        // Test valid references
        for case in test_cases["valid"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            let expected = case["expected"].as_str().unwrap();
            assert_eq!(
                StorageHandler::parse_reference(input).unwrap(),
                expected,
                "Failed to parse valid reference: {}",
                input
            );
        }

        // Test invalid references
        for invalid_ref in test_cases["invalid"].as_array().unwrap() {
            let input = invalid_ref.as_str().unwrap();
            assert!(
                StorageHandler::parse_reference(input).is_none(),
                "Should not parse invalid reference: {}",
                input
            );
        }
    }

    #[test]
    fn test_handler_value_serialization() {
        // Test basic serialization for types that don't have ambiguity issues
        let test_values = vec![
            HandlerValue::String("test".to_string()),
            HandlerValue::Boolean(true),
        ];

        for value in test_values {
            // Test round-trip serialization using Alloy's built-in serde
            let serialized = serde_json::to_value(&value).expect("Failed to serialize");
            let deserialized: HandlerValue = serde_json::from_value(serialized.clone()).expect("Failed to deserialize");
            
            assert_eq!(value, deserialized, "Round-trip failed for value: {:?}", serialized);
            
            // Verify that serialization produces proper formats
            match &value {
                HandlerValue::String(_) => {
                    // String should serialize as string
                    assert!(serialized.is_string());
                }
                HandlerValue::Boolean(_) => {
                    // Boolean should serialize as boolean
                    assert!(serialized.is_boolean());
                }
                _ => {}
            }
        }

        // Test serialization formats for Alloy types (these have ambiguity with untagged enums)
        let address_val = HandlerValue::Address(Address::from([0x42; 20]));
        let serialized = serde_json::to_value(&address_val).expect("Failed to serialize address");
        assert!(serialized.is_string());
        assert!(serialized.as_str().unwrap().starts_with("0x"));
        assert_eq!(serialized.as_str().unwrap().len(), 42); // 0x + 40 hex chars for address

        let bytes_val = HandlerValue::Bytes(Bytes::from_static(b"hello"));
        let serialized = serde_json::to_value(&bytes_val).expect("Failed to serialize bytes");
        assert!(serialized.is_string());
        assert!(serialized.as_str().unwrap().starts_with("0x"));

        let number_val = HandlerValue::Number(U256::from(42));
        let serialized = serde_json::to_value(&number_val).expect("Failed to serialize number");
        assert!(serialized.is_string());
        assert!(serialized.as_str().unwrap().starts_with("0x"));

        // Test array and object serialization
        let array_val = HandlerValue::Array(vec![
            HandlerValue::String("test".to_string()),
            HandlerValue::Boolean(true),
        ]);
        let serialized = serde_json::to_value(&array_val).expect("Failed to serialize array");
        assert!(serialized.is_array());
        assert_eq!(serialized.as_array().unwrap().len(), 2);

        let mut obj_map = std::collections::HashMap::new();
        obj_map.insert("key1".to_string(), HandlerValue::String("value1".to_string()));
        obj_map.insert("key2".to_string(), HandlerValue::Boolean(false));
        let object_val = HandlerValue::Object(obj_map);
        let serialized = serde_json::to_value(&object_val).expect("Failed to serialize object");
        assert!(serialized.is_object());
        assert_eq!(serialized.as_object().unwrap().len(), 2);
    }

    #[test]
    fn test_convert_storage_value() {
        let handler = StorageHandler::new(
            "test".to_string(),
            StorageSlot {
                slot: SlotValue::Direct(U256::from(0)),
                offset: None,
                return_type: Some(StorageReturnType::Address),
            },
        );

        // Create test storage with a known value
        let mut storage_value = vec![0u8; 32];
        storage_value[31] = 0x42; // Set last byte

        // Test address conversion
        let result = handler.convert_storage_value(&storage_value, &Some(StorageReturnType::Address)).unwrap();
        if let HandlerValue::Address(addr) = result {
            assert_eq!(addr.0[19], 0x42); // Check last byte of address
        } else {
            panic!("Expected Address HandlerValue");
        }

        // Test number conversion
        let result = handler.convert_storage_value(&storage_value, &Some(StorageReturnType::Number)).unwrap();
        if let HandlerValue::Number(num) = result {
            assert_eq!(num, U256::from(0x42));
        } else {
            panic!("Expected Number HandlerValue");
        }

        // Test uint8 conversion
        let result = handler.convert_storage_value(&storage_value, &Some(StorageReturnType::Uint8)).unwrap();
        if let HandlerValue::Number(val) = result {
            assert_eq!(val, U256::from(0x42));
        } else {
            panic!("Expected Number HandlerValue for Uint8");
        }

        // Test bytes conversion
        let result = handler.convert_storage_value(&storage_value, &Some(StorageReturnType::Bytes)).unwrap();
        if let HandlerValue::Bytes(bytes) = result {
            assert_eq!(bytes.len(), 32);
            assert_eq!(bytes[31], 0x42);
        } else {
            panic!("Expected Bytes HandlerValue");
        }

        // Test default conversion (None return type)
        let result = handler.convert_storage_value(&storage_value, &None).unwrap();
        if let HandlerValue::Bytes(bytes) = result {
            assert_eq!(bytes.len(), 32);
            assert_eq!(bytes[31], 0x42);
        } else {
            panic!("Expected Bytes HandlerValue for default conversion");
        }
    }
}
