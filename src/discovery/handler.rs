use async_trait::async_trait;
use std::collections::HashMap;
use alloy_primitives::{Address, U256, keccak256, hex, Bytes};
use serde::{Deserialize, Serialize};

/// Contract value type using proper Alloy types for type safety
#[derive(Debug, Clone, PartialEq)]
pub enum HandlerValue {
    Bytes(Bytes),
    Number(U256),
    Address(Address),
    Uint8(u8),
}

impl HandlerValue {
    /// Convert to JSON value for serialization
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            HandlerValue::Bytes(bytes) => serde_json::Value::String(format!("0x{}", hex::encode(bytes))),
            HandlerValue::Number(num) => serde_json::Value::String(num.to_string()),
            HandlerValue::Address(addr) => serde_json::Value::String(format!("0x{:040x}", addr)),
            HandlerValue::Uint8(val) => serde_json::Value::Number(serde_json::Number::from(*val)),
        }
    }

    /// Parse from JSON value (for reference resolution)
    pub fn from_json(value: &serde_json::Value) -> Result<U256, String> {
        match value {
            serde_json::Value::String(s) => {
                if s.starts_with("0x") {
                    U256::from_str_radix(&s[2..], 16)
                        .map_err(|e| format!("Failed to parse hex string: {}", e))
                } else {
                    U256::from_str_radix(s, 10)
                        .map_err(|e| format!("Failed to parse decimal string: {}", e))
                }
            }
            serde_json::Value::Number(n) => {
                if let Some(u) = n.as_u64() {
                    Ok(U256::from(u))
                } else {
                    Err("Number too large or not an integer".to_string())
                }
            }
            _ => Err(format!("Cannot convert JSON value to U256: {:?}", value)),
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