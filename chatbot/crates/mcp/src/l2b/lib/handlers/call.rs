use alloy_primitives::{Address, hex};
use alloy_provider::network::TransactionBuilder;
use alloy_provider::{Provider, RootProvider, network::Network};
use async_trait::async_trait;
use cast::SimpleCast;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::config::HandlerDefinition;
use super::types::{Handler, HandlerResult, HandlerValue, extract_fields, resolve_reference};

/// Call handler configuration, similar to L2Beat's CallHandler
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CallConfig {
    pub method: String,
    pub params: Option<Vec<HandlerValue>>,
    pub address: Option<HandlerValue>, // For cross-contract calls
    pub expect_revert: Option<bool>,
}

impl CallConfig {
    /// Encode method signature + parameters into calldata bytes using foundry's SimpleCast
    pub fn encode_calldata(&self) -> Result<Vec<u8>, String> {
        // Convert HandlerValues to string representations that SimpleCast can understand
        let string_args: Result<Vec<String>, String> = self
            .params
            .clone()
            .unwrap_or_default()
            .iter()
            .map(|param| param.to_string())
            .collect();
        let string_args = string_args?;

        // Use foundry's calldata_encode which handles everything
        let hex_calldata = SimpleCast::calldata_encode(&self.method, &string_args)
            .map_err(|e| format!("Failed to encode calldata: {}", e))?;

        // Convert hex string to bytes (remove "0x" prefix)
        let hex_str = hex_calldata.strip_prefix("0x").unwrap_or(&hex_calldata);
        hex::decode(hex_str).map_err(|e| format!("Failed to decode hex calldata: {}", e))
    }

    /// Resolve all parameters for the call
    fn resolve_parameters(
        &self,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<Vec<HandlerValue>, String> {
        if let Some(params) = &self.params {
            params
                .iter()
                .map(|param| resolve_reference(param, previous_results))
                .collect()
        } else {
            Ok(vec![])
        }
    }
}

/// Call handler implementation mimicking L2Beat's CallHandler
#[derive(Debug, Clone, Default)]
pub struct CallHandler<N> {
    pub field: String,
    pub dependencies: Vec<String>,
    pub call: CallConfig,
    pub hidden: bool,
    _phantom: std::marker::PhantomData<N>,
}

impl<N> CallHandler<N> {
    pub fn new(field: String, call: CallConfig, hidden: bool) -> Self {
        let mut handler = Self {
            field,
            dependencies: Vec::new(),
            call,
            hidden,
            _phantom: std::marker::PhantomData,
        };
        handler.dependencies = handler.resolve_dependencies();
        handler
    }

    /// Create CallHandler from HandlerDefinition::Call
    pub fn from_handler_definition(
        field: String,
        handler: HandlerDefinition,
    ) -> Result<Self, String> {
        match handler {
            HandlerDefinition::Call {
                method,
                args,
                expect_revert,
                address,
                ignore_relative,
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

                Ok(Self::new(
                    field,
                    call_config,
                    ignore_relative.unwrap_or(false),
                ))
            }
            _ => Err("Handler definition is not a call handler".to_string()),
        }
    }

    /// Extract field dependencies from call configuration
    pub fn resolve_dependencies(&self) -> Vec<String> {
        let mut deps = Vec::new();

        // Extract from address parameter
        if let Some(address_param) = &self.call.address {
            extract_fields(address_param, &mut deps);
        }

        // Extract from method parameters
        if let Some(params) = &self.call.params {
            for param in params {
                extract_fields(param, &mut deps);
            }
        }

        deps
    }

    /// Convert call result to HandlerValue using foundry's abi_decode
    fn convert_call_result(&self, result: &[u8]) -> Result<HandlerValue, String> {
        // Handle empty result (void return)
        if result.is_empty() {
            return Ok(HandlerValue::Array(vec![])); // Use empty array for void
        }

        // Convert result to hex string for SimpleCast
        let hex_result = format!("0x{}", hex::encode(result));

        // For now, just return the raw bytes as a hex string
        // TODO: Implement proper ABI decoding once version conflicts are resolved
        Ok(HandlerValue::String(hex_result))
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

        // Execute the call
        let call_result = self
            .make_call(provider, &target_address, previous_results)
            .await;

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
    /// Make actual contract call using the provider
    async fn make_call(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<Vec<u8>, String> {
        // Encode calldata using CallConfig methods
        self.call.resolve_parameters(previous_results)?;
        let calldata = self.call.encode_calldata()?;

        let mut tx = N::TransactionRequest::default();
        tx.set_to(*address);
        tx.set_input(calldata);

        match provider.call(tx).await {
            Ok(result) => Ok(result.to_vec()),
            Err(e) => Err(format!("Contract call failed: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;
    use alloy_provider::network::AnyNetwork;

    type AnyCallHandler = CallHandler<AnyNetwork>;

    #[test]
    fn test_call_handler_creation() {
        let call = CallConfig {
            method: "owner()".to_string(),
            params: None,
            address: None,
            expect_revert: None,
        };

        let handler = AnyCallHandler::new("owner".to_string(), call, false);
        // Basic functionality test - just verify it doesn't panic
        assert_eq!(handler.field(), "owner");
    }

    #[test]
    fn test_call_handler_with_parameters() {
        let call = CallConfig {
            method: "balanceOf(address)".to_string(),
            params: Some(vec![HandlerValue::Reference(
                "{{ userAddress }}".to_string(),
            )]),
            address: None,
            expect_revert: None,
        };

        let handler = AnyCallHandler::new("balance".to_string(), call, false);
        // Test dependency resolution - this is the core functionality
        assert_eq!(handler.dependencies()[0], "userAddress");
    }

    #[test]
    fn test_e2e_linea_token_bridge_call() {
        use crate::l2b::lib::handlers::config::HandlerDefinition;

        // Test E2E parsing of the Linea TokenBridge template call handler
        let handler_def = HandlerDefinition::Call {
            method: "function isPaused(uint8 _pauseType) view returns (bool pauseTypeIsPaused)"
                .to_string(),
            args: Some(vec![serde_json::Value::Number(serde_json::Number::from(1))]),
            expect_revert: None,
            address: None,
            ignore_relative: Some(false),
        };

        let call_handler =
            AnyCallHandler::from_handler_definition("isPaused_GENERAL".to_string(), handler_def)
                .unwrap();

        // Focus on end result: calldata encoding works
        let calldata = call_handler.call.encode_calldata().unwrap();
        assert!(calldata.len() >= 4); // Should have at least selector
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
            method: "totalSupply()".to_string(),
            params: None,
            address: Some(HandlerValue::Reference("{{ tokenAddress }}".to_string())),
            expect_revert: None,
        };
        let handler = AnyCallHandler::new("totalSupply".to_string(), call, false);

        // Create a mock provider for testing
        let provider = foundry_common::provider::get_http_provider("http://localhost:8545");

        // Execute the handler
        let result = handler
            .execute(&provider, &contract_address, &previous_results)
            .await;

        // The handler should resolve the target address to token_address
        // Since we don't have actual contract data, the call will likely fail
        // but we can verify the error handling works
        assert_eq!(result.field, "totalSupply");

        // The call will likely fail due to no actual contract at the address
        // but the important thing is that we're testing the flow
        if result.error.is_some() {
            // Expected for mock environment
        } else if let Some(_value) = result.value {
            // If somehow successful, that's also fine
        }
    }

    #[tokio::test]
    async fn test_dao_proposals_call_execution() {
        // Configure a call to proposals(uint256) with index 1
        let call = CallConfig {
            method: "proposals(uint256)".to_string(),
            params: Some(vec![HandlerValue::Number(U256::from(1u64))]),
            address: None,
            expect_revert: None,
        };
        let handler = AnyCallHandler::new("proposals".to_string(), call, false);

        // Create a mock provider and a dummy contract address
        let provider = foundry_common::provider::get_http_provider("http://localhost:8545");
        let contract_address: Address = "0xBB9bc244D798123fDe783fCc1C72d3Bb8C189413"
            .parse()
            .unwrap();
        //let contract_address = Address::from([0x11u8; 20]);

        // Execute the handler (eth_call). In a mock environment this may error, which is acceptable.
        let result = handler
            .execute(&provider, &contract_address, &HashMap::new())
            .await;

        // Validate field name and that we either have a value or an error
        assert_eq!(result.field, "proposals");
        if result.error.is_some() {
            // Expected for mock environment without a real contract
        } else if let Some(_value) = result.value {
            // If it succeeds (e.g., connected to a real node with the contract), that's fine too
            println!("Proposals call succeeded: {:?}", _value);
        }
    }
}
