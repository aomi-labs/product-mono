pub mod lib;

use alloy_provider::{RootProvider, network::AnyNetwork};
use rmcp::{ErrorData, handler::server::tool::Parameters, model::CallToolResult};
use serde::{Deserialize, Deserializer};

use lib::etherscan::Network;
use lib::runner::DiscoveryRunner;

/// Custom deserializer for HandlerDefinition that handles both JSON objects and JSON strings
fn deserialize_handler<'de, D>(
    deserializer: D,
) -> Result<lib::handlers::config::HandlerDefinition, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    // First try to deserialize as a Value to see what we got
    let value: serde_json::Value = Deserialize::deserialize(deserializer)?;

    // If it's a string, parse it as JSON first
    let parsed_value = if let serde_json::Value::String(s) = value {
        serde_json::from_str(&s).map_err(D::Error::custom)?
    } else {
        value
    };

    // Now deserialize into HandlerDefinition
    serde_json::from_value(parsed_value).map_err(D::Error::custom)
}

/// Parameters for analyzing a smart contract and generating handler configurations
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeContractParams {
    #[schemars(
        description = "Ethereum contract address (e.g., '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48')"
    )]
    pub contract_address: String,

    #[schemars(
        description = "Natural language description of what data to extract from the contract (e.g., 'find implementation and admin', 'get all roles and members')"
    )]
    pub user_intent: String,
}

/// Parameters for extracting data using a handler
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExtractDataParams {
    #[schemars(description = "Ethereum contract address to query")]
    pub contract_address: String,

    #[schemars(
        description = "Handler configuration specifying how to extract data (can be JSON object or JSON string)"
    )]
    #[serde(deserialize_with = "deserialize_handler")]
    pub handler: lib::handlers::config::HandlerDefinition,
}

/// Tool for analyzing smart contracts and generating handler configurations
#[derive(Clone)]
pub struct L2BTool;

impl L2BTool {
    /// Analyze a contract and generate handler configurations
    pub async fn analyze_contract(
        &self,
        Parameters(params): Parameters<AnalyzeContractParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Create provider - using mainnet RPC
        let provider =
            RootProvider::<AnyNetwork>::new_http("http://localhost:8545".parse().unwrap());
        let runner = DiscoveryRunner::new(Network::Mainnet, provider)
            .expect("Failed to create DiscoveryRunner");

        // Generate handler configs
        let (contract_analysis, handlers) = runner
            .generate_handler_configs(&params.contract_address, &params.user_intent)
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to analyze contract: {}", e), None)
            })?;

        let mut handlers_json = serde_json::Map::new();
        use serde_json::json;
        for (name, def) in &handlers {
            let handler_value = serde_json::to_value(def).map_err(|err| {
                ErrorData::internal_error("serde_json filed to conver to value", None)
            })?;
            handlers_json.insert(
                name.clone(),
                json!({
                    "handler": handler_value
                }),
            );
        }

        let handler_output = serde_json::to_string_pretty(&handlers_json).map_err(|err| {
            ErrorData::internal_error("serde_json failed to format pretty string", None)
        })?;

        // Build response JSON
        let response = serde_json::json!({
            "pattern_detected": contract_analysis.pattern_detected,
            "summary": contract_analysis.summary,
            "handlers": handler_output,
        });

        // Serialize to pretty JSON string
        let mut json_str = serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(format!("Failed to serialize response: {}", e), None)
        })?;

        json_str.push_str("  | Further instructions: 
        
        1. Prompt user with a question of whether they want to proceed with execution \
           or just view the formatted handlers. DO NOT make any more tool calls unless the user has instructed you to.
        2. When displaying handlers, show them the same way that I've formatted them for you");

        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            json_str,
        )]))
    }

    /// Generic handler execution function used by all extract_* methods
    async fn execute_handler_internal(
        params: ExtractDataParams,
        expected_type: Option<&str>,
    ) -> Result<CallToolResult, ErrorData> {
        use alloy_primitives::Address;
        use std::collections::HashMap;
        use std::str::FromStr;

        // Validate handler type if specified
        if let Some(expected) = expected_type {
            let actual_type = match &params.handler {
                lib::handlers::config::HandlerDefinition::Call { .. } => "Call",
                lib::handlers::config::HandlerDefinition::Storage { .. } => "Storage",
                lib::handlers::config::HandlerDefinition::Event { .. } => "Event",
                lib::handlers::config::HandlerDefinition::AccessControl { .. } => "AccessControl",
                lib::handlers::config::HandlerDefinition::DynamicArray { .. } => "DynamicArray",
                _ => "Other",
            };

            if actual_type != expected {
                return Err(ErrorData::invalid_params(
                    format!(
                        "Handler must be of type '{}', got '{}'",
                        expected, actual_type
                    ),
                    None,
                ));
            }
        }

        // Parse contract address
        let contract_address = Address::from_str(&params.contract_address).map_err(|e| {
            ErrorData::invalid_params(format!("Invalid contract address: {}", e), None)
        })?;

        // Create provider
        let provider =
            RootProvider::<AnyNetwork>::new_http("http://localhost:8545".parse().unwrap());
        let runner = DiscoveryRunner::new(Network::Mainnet, provider).map_err(|e| {
            ErrorData::internal_error(format!("Failed to create runner: {}", e), None)
        })?;

        // Execute the handler
        let result = runner
            .execute_handler(
                "result".to_string(),
                params.handler,
                &contract_address,
                &HashMap::new(),
            )
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to execute handler: {}", e), None)
            })?;

        // Build response
        let response = serde_json::json!({
            "value": result.value,
            "error": result.error,
        });

        let mut json_str = serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(format!("Failed to serialize response: {}", e), None)
        })?;

        json_str.push_str(" | Additional info: \
            1. If an empty list was provided, then there were no records found in the last N blocks searched ");

        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            json_str,
        )]))
    }

    /// Extract data using a Call handler
    pub async fn extract_call_data(
        &self,
        Parameters(params): Parameters<ExtractDataParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Self::execute_handler_internal(params, Some("Call")).await
    }

    /// Extract data using a Storage handler
    pub async fn extract_storage_data(
        &self,
        Parameters(params): Parameters<ExtractDataParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Self::execute_handler_internal(params, Some("Storage")).await
    }

    /// Extract data using an AccessControl handler
    pub async fn extract_access_control_data(
        &self,
        Parameters(params): Parameters<ExtractDataParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Self::execute_handler_internal(params, Some("AccessControl")).await
    }

    /// Extract data using an Event handler
    pub async fn extract_event_data(
        &self,
        Parameters(params): Parameters<ExtractDataParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Self::execute_handler_internal(params, Some("Event")).await
    }

    /// Extract data using any handler (no type validation)
    pub async fn extract_data(
        &self,
        Parameters(params): Parameters<ExtractDataParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Self::execute_handler_internal(params, None).await
    }
}
