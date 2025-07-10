use async_trait::async_trait;
use std::collections::HashMap;
use alloy_primitives::{Address, U256, Bytes};
use serde::{Deserialize, Serialize};

/// Contract value type using proper Alloy types for type safety
/// Based on L2Beat's ContractValue but with Alloy types for better type safety
/// Supports arbitrary-length data from eth_call results
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HandlerValue {
    // For references like "{{ admin }}"
    Reference(String),
    // Primitive types
    String(String),
    Number(U256),
    Address(Address),
    Bytes(Bytes),
    Boolean(bool),
    // Complex types for call results that can return structured data
    Array(Vec<HandlerValue>),
    Object(HashMap<String, HandlerValue>),
}


impl HandlerValue {
    /// Convert HandlerValue to U256 for reference resolution
    /// Used when this value is referenced by other handlers
    pub fn try_to_u256(&self) -> Result<U256, String> {
        match self {
            HandlerValue::Number(num) => Ok(*num),
            HandlerValue::Address(addr) => {
                // Convert address to U256 by padding with zeros
                let mut bytes = [0u8; 32];
                bytes[12..].copy_from_slice(addr.as_slice());
                Ok(U256::from_be_bytes(bytes))
            }
            HandlerValue::String(s) => {
                // Try parsing as hex string first, then decimal
                if s.starts_with("0x") {
                    U256::from_str_radix(&s[2..], 16)
                        .map_err(|e| format!("Failed to parse hex string: {}", e))
                } else {
                    U256::from_str_radix(s, 10)
                        .map_err(|e| format!("Failed to parse decimal string: {}", e))
                }
            }
            HandlerValue::Boolean(b) => Ok(U256::from(*b as u64)),
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
            HandlerValue::Array(_) => Err("Cannot convert array to U256".to_string()),
            HandlerValue::Object(_) => Err("Cannot convert object to U256".to_string()),
            HandlerValue::Reference(_) => Err("Cannot convert reference to U256".to_string()),
        }
    }

    /// Helper method to create a HandlerValue from a decoded call result
    /// This would be used when processing eth_call results with ABI decoding
    pub fn from_raw_bytes(result: &[u8]) -> HandlerValue {
        // For now, return as bytes - in a real implementation this would involve ABI decoding
        // The ABI decoder would determine the actual type based on the function signature
        HandlerValue::Bytes(Bytes::copy_from_slice(result))
    }

    /// Helper method to create complex structured values from ABI-decoded results
    pub fn from_json_value(value: serde_json::Value) -> Result<HandlerValue, String> {
        match value {
            serde_json::Value::String(s) => {
                if s.trim().starts_with("{{") && s.trim().ends_with("}}") {
                    Ok(HandlerValue::Reference(s))
                } else {
                    Ok(HandlerValue::String(s))
                }
            },
            serde_json::Value::Number(n) => {
                if let Some(u) = n.as_u64() {
                    Ok(HandlerValue::Number(U256::from(u)))
                } else {
                    Err("Number too large or not an integer".to_string())
                }
            }
            serde_json::Value::Bool(b) => Ok(HandlerValue::Boolean(b)),
            serde_json::Value::Array(arr) => {
                let mut values = Vec::new();
                for item in arr {
                    values.push(Self::from_json_value(item)?);
                }
                Ok(HandlerValue::Array(values))
            }
            serde_json::Value::Object(obj) => {
                let mut map = HashMap::new();
                for (key, value) in obj {
                    map.insert(key, Self::from_json_value(value)?);
                }
                Ok(HandlerValue::Object(map))
            }
            serde_json::Value::Null => Ok(HandlerValue::String("null".to_string())),
        }
    }
}

/// Handler execution result using proper Alloy types
#[derive(Debug, Clone)]
pub struct HandlerResult {
    pub field: String,
    pub value: Option<HandlerValue>,
    pub error: Option<String>,
    pub ignore_relative: Option<bool>,
}

/// Trait for all contract field handlers (storage, call, event, etc.)
#[async_trait]
pub trait Handler: Send + Sync {
    /// The field name this handler is responsible for
    fn field(&self) -> &str;
    /// List of dependency field names (for reference resolution)
    fn dependencies(&self) -> &[String];

    /// Execute the handler, given a provider, contract address, and previous results
    async fn execute(
        &self,
        provider: &(dyn Send + Sync), // TODO: replace with actual provider trait
        address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> HandlerResult;
}

pub fn parse_reference(ref_str: &str) -> Option<String> {
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

/// Resolve call parameter value using previous results
pub fn resolve_reference(
    param: &HandlerValue,
    previous_results: &HashMap<String, HandlerResult>,
) -> Result<HandlerValue, String> {
    match param {
        HandlerValue::Reference(ref_str) => {
            if let Some(field_name) = parse_reference(ref_str) {
                if let Some(result) = previous_results.get(&field_name) {
                    if let Some(value) = &result.value {
                        Ok(value.clone())
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
        HandlerValue::Array(arr) => {
            let mut resolved_array = Vec::new();
            for item in arr {
                resolved_array.push(resolve_reference(item, previous_results)?);
            }
            Ok(HandlerValue::Array(resolved_array))
        }
        HandlerValue::Object(obj) => {
            let mut resolved_object = HashMap::new();
            for (key, value) in obj {
                resolved_object.insert(key.clone(), resolve_reference(value, previous_results)?);
            }
            Ok(HandlerValue::Object(resolved_object))
        }
        _ => {
            // For non-reference values, return as-is
            Ok(param.clone())
        }
    }
}

/// Recursively extract dependencies from slot values
pub fn extract_fields(val: &HandlerValue, deps: &mut Vec<String>) {
    match val {
        HandlerValue::Reference(ref_str) => {
            // Extract field name from reference like "{{ admin }}"
            if let Some(field_name) = parse_reference(ref_str) {
                deps.push(field_name);
            }
        }
        HandlerValue::Array(values) => {
            for value in values {
                extract_fields(value, deps);
            }
        }
        HandlerValue::Object(obj) => {
            for value in obj.values() {
                extract_fields(value, deps);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;


    #[test]
    fn test_try_to_u256() {
        // Test key conversions
        assert_eq!(HandlerValue::Number(U256::from(42)).try_to_u256().unwrap(), U256::from(42));
        assert_eq!(HandlerValue::String("0x2a".to_string()).try_to_u256().unwrap(), U256::from(42));
        assert!(HandlerValue::Address(Address::from([0x42; 20])).try_to_u256().is_ok());
        
        // Test error cases
        assert!(HandlerValue::Reference("{{ admin }}".to_string()).try_to_u256().is_err());
        assert!(HandlerValue::Array(vec![]).try_to_u256().is_err());
    }

    #[test]
    fn test_from_json_value() {
        // Test reference detection vs regular string
        assert_eq!(
            HandlerValue::from_json_value(serde_json::Value::String("{{ admin }}".to_string())).unwrap(),
            HandlerValue::Reference("{{ admin }}".to_string())
        );
        assert_eq!(
            HandlerValue::from_json_value(serde_json::Value::String("test".to_string())).unwrap(),
            HandlerValue::String("test".to_string())
        );
        
        // Test other basic types
        assert_eq!(
            HandlerValue::from_json_value(serde_json::Value::Number(serde_json::Number::from(42))).unwrap(),
            HandlerValue::Number(U256::from(42))
        );
        assert_eq!(
            HandlerValue::from_json_value(serde_json::Value::Bool(true)).unwrap(),
            HandlerValue::Boolean(true)
        );
    }

    #[test]
    fn test_resolve_reference() {
        let mut previous_results = HashMap::new();
        previous_results.insert("admin".to_string(), HandlerResult {
            field: "admin".to_string(),
            value: Some(HandlerValue::Address(Address::from([0x42; 20]))),
            error: None,
            ignore_relative: None,
        });
        
        // Test simple reference resolution
        let admin_ref = HandlerValue::Reference("{{ admin }}".to_string());
        let resolved = resolve_reference(&admin_ref, &previous_results).unwrap();
        assert!(matches!(resolved, HandlerValue::Address(_)));
        
        // Test array with reference
        let array_with_ref = HandlerValue::Array(vec![
            HandlerValue::String("static".to_string()),
            HandlerValue::Reference("{{ admin }}".to_string()),
        ]);
        let resolved = resolve_reference(&array_with_ref, &previous_results).unwrap();
        assert!(matches!(resolved, HandlerValue::Array(_)));
        
        // Test non-reference returns as-is
        let non_ref = HandlerValue::Number(U256::from(999));
        let resolved = resolve_reference(&non_ref, &previous_results).unwrap();
        assert_eq!(resolved, HandlerValue::Number(U256::from(999)));
        
        // Test error case
        let invalid_ref = HandlerValue::Reference("{{ nonexistent }}".to_string());
        assert!(resolve_reference(&invalid_ref, &previous_results).is_err());
    }
}