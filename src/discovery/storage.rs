use async_trait::async_trait;
use std::collections::HashMap;
use alloy_primitives::{Address, U256, keccak256, hex, Bytes};
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
                            // Convert HandlerValue to U256
                            match value {
                                HandlerValue::Number(num) => Ok(*num),
                                HandlerValue::Address(addr) => Ok(U256::from_be_bytes(addr.0.0)),
                                HandlerValue::Uint8(val) => Ok(U256::from(*val)),
                                HandlerValue::Bytes(bytes) => {
                                    if bytes.len() <= 32 {
                                        let mut arr = [0u8; 32];
                                        let start = 32 - bytes.len();
                                        arr[start..].copy_from_slice(&bytes);
                                        Ok(U256::from_be_bytes(arr))
                                    } else {
                                        Err("Bytes too long for U256 conversion".to_string())
                                    }
                                }
                            }
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
                        Ok(HandlerValue::Uint8(storage_value[byte_index]))
                    } else {
                        Err("Offset out of bounds for uint8".to_string())
                    }
                } else {
                    Ok(HandlerValue::Uint8(storage_value[31])) // Last byte
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
        assert_eq!(
            StorageHandler::parse_reference("{{ admin }}").unwrap(),
            "admin"
        );
        assert_eq!(
            StorageHandler::parse_reference("{{owner}}").unwrap(),
            "owner"
        );
        assert!(StorageHandler::parse_reference("admin").is_none());
        assert!(StorageHandler::parse_reference("{{ }}").is_none());
    }

    #[test]
    fn test_handler_value_conversion() {
        // Test JSON conversion for reference resolution
        let json_str = serde_json::Value::String("0x1234".to_string());
        let result = HandlerValue::from_json(&json_str).unwrap();
        assert_eq!(result, U256::from(0x1234));

        let json_num = serde_json::Value::Number(serde_json::Number::from(42));
        let result = HandlerValue::from_json(&json_num).unwrap();
        assert_eq!(result, U256::from(42));

        // Test HandlerValue to JSON conversion
        let addr = Address::from([0x42; 20]);
        let handler_val = HandlerValue::Address(addr);
        let json_val = handler_val.to_json();
        assert!(json_val.as_str().unwrap().starts_with("0x"));
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

        // Test address conversion
        let mut storage_value = vec![0u8; 32];
        storage_value[31] = 0x42; // Set last byte of address

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
        if let HandlerValue::Uint8(val) = result {
            assert_eq!(val, 0x42);
        } else {
            panic!("Expected Uint8 HandlerValue");
        }

        // Test bytes conversion
        let result = handler.convert_storage_value(&storage_value, &Some(StorageReturnType::Bytes)).unwrap();
        if let HandlerValue::Bytes(bytes) = result {
            assert_eq!(bytes.len(), 32);
            assert_eq!(bytes[31], 0x42);
        } else {
            panic!("Expected Bytes HandlerValue");
        }
    }
}
