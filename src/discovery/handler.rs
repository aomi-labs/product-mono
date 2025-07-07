use async_trait::async_trait;
use std::collections::HashMap;
use alloy_primitives::{Address, U256, Bytes};
use serde::{Deserialize, Serialize};

/// Contract value type using proper Alloy types for type safety
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum HandlerValue {
    Bytes(Bytes),
    Number(U256),
    Address(Address),
    Uint8(u8),
}


impl HandlerValue {
    /// Convert HandlerValue to U256 for reference resolution
    /// Used when this value is referenced by other handlers
    pub fn to_u256(&self) -> Result<U256, String> {
        match self {
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