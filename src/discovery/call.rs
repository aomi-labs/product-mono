use async_trait::async_trait;
use std::collections::HashMap;
use alloy_primitives::{Address, U256, Bytes};
use serde::{Deserialize, Serialize};

use crate::discovery::handler::{parse_reference, Handler, HandlerResult, HandlerValue};

/// Function parameter for call handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CallParameter {
    Direct(CallValue),
    Reference(String), // For references like "{{ admin }}"
}

/// Call value - can be various types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CallValue {
    Address(Address),
    Number(U256),
    Bytes(Bytes),
    String(String),
    Boolean(bool),
}

/// Call handler configuration, similar to L2Beat's CallHandler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallConfig {
    pub method: String,
    pub params: Option<Vec<CallParameter>>,
    pub address: Option<CallParameter>, // For cross-contract calls
    pub expect_revert: Option<bool>,
}

/// Call handler implementation mimicking L2Beat's CallHandler
#[derive(Debug, Clone)]
pub struct CallHandler {
    pub field: String,
    pub dependencies: Vec<String>,
    pub call: CallConfig,
}

impl CallHandler {
    pub fn new(field: String, call: CallConfig) -> Self {
        let dependencies = Self::extract_dependencies(&call);
        Self {
            field,
            dependencies,
            call,
        }
    }

    /// Extract field dependencies from call configuration
    fn extract_dependencies(call: &CallConfig) -> Vec<String> {
        let mut deps = Vec::new();
        
        // Extract from address parameter
        if let Some(address_param) = &call.address {
            Self::extract_param_deps(address_param, &mut deps);
        }
        
        // Extract from method parameters
        if let Some(params) = &call.params {
            for param in params {
                Self::extract_param_deps(param, &mut deps);
            }
        }
        
        deps
    }

    /// Extract dependencies from a parameter
    fn extract_param_deps(param: &CallParameter, deps: &mut Vec<String>) {
        if let CallParameter::Reference(ref_str) = param {
            if let Some(field_name) = parse_reference(ref_str) {
                deps.push(field_name);
            }
        }
    }

    /// Resolve call parameter value using previous results
    fn resolve_parameter(
        &self,
        param: &CallParameter,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<CallValue, String> {
        match param {
            CallParameter::Direct(value) => Ok(value.clone()),
            CallParameter::Reference(ref_str) => {
                if let Some(field_name) = parse_reference(ref_str) {
                    if let Some(result) = previous_results.get(&field_name) {
                        if let Some(value) = &result.value {
                            Self::handler_value_to_call_value(value)
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

    /// Convert HandlerValue to CallValue
    fn handler_value_to_call_value(handler_value: &HandlerValue) -> Result<CallValue, String> {
        match handler_value {
            HandlerValue::Address(addr) => Ok(CallValue::Address(*addr)),
            HandlerValue::Number(num) => Ok(CallValue::Number(*num)),
            HandlerValue::Bytes(bytes) => Ok(CallValue::Bytes(bytes.clone())),
            HandlerValue::String(s) => Ok(CallValue::String(s.clone())),
            HandlerValue::Boolean(b) => Ok(CallValue::Boolean(*b)),
            HandlerValue::Array(_) => Err("Cannot convert array to call parameter".to_string()),
            HandlerValue::Object(_) => Err("Cannot convert object to call parameter".to_string()),
        }
    }

    /// Convert call result to HandlerValue
    /// This uses the improved HandlerValue that can handle arbitrary-length data
    fn convert_call_result(&self, result: &[u8]) -> Result<HandlerValue, String> {
        // Use the improved from_call_result method that can handle arbitrary-length data
        // In a real implementation, this would:
        // 1. Decode the result using the function's ABI
        // 2. Convert the decoded result to the appropriate HandlerValue type
        // 3. Handle complex return types like arrays and structs
        Ok(HandlerValue::from_call_result(result))
    }

    /// Get the target address for the call
    fn get_target_address(
        &self,
        contract_address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<Address, String> {
        if let Some(address_param) = &self.call.address {
            let resolved = self.resolve_parameter(address_param, previous_results)?;
            match resolved {
                CallValue::Address(addr) => Ok(addr),
                _ => Err("Address parameter must resolve to an address".to_string()),
            }
        } else {
            Ok(*contract_address)
        }
    }

    /// Resolve all parameters for the call
    fn resolve_parameters(
        &self,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<Vec<CallValue>, String> {
        if let Some(params) = &self.call.params {
            params.iter()
                .map(|param| self.resolve_parameter(param, previous_results))
                .collect()
        } else {
            Ok(vec![])
        }
    }
}

#[async_trait]
impl Handler for CallHandler {
    fn field(&self) -> &str {
        &self.field
    }

    fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    async fn execute(
        &self,
        provider: &(dyn Send + Sync),
        address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> HandlerResult {
        // Get target address (might be different from contract address for cross-contract calls)
        let target_address = match self.get_target_address(address, previous_results) {
            Ok(addr) => addr,
            Err(error) => {
                return HandlerResult {
                    field: self.field.clone(),
                    value: None,
                    error: Some(format!("Failed to resolve target address: {}", error)),
                    ignore_relative: None,
                };
            }
        };

        // Resolve parameters
        let parameters = match self.resolve_parameters(previous_results) {
            Ok(params) => params,
            Err(error) => {
                return HandlerResult {
                    field: self.field.clone(),
                    value: None,
                    error: Some(format!("Failed to resolve parameters: {}", error)),
                    ignore_relative: None,
                };
            }
        };

        // Execute the call
        let call_result = self.simulate_call(provider, &target_address, &self.call.method, &parameters).await;

        match call_result {
            Ok(result) => {
                // Convert the call result to HandlerValue
                match self.convert_call_result(&result) {
                    Ok(converted_value) => HandlerResult {
                        field: self.field.clone(),
                        value: Some(converted_value),
                        error: None,
                        ignore_relative: None,
                    },
                    Err(error) => HandlerResult {
                        field: self.field.clone(),
                        value: None,
                        error: Some(format!("Failed to convert call result: {}", error)),
                        ignore_relative: None,
                    },
                }
            }
            Err(error) => {
                // Check if revert was expected
                if self.call.expect_revert.unwrap_or(false) {
                    // Expected revert - return success with null value
                    HandlerResult {
                        field: self.field.clone(),
                        value: None,
                        error: None,
                        ignore_relative: None,
                    }
                } else {
                    HandlerResult {
                        field: self.field.clone(),
                        value: None,
                        error: Some(format!("Call failed: {}", error)),
                        ignore_relative: None,
                    }
                }
            }
        }
    }
}

impl CallHandler {
    /// Simulate contract call - this should be replaced with actual provider call
    async fn simulate_call(
        &self,
        _provider: &(dyn Send + Sync),
        _address: &Address,
        _method: &str,
        _parameters: &[CallValue],
    ) -> Result<Vec<u8>, String> {
        // TODO: Replace with actual provider.call(address, method, parameters) call
        // For now, return a placeholder result
        Ok(vec![0u8; 32])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_handler_creation() {
        let call = CallConfig {
            method: "owner".to_string(),
            params: None,
            address: None,
            expect_revert: None,
        };
        
        let handler = CallHandler::new("owner".to_string(), call);
        assert_eq!(handler.field(), "owner");
        assert_eq!(handler.dependencies().len(), 0);
    }

    #[test]
    fn test_call_handler_with_parameters() {
        let call = CallConfig {
            method: "balanceOf".to_string(),
            params: Some(vec![
                CallParameter::Reference("{{ userAddress }}".to_string()),
            ]),
            address: None,
            expect_revert: None,
        };
        
        let handler = CallHandler::new("balance".to_string(), call);
        assert_eq!(handler.dependencies().len(), 1);
        assert_eq!(handler.dependencies()[0], "userAddress");
    }

    #[test]
    fn test_cross_contract_call() {
        let call = CallConfig {
            method: "totalSupply".to_string(),
            params: None,
            address: Some(CallParameter::Reference("{{ tokenAddress }}".to_string())),
            expect_revert: None,
        };
        
        let handler = CallHandler::new("totalSupply".to_string(), call);
        assert_eq!(handler.dependencies().len(), 1);
        assert_eq!(handler.dependencies()[0], "tokenAddress");
    }

    #[test]
    fn test_parse_reference() {
        // Test cases for reference parsing
        let test_cases = serde_json::json!({
            "valid": [
                {"input": "{{ admin }}", "expected": "admin"},
                {"input": "{{owner}}", "expected": "owner"},
                {"input": "{{ tokenAddress }}", "expected": "tokenAddress"}
            ],
            "invalid": ["admin", "{{ }}", "", "{ admin }"]
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
    fn test_handler_value_to_call_value() {
        // Test conversion from HandlerValue to CallValue
        let addr = Address::from([0x42; 20]);
        let handler_addr = HandlerValue::Address(addr);
        let call_val = CallHandler::handler_value_to_call_value(&handler_addr).unwrap();
        if let CallValue::Address(converted_addr) = call_val {
            assert_eq!(converted_addr, addr);
        } else {
            panic!("Expected CallValue::Address");
        }

        let handler_num = HandlerValue::Number(U256::from(42));
        let call_val = CallHandler::handler_value_to_call_value(&handler_num).unwrap();
        if let CallValue::Number(converted_num) = call_val {
            assert_eq!(converted_num, U256::from(42));
        } else {
            panic!("Expected CallValue::Number");
        }

        let handler_bytes = HandlerValue::Bytes(Bytes::from_static(b"test"));
        let call_val = CallHandler::handler_value_to_call_value(&handler_bytes).unwrap();
        if let CallValue::Bytes(converted_bytes) = call_val {
            assert_eq!(converted_bytes, Bytes::from_static(b"test"));
        } else {
            panic!("Expected CallValue::Bytes");
        }

        let handler_string = HandlerValue::String("test_string".to_string());
        let call_val = CallHandler::handler_value_to_call_value(&handler_string).unwrap();
        if let CallValue::String(converted_str) = call_val {
            assert_eq!(converted_str, "test_string");
        } else {
            panic!("Expected CallValue::String");
        }

        let handler_bool = HandlerValue::Boolean(true);
        let call_val = CallHandler::handler_value_to_call_value(&handler_bool).unwrap();
        if let CallValue::Boolean(converted_bool) = call_val {
            assert_eq!(converted_bool, true);
        } else {
            panic!("Expected CallValue::Boolean");
        }
    }
}