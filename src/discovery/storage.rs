use alloy_primitives::{Address, Bytes, U256, keccak256};
use alloy_provider::{
    Provider, RootProvider,
    network::{AnyNetwork, Network},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::discovery::config::HandlerDefinition;
use crate::discovery::handler::{
    Handler, HandlerResult, HandlerValue, extract_fields, parse_reference, resolve_reference,
};

/// Type alias for StorageHandler with AnyNetwork for convenience
pub type AnyStorageHandler = StorageHandler<AnyNetwork>;

/// Storage slot configuration, similar to L2Beat's StorageHandler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSlot {
    pub slot: HandlerValue,
    pub offset: Option<u32>,
    pub return_type: Option<String>, // Will be parsed to determine HandlerValue type
}

impl StorageSlot {
    /// Compute storage slot based on configuration and previous results
    pub fn resolve(&self, previous_results: &HashMap<String, HandlerResult>) -> Result<U256, String> {
        let slot_value = &self.slot;
        let slot = Self::compute_resolve(slot_value, previous_results)?;
        let offset = self.offset.unwrap_or(0) as u64;
        Ok(slot + U256::from(offset))
    }

    /// Resolve a slot value to U256 without adding offset (used internally)
    pub fn compute_resolve(
        slot_value: &HandlerValue,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<U256, String> {
        match slot_value {
            HandlerValue::Number(slot) => Ok(*slot),
            HandlerValue::Array(values) => {
                let mut resolved_values = Vec::new();
                for value in values {
                    let computed = Self::compute_resolve(value, previous_results)?;
                    resolved_values.push(computed);
                }
                compute_nested_mapping(resolved_values)
            }
            HandlerValue::Reference(_) => {
                // Use the general reference resolution function
                let resolved_reference = resolve_reference(slot_value, previous_results)?;
                Self::compute_resolve(&resolved_reference, previous_results)
            }
            // Convert other HandlerValue types to U256 for slot computation
            other => other.try_to_u256().map_err(|e| format!("Failed to convert slot value to U256: {}", e)),
        }
    }

    pub async fn get_value<N: Network>(
        &self,
        previous_results: &HashMap<String, HandlerResult>,
        provider: &RootProvider<N>,
        address: &Address,
    ) -> Result<U256, String> {
        let slot = self.resolve(previous_results)?;
        provider.get_storage_at(*address, slot).await.map_err(|e| format!("Failed to read storage: {}", e))
    }

    pub async fn get_resolved_value<N: Network>(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
    ) -> Result<U256, String> {
        self.get_value(&HashMap::new(), provider, address).await
    }

    pub fn convert_return(&self, storage_value: U256) -> Result<HandlerValue, String> {
        let bytes: [u8; 32] = storage_value.to_be_bytes();
        let return_type = self.return_type.as_ref().map(|s| s.to_lowercase());
        match return_type.as_ref().map(|s| s.as_str()) {
            Some("address") => {
                let address = Address::from_slice(&bytes[12..32]);
                Ok(HandlerValue::Address(address))
            }
            Some("bytes") | None => Ok(HandlerValue::Bytes(Bytes::copy_from_slice(&bytes))),
            Some("number") => {
                let u256_val = U256::from_be_bytes::<32>(bytes.try_into().unwrap());
                Ok(HandlerValue::Number(u256_val))
            }
            Some("string") => {
                // Convert bytes to string, removing null bytes
                let string_bytes: Vec<u8> = bytes.iter().copied().take_while(|&b| b != 0).collect();
                let string_val =
                    String::from_utf8(string_bytes).map_err(|e| format!("Invalid UTF-8 in storage value: {}", e))?;
                Ok(HandlerValue::String(string_val))
            }
            Some("boolean") => {
                // Consider non-zero as true
                let is_true = bytes.iter().any(|&b| b != 0);
                Ok(HandlerValue::Boolean(is_true))
            }
            Some("uint8") => {
                if let Some(offset) = self.offset {
                    let byte_index = offset as usize;
                    if byte_index < bytes.len() {
                        Ok(HandlerValue::Number(U256::from(bytes[byte_index])))
                    } else {
                        Err("Offset out of bounds for uint8".to_string())
                    }
                } else {
                    Ok(HandlerValue::Number(U256::from(bytes[31]))) // Last byte
                }
            }
            Some(unknown) => Err(format!("Unknown return type: {}", unknown)),
        }
    }
}

/// Storage handler implementation mimicking L2Beat's StorageHandler
#[derive(Debug, Clone)]
pub struct StorageHandler<N> {
    pub field: String,
    pub dependencies: Vec<String>,
    pub slot: StorageSlot,
    pub hidden: bool,
    _phantom: std::marker::PhantomData<N>,
}

impl<N> StorageHandler<N> {
    pub fn new(field: String, slot: StorageSlot, hidden: bool) -> Self {
        let mut handler = Self {
            field,
            dependencies: Vec::new(),
            slot,
            hidden,
            _phantom: std::marker::PhantomData,
        };
        handler.dependencies = handler.resolve_dependencies();
        handler
    }

    /// Create StorageHandler from HandlerDefinition::Storage
    pub fn from_handler_definition(field: String, handler: HandlerDefinition) -> Result<Self, String> {
        match handler {
            HandlerDefinition::Storage {
                slot,
                offset,
                return_type,
                ignore_relative,
            } => {
                let slot_val = slot
                    .map(HandlerValue::from_json_value)
                    .unwrap_or_else(|| Err("Storage slot is required".to_string()))?;

                let storage_slot = StorageSlot {
                    slot: slot_val,
                    offset: offset.map(|o| o as u32),
                    return_type,
                };
                Ok(Self::new(field, storage_slot, ignore_relative.unwrap_or(false)))
            }
            _ => Err("Handler definition is not a storage handler".to_string()),
        }
    }

    /// Recursively extract dependencies from slot values
    pub fn resolve_dependencies(&self) -> Vec<String> {
        let mut dependencies = Vec::new();
        extract_fields(&self.slot.slot, &mut dependencies);
        dependencies
    }

    /// Compute storage slot based on configuration and previous results
    pub fn resolve_slot(&self, previous_results: &HashMap<String, HandlerResult>) -> Result<U256, String> {
        self.slot.resolve(previous_results)
    }
}

/// Compute mapping slot
/// For [10, 1, 2], mapping(uint => mapping(uint => uint))
/// keccak256(abi.encodePacked(2, keccak256(abi.encodePacked(1, 10))))
fn compute_nested_mapping(mut parts: Vec<U256>) -> Result<U256, String> {
    // While we have 3 or more parts, hash pairs from the front
    while parts.len() >= 2 {
        let a = parts.remove(0);
        let b = parts.remove(0);
        let data = [b.to_be_bytes::<32>(), a.to_be_bytes::<32>()].concat();
        parts.insert(0, keccak256(&data).into());
    }

    Ok(parts.remove(0))
}

#[async_trait]
impl<N: Network> Handler<N> for StorageHandler<N> {
    fn field(&self) -> &str {
        &self.field
    }

    fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    fn hidden(&self) -> bool {
        self.hidden
    }

    async fn execute(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> HandlerResult {
        // Resolve and Read storage from the provider
        let initial_slot = &self.slot;
        if let Ok(storage_value) = initial_slot.get_value(previous_results, provider, address).await {
            let res = initial_slot.convert_return(storage_value);
            return match res {
                Ok(converted_value) => HandlerResult {
                    field: self.field.clone(),
                    value: Some(converted_value),
                    error: None,
                    hidden: self.hidden,
                },
                Err(error) => HandlerResult {
                    field: self.field.clone(),
                    value: None,
                    error: Some(format!("Failed to convert storage value: {}", error)),
                    hidden: self.hidden,
                },
            };
        } else {
            return HandlerResult {
                field: self.field.clone(),
                value: None,
                error: Some(format!("Failed to read storage: {:?}", initial_slot.slot)),
                hidden: self.hidden,
            };
        }
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
            slot: HandlerValue::Number(U256::from(0)),
            offset: None,
            return_type: Some("address".to_string()),
        };

        let handler = AnyStorageHandler::new("admin".to_string(), slot, false);
        assert_eq!(handler.field(), "admin");
        assert_eq!(handler.dependencies().len(), 0);
    }

    #[test]
    fn test_nested_array_slot_dependencies() {
        // Test array containing nested arrays with references
        let nested_arrays = HandlerValue::Array(vec![
            HandlerValue::Reference("{{ mainSlot }}".to_string()),
            HandlerValue::Array(vec![
                HandlerValue::Reference("{{ subKey1 }}".to_string()),
                HandlerValue::Array(vec![
                    HandlerValue::Reference("{{ deepKey }}".to_string()),
                    HandlerValue::Number(U256::from(42)),
                ]),
                HandlerValue::Reference("{{ subKey2 }}".to_string()),
            ]),
            HandlerValue::Number(U256::from(100)),
        ]);

        let nested_slot = StorageSlot {
            slot: nested_arrays,
            offset: Some(1),
            return_type: Some("bytes".to_string()),
        };

        let nested_handler = AnyStorageHandler::new("nestedMapping".to_string(), nested_slot, false);
        assert_eq!(nested_handler.dependencies().len(), 4);
        let nested_deps = &nested_handler.dependencies();
        assert!(nested_deps.contains(&"mainSlot".to_string()));
        assert!(nested_deps.contains(&"subKey1".to_string()));
        assert!(nested_deps.contains(&"deepKey".to_string()));
        assert!(nested_deps.contains(&"subKey2".to_string()));
    }

    #[test]
    fn test_parse_reference() {
        // Valid references
        assert_eq!(parse_reference("{{ admin }}"), Some("admin".to_string()));
        assert_eq!(parse_reference("{{owner}}"), Some("owner".to_string()));
        assert_eq!(parse_reference("{{ balance_slot }}"), Some("balance_slot".to_string()));

        // Invalid references
        assert_eq!(parse_reference("admin"), None);
        assert_eq!(parse_reference("{{ }}"), None);
        assert_eq!(parse_reference(""), None);
        assert_eq!(parse_reference("{ admin }"), None);
        assert_eq!(parse_reference("{{}}"), None);
    }

    #[test]
    fn test_convert_storage_return() {
        let mut slot = StorageSlot {
            slot: HandlerValue::Number(U256::from(0)),
            offset: None,
            return_type: Some("address".to_string()),
        };

        // Test storage with value 0x42 in last byte
        let mut storage_bytes = [0u8; 32];
        storage_bytes[31] = 0x42;
        let storage_value = U256::from_be_bytes(storage_bytes);

        // Test address conversion
        let result = slot.convert_return(storage_value).unwrap();
        assert!(matches!(result, HandlerValue::Address(_)));

        // Test number conversion
        slot.return_type = Some("number".to_string());
        let result = slot.convert_return(storage_value).unwrap();
        assert_eq!(result, HandlerValue::Number(U256::from(0x42)));

        // Test default (bytes) conversion
        slot.return_type = None;
        let result = slot.convert_return(storage_value).unwrap();
        assert!(matches!(result, HandlerValue::Bytes(_)));
    }

    #[test]
    fn test_handler_definition_conversion() {
        // Test conversion from HandlerDefinition::Storage to StorageSlot
        let handler_def = HandlerDefinition::Storage {
            slot: Some(serde_json::Value::Number(serde_json::Number::from(3))),
            offset: None,
            return_type: Some("address".to_string()),
            ignore_relative: None,
        };

        let storage_handler =
            AnyStorageHandler::from_handler_definition("test_field".to_string(), handler_def).unwrap();

        assert_eq!(storage_handler.field(), "test_field");
        assert_eq!(storage_handler.dependencies().len(), 0);

        // Check the slot configuration
        match &storage_handler.slot.slot {
            HandlerValue::Number(slot) => assert_eq!(*slot, U256::from(3)),
            _ => panic!("Expected Direct slot value"),
        }

        assert_eq!(storage_handler.slot.offset, None);
        assert_eq!(storage_handler.slot.return_type, Some("address".to_string()));
    }

    #[test]
    fn test_e2e_sharp_verifier_template() {
        // Test E2E parsing of the actual SHARPVerifier template
        let template_path =
            Path::new("src/discovery/projects/_templates/shared-sharp-verifier/SHARPVerifier/template.jsonc");
        let contract_config = parse_config_file(template_path).expect("Failed to parse template file");

        // Test bootloaderProgramContractAddress field
        let fields = contract_config.fields.expect("Contract should have fields");
        let bootloader_field =
            fields.get("bootloaderProgramContractAddress").expect("bootloaderProgramContractAddress field not found");

        let handler_def = bootloader_field.handler.as_ref().expect("Handler definition not found");

        let storage_handler = AnyStorageHandler::from_handler_definition(
            "bootloaderProgramContractAddress".to_string(),
            handler_def.clone(),
        )
        .expect("Failed to create StorageHandler");

        // Verify the handler configuration
        assert_eq!(storage_handler.field(), "bootloaderProgramContractAddress");
        assert_eq!(storage_handler.dependencies().len(), 0);

        // Check slot configuration
        match &storage_handler.slot.slot {
            HandlerValue::Number(slot) => assert_eq!(*slot, U256::from(3)),
            _ => panic!("Expected Direct slot value"),
        }

        assert_eq!(storage_handler.slot.offset, None);
        assert_eq!(storage_handler.slot.return_type, Some("address".to_string()));

        // Test memoryPageFactRegistry field
        let memory_field = fields.get("memoryPageFactRegistry").expect("memoryPageFactRegistry field not found");

        let handler_def = memory_field.handler.as_ref().expect("Handler definition not found");

        let storage_handler =
            AnyStorageHandler::from_handler_definition("memoryPageFactRegistry".to_string(), handler_def.clone())
                .expect("Failed to create StorageHandler");

        // Verify the handler configuration
        assert_eq!(storage_handler.field(), "memoryPageFactRegistry");
        assert_eq!(storage_handler.dependencies().len(), 0);

        // Check slot configuration
        match &storage_handler.slot.slot {
            HandlerValue::Number(slot) => assert_eq!(*slot, U256::from(4)),
            _ => panic!("Expected Direct slot value"),
        }

        assert_eq!(storage_handler.slot.offset, None);
        assert_eq!(storage_handler.slot.return_type, Some("address".to_string()));

        // Test that we can handle other handler types gracefully
        let cpu_field = fields.get("cpuFrilessVerifiers").expect("cpuFrilessVerifiers field not found");

        let handler_def = cpu_field.handler.as_ref().expect("Handler definition not found");

        // This should fail because it's not a storage handler
        let result = AnyStorageHandler::from_handler_definition("cpuFrilessVerifiers".to_string(), handler_def.clone());

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Handler definition is not a storage handler"));
    }

    #[test]
    fn test_mapping_slot_computation() {
        // Test L2Beat's expected behavior: Read the `[1][2]` value of a `mapping(uint => mapping(uint => uint))` at slot `10`
        // Expected slot array: [10, 1, 2]

        let handler = AnyStorageHandler::new(
            "mapping_test".to_string(),
            StorageSlot {
                slot: HandlerValue::Array(vec![
                    HandlerValue::Number(U256::from(10)),
                    HandlerValue::Number(U256::from(1)),
                    HandlerValue::Number(U256::from(2)),
                ]),
                offset: None,
                return_type: Some("number".to_string()),
            },
            false,
        );

        let previous_results = HashMap::new();
        let computed_slot = handler.resolve_slot(&previous_results).unwrap();

        // 1. parts = [10, 1, 2]
        // 2. While parts.length >= 3: a=10, b=1, hash([1, 10]) -> parts = [hash([1, 10]), 2]
        // 3. parts.length < 3, so reverse: [2, hash([1, 10])]
        // 4. Final hash = hash([2, hash([1, 10])])

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
    fn test_nested_slot_with_offset() {
        // Test nested mapping: mapping(baseSlot => mapping(user => balance)) with offset
        let nested_slot = HandlerValue::Array(vec![
            HandlerValue::Reference("{{ baseSlot }}".to_string()),
            HandlerValue::Reference("{{ userAddress }}".to_string()),
        ]);

        let handler = AnyStorageHandler::new(
            "userBalance".to_string(),
            StorageSlot {
                slot: nested_slot,
                offset: Some(8),
                return_type: Some("number".to_string()),
            },
            false,
        );

        // Verify dependencies extracted
        assert_eq!(handler.dependencies().len(), 2);

        // Create previous results to resolve dependencies
        let mut previous_results = HashMap::new();
        previous_results.insert(
            "baseSlot".to_string(),
            HandlerResult {
                field: "baseSlot".to_string(),
                value: Some(HandlerValue::Number(U256::from(5))),
                error: None,
                hidden: handler.hidden,
            },
        );
        previous_results.insert(
            "userAddress".to_string(),
            HandlerResult {
                field: "userAddress".to_string(),
                value: Some(HandlerValue::Address(Address::from([0x42; 20]))),
                error: None,
                hidden: handler.hidden,
            },
        );

        // Compute slot and verify it includes offset
        let computed_slot = handler.resolve_slot(&previous_results).unwrap();

        // Should be keccak256(userAddress, baseSlot) + offset
        let user_addr_u256 = {
            let mut bytes = [0u8; 32];
            bytes[12..].copy_from_slice(&[0x42; 20]);
            U256::from_be_bytes(bytes)
        };

        let expected_base = {
            let mut data = Vec::new();
            data.extend_from_slice(&user_addr_u256.to_be_bytes::<32>());
            data.extend_from_slice(&U256::from(5).to_be_bytes::<32>());
            U256::from_be_bytes(keccak256(&data).0)
        };

        assert_eq!(computed_slot, expected_base + U256::from(8));
    }
}
