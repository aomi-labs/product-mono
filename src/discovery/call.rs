use async_trait::async_trait;
use std::collections::HashMap;
use alloy_primitives::{Address, U256};
use alloy_provider::{RootProvider, network::Network};
use serde::{Deserialize, Serialize};

use crate::discovery::handler::{extract_fields, parse_reference, resolve_reference, Handler, HandlerResult, HandlerValue};
use crate::discovery::config::HandlerDefinition;

/// Call handler configuration, similar to L2Beat's CallHandler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallConfig {
    pub method: String,
    pub params: Option<Vec<HandlerValue>>,
    pub address: Option<HandlerValue>, // For cross-contract calls
    pub expect_revert: Option<bool>,
}

/// Call handler implementation mimicking L2Beat's CallHandler
#[derive(Debug, Clone)]
pub struct CallHandler<N> {
    pub field: String,
    pub dependencies: Vec<String>,
    pub call: CallConfig,
    pub hidden: bool,
    _phantom: std::marker::PhantomData<N>,
}

impl<N> CallHandler<N> {
    pub fn new(field: String, call: CallConfig, hidden: bool) -> Self {
        let dependencies = Self::resolve_dependencies(&call);
        Self {
            field,
            dependencies,
            call,
            hidden,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Create CallHandler from HandlerDefinition::Call
    pub fn from_handler_definition(field: String, handler: HandlerDefinition) -> Result<Self, String> {
        match handler {
            HandlerDefinition::Call { 
                method, 
                args, 
                expect_revert,
                address,
                ignore_relative 
            } => {
                let params = if let Some(args) = args {
                    let mut call_params = Vec::new();
                    for arg in args {
                        let param = HandlerValue::from_json_value(arg)?;
                        call_params.push(param);
                    }
                    Some(call_params)
                } else {
                    None
                };
                let address = if let Some(address) = address {
                    Some(HandlerValue::from_json_value(address.into())?)
                } else {
                    None
                };
                let call_config = CallConfig {
                    method,
                    params,
                    address,
                    expect_revert,
                };

                Ok(Self::new(field, call_config, ignore_relative.unwrap_or(false)))
            }
            _ => Err("Handler definition is not a call handler".to_string()),
        }
    }

    /// Convert HandlerDefinition::Call fields to CallConfig
    fn convert_to_call_config(
        method: String,
        args: Option<Vec<serde_json::Value>>,
        _return_type: Option<String>, // Not used in CallConfig currently
    ) -> Result<CallConfig, String> {
        let params = if let Some(args) = args {
            let mut call_params = Vec::new();
            for arg in args {
                let param = HandlerValue::from_json_value(arg)?;
                call_params.push(param);
            }
            Some(call_params)
        } else {
            None
        };

        Ok(CallConfig {
            method,
            params,
            address: None, // Cross-contract calls would be handled separately
            expect_revert: None, // Default to not expecting revert
        })
    }

    /// Extract field dependencies from call configuration
    fn resolve_dependencies(call: &CallConfig) -> Vec<String> {
        let mut deps = Vec::new();
        
        // Extract from address parameter
        if let Some(address_param) = &call.address {
            extract_fields(address_param, &mut deps);
        }
        
        // Extract from method parameters
        if let Some(params) = &call.params {
            for param in params {
                extract_fields(param, &mut deps);
            }
        }
        
        deps
    }


    /// Convert call result to HandlerValue
    /// This uses the improved HandlerValue that can handle arbitrary-length data
    fn convert_call_result(&self, result: &[u8]) -> Result<HandlerValue, String> {
        // Use the improved from_call_result method that can handle arbitrary-length data
        // In a real implementation, this would:
        // 1. Decode the result using the function's ABI
        // 2. Convert the decoded result to the appropriate HandlerValue type
        // 3. Handle complex return types like arrays and structs
        Ok(HandlerValue::from_raw_bytes(result))
    }

    /// Get the target address for the call
    fn resolve_target_address(
        &self,
        contract_address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<Address, String> {
        if let Some(address_param) = &self.call.address {
            let resolved = resolve_reference(address_param, previous_results)?;
            match resolved {
                HandlerValue::Address(addr) => Ok(addr),
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
    ) -> Result<Vec<HandlerValue>, String> {
        if let Some(params) = &self.call.params {
            params.iter()
                .map(|param| resolve_reference(param, previous_results))
                .collect()
        } else {
            Ok(vec![])
        }
    }
}

#[async_trait]
impl<N: Network> Handler<N> for CallHandler<N> {
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
        // Get target address (might be different from contract address for cross-contract calls)
        let target_address = match self.resolve_target_address(address, previous_results) {
            Ok(addr) => addr,
            Err(error) => {
                return HandlerResult {
                    field: self.field.clone(),
                    value: None,
                    error: Some(format!("Failed to resolve target address: {}", error)),
                    hidden: self.hidden,
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
                    hidden: self.hidden,
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
                        hidden: self.hidden,
                    },
                    Err(error) => HandlerResult {
                        field: self.field.clone(),
                        value: None,
                        error: Some(format!("Failed to convert call result: {}", error)),
                        hidden: self.hidden,
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
                        hidden: self.hidden,
                    }
                } else {
                    HandlerResult {
                        field: self.field.clone(),
                        value: None,
                        error: Some(format!("Call failed: {}", error)),
                        hidden: self.hidden,
                    }
                }
            }
        }
    }
}

impl<N: Network> CallHandler<N> {
    /// Simulate contract call - this should be replaced with actual provider call
    async fn simulate_call(
        &self,
        _provider: &RootProvider<N>,
        _address: &Address,
        _method: &str,
        _parameters: &[HandlerValue],
    ) -> Result<Vec<u8>, String> {
        // TODO: Replace with actual provider.call(address, method, parameters) call
        // For now, return a placeholder result
        Ok(vec![0u8; 32])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_provider::network::AnyNetwork;

    type AnyCallHandler = CallHandler<AnyNetwork>;


    #[test]
    fn test_call_handler_creation() {
        let call = CallConfig {
            method: "owner".to_string(),
            params: None,
            address: None,
            expect_revert: None,
        };
        
        let handler = AnyCallHandler::new("owner".to_string(), call, false);
        assert_eq!(handler.field(), "owner");
        assert_eq!(handler.dependencies().len(), 0);
    }

    #[test]
    fn test_call_handler_with_parameters() {
        let call = CallConfig {
            method: "balanceOf".to_string(),
            params: Some(vec![
                HandlerValue::Reference("{{ userAddress }}".to_string()),
            ]),
            address: None,
            expect_revert: None,
        };
        
        let handler = AnyCallHandler::new("balance".to_string(), call, false);
        assert_eq!(handler.dependencies().len(), 1);
        assert_eq!(handler.dependencies()[0], "userAddress");
    }

    #[test]
    fn test_cross_contract_call() {
        let call = CallConfig {
            method: "totalSupply".to_string(),
            params: None,
            address: Some(HandlerValue::Reference("{{ tokenAddress }}".to_string())),
            expect_revert: None,
        };
        
        let handler = AnyCallHandler::new("totalSupply".to_string(), call, false);
        assert_eq!(handler.dependencies().len(), 1);
        assert_eq!(handler.dependencies()[0], "tokenAddress");
    }

    #[test]
    fn test_from_handler_definition() {
        // Test basic call handler creation
        let handler_def = HandlerDefinition::Call {
            method: "owner".to_string(),
            args: None,
            expect_revert: None,
            address: None,
            ignore_relative: None,
        };

        let call_handler = AnyCallHandler::from_handler_definition("owner".to_string(), handler_def).unwrap();
        
        assert_eq!(call_handler.field(), "owner");
        assert_eq!(call_handler.dependencies().len(), 0);
        assert_eq!(call_handler.call.method, "owner");
        assert!(call_handler.call.params.is_none());
        assert!(call_handler.call.address.is_none());
        assert!(call_handler.call.expect_revert.is_none());
    }


    #[test]
    fn test_wrong_handler_definition_type() {
        // Test that non-call handler definitions are rejected
        let handler_def = HandlerDefinition::Storage {
            slot: Some(serde_json::Value::Number(serde_json::Number::from(3))),
            offset: None,
            return_type: Some("address".to_string()),
            ignore_relative: None,
        };

        let result = AnyCallHandler::from_handler_definition("test".to_string(), handler_def);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Handler definition is not a call handler"));
    }

    #[tokio::test]
    async fn test_cross_contract_call_execution() {
        // Simulate a token address to be used as the cross-contract target
        let token_address = Address::from([0x42u8; 20]);
        let contract_address = Address::from([0x11u8; 20]);

        // Prepare previous_results with tokenAddress resolved
        let mut previous_results = HashMap::new();
        previous_results.insert(
            "tokenAddress".to_string(),
            HandlerResult {
                field: "tokenAddress".to_string(),
                value: Some(HandlerValue::Address(token_address)),
                error: None,
                hidden: false,
            },
        );

        // Handler that references tokenAddress for its call target
        let call = CallConfig {
            method: "totalSupply".to_string(),
            params: None,
            address: Some(HandlerValue::Reference("{{ tokenAddress }}".to_string())),
            expect_revert: None,
        };
        let handler = AnyCallHandler::new("totalSupply".to_string(), call, false);

        // Create a mock provider for testing - since simulate_call doesn't actually use it,
        // we can use foundry's provider builder which returns a RootProvider
        let provider = foundry_common::provider::get_http_provider("http://localhost:8545");

        // Execute the handler
        let result = handler.execute(&provider, &contract_address, &previous_results).await;

        // The handler should resolve the target address to token_address
        // The simulated call returns 32 zero bytes, so the value should be Bytes([0u8; 32])
        assert!(result.error.is_none());
        assert_eq!(result.field, "totalSupply");
        let expected_value = HandlerValue::Bytes(alloy_primitives::Bytes::copy_from_slice(&vec![0u8; 32]));
        assert_eq!(result.value, Some(expected_value));
    }
}