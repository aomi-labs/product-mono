use async_trait::async_trait;
use std::collections::HashMap;
use alloy_primitives::{Address, U256, keccak256, Bytes};
use serde::{Deserialize, Serialize};

use crate::discovery::handler::{parse_reference, Handler, HandlerResult, HandlerValue};
use crate::discovery::config::HandlerDefinition;



/// Storage slot configuration, similar to L2Beat's StorageHandler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSlot {
    pub slot: SlotValue,
    pub offset: Option<u32>,
    pub return_type: Option<String>, // Will be parsed to determine HandlerValue type
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
        Self {
            field,
            dependencies: Self::extract_dependencies(&slot),
            slot,
        }
    }

    /// Create StorageHandler from HandlerDefinition::Storage
    pub fn from_handler_definition(field: String, handler: HandlerDefinition) -> Result<Self, String> {
        match handler {
            HandlerDefinition::Storage { slot, offset, return_type } => {
                let slot_val = slot
                    .map(Self::convert_slot_value)
                    .unwrap_or_else(|| Err("Storage slot is required".to_string()))?;

                let storage_slot = StorageSlot {
                    slot: slot_val,
                    offset: offset.map(|o| o as u32),
                    return_type,
                };
                Ok(Self::new(field, storage_slot))
            }
            _ => Err("Handler definition is not a storage handler".to_string()),
        }
    }

    /// Convert serde_json::Value to SlotValue
    fn convert_slot_value(value: serde_json::Value) -> Result<SlotValue, String> {
        match value {
            serde_json::Value::Number(n) => {
                if let Some(u) = n.as_u64() {
                    Ok(SlotValue::Direct(U256::from(u)))
                } else {
                    Err("Invalid slot number".to_string())
                }
            }
            serde_json::Value::String(s) => {
                // Check if it's a reference like "{{ admin }}"
                if s.trim().starts_with("{{") && s.trim().ends_with("}}") {
                    Ok(SlotValue::Reference(s))
                } else {
                    // Try to parse as hex or decimal
                    if s.starts_with("0x") {
                        U256::from_str_radix(&s[2..], 16)
                            .map(SlotValue::Direct)
                            .map_err(|e| format!("Failed to parse hex slot: {}", e))
                    } else {
                        U256::from_str_radix(&s, 10)
                            .map(SlotValue::Direct)
                            .map_err(|e| format!("Failed to parse decimal slot: {}", e))
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                let mut slot_values = Vec::new();
                for item in arr {
                    slot_values.push(Self::convert_slot_value(item)?);
                }
                Ok(SlotValue::Array(slot_values))
            }
            _ => Err("Invalid slot value type".to_string()),
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
                if let Some(field_name) = parse_reference(ref_str) {
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

    /// Compute storage slot based on configuration and previous results
    fn compute_slot(
        &self,
        slot_value: &SlotValue,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<U256, String> {
        let slot =self.resolve_slot_value(slot_value, previous_results)?;
        let offset = self.slot.offset.unwrap_or(0) as u64;
        Ok(slot + U256::from(offset))
    }

    /// Resolve a slot value to U256 without adding offset (used internally)
    fn resolve_slot_value(
        &self,
        slot_value: &SlotValue,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<U256, String> {
        match slot_value {
            SlotValue::Direct(slot) => Ok(*slot),
            SlotValue::Array(values) => {
                let mut resolved_values = Vec::new();
                for value in values {
                    let computed = self.resolve_slot_value(value, previous_results)?;
                    resolved_values.push(computed);
                }
                self.compute_mapping_slot(resolved_values)
            }
            SlotValue::Reference(ref_str) => {
                let resolved_ref = parse_reference(ref_str)
                    .ok_or(format!("Invalid reference format: {}", ref_str))?;
                let handler_result = previous_results.get(&resolved_ref)
                    .ok_or(format!("Reference '{}' not found in previous results", resolved_ref))?;
                
                if let Some(value) = &handler_result.value {
                    value.to_u256()
                        .map_err(|e| format!("Failed to convert reference '{}' to U256: {}", handler_result.field, e))
                } else {
                    Err(format!("Reference '{}' has no value", handler_result.field))
                }
            }
        }
    }

    /// Compute mapping slot
    /// For [10, 1, 2], mapping(uint => mapping(uint => uint))
    /// keccak256(abi.encodePacked(2, keccak256(abi.encodePacked(1, 10))))
    fn compute_mapping_slot(&self, mut parts: Vec<U256>) -> Result<U256, String> {
        // While we have 3 or more parts, hash pairs from the front
        while parts.len() >= 2 {
            let a = parts.remove(0); 
            let b = parts.remove(0); 
            let data = [b.to_be_bytes::<32>(), a.to_be_bytes::<32>()].concat();
            parts.insert(0, keccak256(&data).into()); 
        }
        
        let slot = parts.remove(0);
        let offset = self.slot.offset.unwrap_or(0) as u64;
        Ok(slot + U256::from(offset))
    }


    /// Convert storage value to proper HandlerValue based on return type
    fn convert_storage_return(
        &self,
        storage_value: &[u8],
        return_type: &Option<String>,
    ) -> Result<HandlerValue, String> {
        // Ensure we have 32 bytes for storage values
        if storage_value.len() != 32 {
            return Err(format!("Invalid storage value length: expected 32 bytes, got {}", storage_value.len()));
        }

        let return_type_str = return_type.as_ref().map(|s| s.to_lowercase());
        match return_type_str.as_ref().map(|s| s.as_str()) {
            Some("address") => {
                let address = Address::from_slice(&storage_value[12..32]);
                Ok(HandlerValue::Address(address))
            }
            Some("bytes") | None => {
                Ok(HandlerValue::Bytes(Bytes::copy_from_slice(storage_value)))
            }
            Some("number") => {
                let u256_val = U256::from_be_bytes::<32>(storage_value.try_into().unwrap());
                Ok(HandlerValue::Number(u256_val))
            }
            Some("string") => {
                // Convert bytes to string, removing null bytes
                let string_bytes: Vec<u8> = storage_value.iter().copied().take_while(|&b| b != 0).collect();
                let string_val = String::from_utf8(string_bytes)
                    .map_err(|e| format!("Invalid UTF-8 in storage value: {}", e))?;
                Ok(HandlerValue::String(string_val))
            }
            Some("boolean") => {
                // Consider non-zero as true
                let is_true = storage_value.iter().any(|&b| b != 0);
                Ok(HandlerValue::Boolean(is_true))
            }
            Some("uint8") => {
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
            Some(unknown) => {
                Err(format!("Unknown return type: {}", unknown))
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
                match self.convert_storage_return(&storage_value, &self.slot.return_type) {
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
    use crate::discovery::config::parse_config_file;
    use std::path::Path;

    #[test]
    fn test_storage_handler_creation() {
        let slot = StorageSlot {
            slot: SlotValue::Direct(U256::from(0)),
            offset: None,
            return_type: Some("address".to_string()),
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
            return_type: Some("address".to_string()),
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
            return_type: Some("number".to_string()),
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
                parse_reference(input).unwrap(),
                expected,
                "Failed to parse valid reference: {}",
                input
            );
        }

        // Test invalid references
        for invalid_ref in test_cases["invalid"].as_array().unwrap() {
            let input = invalid_ref.as_str().unwrap();
            assert!(
                parse_reference(input).is_none(),
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
                return_type: Some("address".to_string()),
            },
        );

        // Create test storage with a known value
        let mut storage_value = vec![0u8; 32];
        storage_value[31] = 0x42; // Set last byte

        // Test address conversion
        let result = handler.convert_storage_return(&storage_value, &Some("address".to_string())).unwrap();
        if let HandlerValue::Address(addr) = result {
            assert_eq!(addr.0[19], 0x42); // Check last byte of address
        } else {
            panic!("Expected Address HandlerValue");
        }

        // Test number conversion
        let result = handler.convert_storage_return(&storage_value, &Some("number".to_string())).unwrap();
        if let HandlerValue::Number(num) = result {
            assert_eq!(num, U256::from(0x42));
        } else {
            panic!("Expected Number HandlerValue");
        }

        // Test uint8 conversion
        let result = handler.convert_storage_return(&storage_value, &Some("uint8".to_string())).unwrap();
        if let HandlerValue::Number(val) = result {
            assert_eq!(val, U256::from(0x42));
        } else {
            panic!("Expected Number HandlerValue for Uint8");
        }

        // Test bytes conversion
        let result = handler.convert_storage_return(&storage_value, &Some("bytes".to_string())).unwrap();
        if let HandlerValue::Bytes(bytes) = result {
            assert_eq!(bytes.len(), 32);
            assert_eq!(bytes[31], 0x42);
        } else {
            panic!("Expected Bytes HandlerValue");
        }

        // Test string conversion
        let mut string_storage = vec![0u8; 32];
        string_storage[0] = b'H';
        string_storage[1] = b'i';
        let result = handler.convert_storage_return(&string_storage, &Some("string".to_string())).unwrap();
        if let HandlerValue::String(s) = result {
            assert_eq!(s, "Hi");
        } else {
            panic!("Expected String HandlerValue");
        }

        // Test boolean conversion
        let result = handler.convert_storage_return(&storage_value, &Some("boolean".to_string())).unwrap();
        if let HandlerValue::Boolean(b) = result {
            assert_eq!(b, true); // Non-zero should be true
        } else {
            panic!("Expected Boolean HandlerValue");
        }

        // Test default conversion (None return type)
        let result = handler.convert_storage_return(&storage_value, &None).unwrap();
        if let HandlerValue::Bytes(bytes) = result {
            assert_eq!(bytes.len(), 32);
            assert_eq!(bytes[31], 0x42);
        } else {
            panic!("Expected Bytes HandlerValue for default conversion");
        }
    }

    #[test]
    fn test_handler_definition_conversion() {
        // Test conversion from HandlerDefinition::Storage to StorageSlot
        let handler_def = HandlerDefinition::Storage {
            slot: Some(serde_json::Value::Number(serde_json::Number::from(3))),
            offset: None,
            return_type: Some("address".to_string()),
        };

        let storage_handler = StorageHandler::from_handler_definition("test_field".to_string(), handler_def).unwrap();
        
        assert_eq!(storage_handler.field(), "test_field");
        assert_eq!(storage_handler.dependencies().len(), 0);
        
        // Check the slot configuration
        match &storage_handler.slot.slot {
            SlotValue::Direct(slot) => assert_eq!(*slot, U256::from(3)),
            _ => panic!("Expected Direct slot value"),
        }
        
        assert_eq!(storage_handler.slot.offset, None);
        assert_eq!(storage_handler.slot.return_type, Some("address".to_string()));
    }

    #[test]
    fn test_convert_slot_value_types() {
        // Test number slot
        let number_slot = serde_json::Value::Number(serde_json::Number::from(42));
        let result = StorageHandler::convert_slot_value(number_slot).unwrap();
        assert!(matches!(result, SlotValue::Direct(slot) if slot == U256::from(42)));

        // Test hex string slot
        let hex_slot = serde_json::Value::String("0x2a".to_string());
        let result = StorageHandler::convert_slot_value(hex_slot).unwrap();
        assert!(matches!(result, SlotValue::Direct(slot) if slot == U256::from(42)));

        // Test reference slot
        let ref_slot = serde_json::Value::String("{{ admin }}".to_string());
        let result = StorageHandler::convert_slot_value(ref_slot).unwrap();
        assert!(matches!(result, SlotValue::Reference(ref_str) if ref_str == "{{ admin }}"));

        // Test array slot
        let array_slot = serde_json::Value::Array(vec![
            serde_json::Value::Number(serde_json::Number::from(1)),
            serde_json::Value::String("{{ token }}".to_string()),
        ]);
        let result = StorageHandler::convert_slot_value(array_slot).unwrap();
        if let SlotValue::Array(values) = result {
            assert_eq!(values.len(), 2);
            assert!(matches!(values[0], SlotValue::Direct(slot) if slot == U256::from(1)));
            assert!(matches!(values[1], SlotValue::Reference(ref ref_str) if ref_str == "{{ token }}"));
        } else {
            panic!("Expected Array slot value");
        }
    }

    #[test]
    fn test_e2e_sharp_verifier_template() {
        // Test E2E parsing of the actual SHARPVerifier template
        let template_path = Path::new("/Users/ceciliazhang/Code/forge-mcp/src/discovery/projects/_templates/shared-sharp-verifier/SHARPVerifier/template.jsonc");
        
        let contract_config = parse_config_file(template_path).expect("Failed to parse template file");
        
        // Test bootloaderProgramContractAddress field
        let fields = contract_config.fields.expect("Contract should have fields");
        let bootloader_field = fields.get("bootloaderProgramContractAddress")
            .expect("bootloaderProgramContractAddress field not found");
        
        let handler_def = bootloader_field.handler.as_ref()
            .expect("Handler definition not found");
        
        let storage_handler = StorageHandler::from_handler_definition(
            "bootloaderProgramContractAddress".to_string(), 
            handler_def.clone()
        ).expect("Failed to create StorageHandler");
        
        // Verify the handler configuration
        assert_eq!(storage_handler.field(), "bootloaderProgramContractAddress");
        assert_eq!(storage_handler.dependencies().len(), 0);
        
        // Check slot configuration
        match &storage_handler.slot.slot {
            SlotValue::Direct(slot) => assert_eq!(*slot, U256::from(3)),
            _ => panic!("Expected Direct slot value"),
        }
        
        assert_eq!(storage_handler.slot.offset, None);
        assert_eq!(storage_handler.slot.return_type, Some("address".to_string()));
        
        // Test memoryPageFactRegistry field
        let memory_field = fields.get("memoryPageFactRegistry")
            .expect("memoryPageFactRegistry field not found");
        
        let handler_def = memory_field.handler.as_ref()
            .expect("Handler definition not found");
        
        let storage_handler = StorageHandler::from_handler_definition(
            "memoryPageFactRegistry".to_string(), 
            handler_def.clone()
        ).expect("Failed to create StorageHandler");
        
        // Verify the handler configuration
        assert_eq!(storage_handler.field(), "memoryPageFactRegistry");
        assert_eq!(storage_handler.dependencies().len(), 0);
        
        // Check slot configuration
        match &storage_handler.slot.slot {
            SlotValue::Direct(slot) => assert_eq!(*slot, U256::from(4)),
            _ => panic!("Expected Direct slot value"),
        }
        
        assert_eq!(storage_handler.slot.offset, None);
        assert_eq!(storage_handler.slot.return_type, Some("address".to_string()));
        
        // Test that we can handle other handler types gracefully
        let cpu_field = fields.get("cpuFrilessVerifiers")
            .expect("cpuFrilessVerifiers field not found");
        
        let handler_def = cpu_field.handler.as_ref()
            .expect("Handler definition not found");
        
        // This should fail because it's not a storage handler
        let result = StorageHandler::from_handler_definition(
            "cpuFrilessVerifiers".to_string(), 
            handler_def.clone()
        );
        
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Handler definition is not a storage handler"));
    }

    #[test]
    fn test_mapping_slot_computation() {
        // Test L2Beat's expected behavior: Read the `[1][2]` value of a `mapping(uint => mapping(uint => uint))` at slot `10`
        // Expected slot array: [10, 1, 2]
        
        let handler = StorageHandler::new(
            "mapping_test".to_string(),
            StorageSlot {
                slot: SlotValue::Array(vec![
                    SlotValue::Direct(U256::from(10)),
                    SlotValue::Direct(U256::from(1)),
                    SlotValue::Direct(U256::from(2)),
                ]),
                offset: None,
                return_type: Some("number".to_string()),
            },
        );

        let previous_results = HashMap::new();
        let computed_slot = handler.compute_slot(&handler.slot.slot, &previous_results).unwrap();
        
        // Verify the computation matches L2Beat's algorithm:
        // 1. parts = [10, 1, 2]
        // 2. While parts.length >= 3: a=10, b=1, hash([1, 10]) -> parts = [hash([1, 10]), 2]
        // 3. parts.length < 3, so reverse: [2, hash([1, 10])]
        // 4. Final hash = hash([2, hash([1, 10])])
        
        // Let's manually compute what we expect:
        // First hash: keccak256(abi.encode(1, 10))
        let first_hash = {
            let mut data = Vec::new();
            data.extend_from_slice(&U256::from(1).to_be_bytes::<32>());
            data.extend_from_slice(&U256::from(10).to_be_bytes::<32>());
            U256::from_be_bytes(keccak256(&data).0)
        };
        
        // Final hash: keccak256(abi.encode(2, first_hash))
        let expected_slot = {
            let mut data = Vec::new();
            data.extend_from_slice(&U256::from(2).to_be_bytes::<32>());
            data.extend_from_slice(&first_hash.to_be_bytes::<32>());
            U256::from_be_bytes(keccak256(&data).0)
        };
        
        assert_eq!(computed_slot, expected_slot);
    }

    #[test]
    fn test_simple_mapping_slot() {
        // Test simple mapping: mapping[key] at slot
        // Expected: hash([key, slot])
        
        let handler = StorageHandler::new(
            "simple_mapping".to_string(),
            StorageSlot {
                slot: SlotValue::Array(vec![
                    SlotValue::Direct(U256::from(5)), // slot
                    SlotValue::Direct(U256::from(123)), // key
                ]),
                offset: None,
                return_type: Some("number".to_string()),
            },
        );

        let previous_results = HashMap::new();
        let computed_slot = handler.compute_slot(&handler.slot.slot, &previous_results).unwrap();
        
        // Expected: hash([123, 5]) since L2Beat reverses before final hash
        let expected_slot = {
            let mut data = Vec::new();
            data.extend_from_slice(&U256::from(123).to_be_bytes::<32>());
            data.extend_from_slice(&U256::from(5).to_be_bytes::<32>());
            U256::from_be_bytes(keccak256(&data).0)
        };
        
        assert_eq!(computed_slot, expected_slot);
    }

    #[test]
    fn test_slot_with_offset() {
        // Test slot computation with offset
        let handler = StorageHandler::new(
            "offset_test".to_string(),
            StorageSlot {
                slot: SlotValue::Direct(U256::from(10)),
                offset: Some(5),
                return_type: Some("number".to_string()),
            },
        );

        let previous_results = HashMap::new();
        let computed_slot = handler.compute_slot(&handler.slot.slot, &previous_results).unwrap();
        
        // Expected: 10 + 5 = 15
        assert_eq!(computed_slot, U256::from(15));
    }
}
