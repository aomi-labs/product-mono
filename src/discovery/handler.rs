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
    pub fn to_u256(&self) -> Result<U256, String> {
        match self {
            HandlerValue::Number(num) => Ok(*num),
            HandlerValue::Address(addr) => Ok(U256::from_be_bytes(addr.0.0)),
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
        }
    }

    /// Helper method to create a HandlerValue from a decoded call result
    /// This would be used when processing eth_call results with ABI decoding
    pub fn from_call_result(result: &[u8]) -> HandlerValue {
        // For now, return as bytes - in a real implementation this would involve ABI decoding
        // The ABI decoder would determine the actual type based on the function signature
        HandlerValue::Bytes(Bytes::copy_from_slice(result))
    }

    /// Helper method to create complex structured values from ABI-decoded results
    pub fn from_decoded_result(value: serde_json::Value) -> Result<HandlerValue, String> {
        match value {
            serde_json::Value::String(s) => Ok(HandlerValue::String(s)),
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
                    values.push(Self::from_decoded_result(item)?);
                }
                Ok(HandlerValue::Array(values))
            }
            serde_json::Value::Object(obj) => {
                let mut map = HashMap::new();
                for (key, value) in obj {
                    map.insert(key, Self::from_decoded_result(value)?);
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