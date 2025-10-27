use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;

use rig::tool::ToolError;
use rig_derive::rig_tool;
use baml_client::apis::{configuration::Configuration, default_api};
use baml_client::models::{AnalyzeAbiRequest, AnalyzeEventRequest, AnalyzeLayoutRequest};
use lib::etherscan::{EtherscanClient, Network};
use alloy_primitives::Address as AlloyAddress;
use alloy_provider::{network::AnyNetwork, RootProvider};
use lib::runner::DiscoveryRunner;
use std::str::FromStr;

use crate::l2b::lib::handlers::config::HandlerDefinition;

pub mod lib;

// Global handler map that gets populated by the analysis tools
static HANDLER_MAP: LazyLock<Mutex<HashMap<String, HandlerDefinition>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// ============================================================================
// Tool 1: Analyze ABI
// ============================================================================

#[rig_tool(
    description = "Analyze a smart contract's ABI to identify view/pure functions and generate Call handler definitions. Fetches contract data from Etherscan and populates the global handler map.",
    params(
        contract_address = "Ethereum contract address (e.g., '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48')",
        intent = "Optional: What data you want to retrieve (e.g., 'Get token balance and supply')"
    )
)]
async fn analyze_abi(
    contract_address: String,
    _intent: Option<String>,
) -> Result<String, rig::tool::ToolError> {

    // Fetch contract data from Etherscan
    let etherscan = EtherscanClient::new(Network::Mainnet)
        .map_err(|e| ToolError::ToolCallError(format!("Etherscan client error: {}", e).into()))?;

    let etherscan_results = etherscan
        .fetch_contract_data(&contract_address)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Failed to fetch from Etherscan: {}", e).into()))?;

    // Convert to ContractInfo
    let contract_info = lib::adapter::etherscan_to_contract_info(etherscan_results, None)
        .map_err(|e| ToolError::ToolCallError(format!("Failed to convert contract info: {}", e).into()))?;

    // Get BAML server URL
    let baml_base_url = std::env::var("BAML_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:2024".to_string());

    let mut config = Configuration::new();
    config.base_path = baml_base_url;

    // Prepare request
    let request = AnalyzeAbiRequest::new(contract_info);

    // Call BAML function
    let result = default_api::analyze_abi(&config, request)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("BAML call failed: {:?}", e).into()))?;

    // Convert to handler definitions
    let handlers = lib::adapter::abi_analysis_to_call_handlers(result.clone());

    // Populate global handler map
    let mut map = HANDLER_MAP.lock().await;

    let handler_names: Vec<String> = handlers.iter().map(|(name, _)| name.clone()).collect();

    for (name, handler_def) in handlers {
        map.insert(name, handler_def);
    }

    // Return formatted result
    let output = serde_json::json!({
        "summary": result.summary,
        "handler_count": handler_names.len(),
        "handler_names": handler_names,
        "retrievals": result.retrievals,
    });

    serde_json::to_string_pretty(&output)
        .map_err(|e| ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 2: Analyze Events
// ============================================================================

#[rig_tool(
    description = "Analyze smart contract events to generate Event and AccessControl handler definitions. Fetches contract data from Etherscan and populates the global handler map.",
    params(
        contract_address = "Ethereum contract address",
        intent = "Optional: What events to track (e.g., 'Track validators for chain 325')"
    )
)]
async fn analyze_events(
    contract_address: String,
    _intent: Option<String>,
) -> Result<String, rig::tool::ToolError> {

    // Fetch contract data from Etherscan
    let etherscan = EtherscanClient::new(Network::Mainnet)
        .map_err(|e| ToolError::ToolCallError(format!("Etherscan client error: {}", e).into()))?;

    let etherscan_results = etherscan
        .fetch_contract_data(&contract_address)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Failed to fetch from Etherscan: {}", e).into()))?;

    // Convert to ContractInfo
    let contract_info = lib::adapter::etherscan_to_contract_info(etherscan_results, None)
        .map_err(|e| ToolError::ToolCallError(format!("Failed to convert contract info: {}", e).into()))?;

    let baml_base_url = std::env::var("BAML_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:2024".to_string());

    let mut config = Configuration::new();
    config.base_path = baml_base_url;

    // First, analyze ABI to get events
    let abi_request = AnalyzeAbiRequest::new(contract_info.clone());

    let abi_result = default_api::analyze_abi(&config, abi_request)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("ABI analysis failed: {:?}", e).into()))?;

    // Then analyze events
    let event_request = AnalyzeEventRequest::new(abi_result, contract_info);

    let result = default_api::analyze_event(&config, event_request)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Event analysis failed: {:?}", e).into()))?;

    // Convert to handler definitions
    let handlers = lib::adapter::event_analysis_to_event_handlers(result.clone());

    // Populate global handler map
    let mut map = HANDLER_MAP.lock().await;

    let handler_names: Vec<String> = handlers.iter().map(|(name, _)| name.clone()).collect();

    for (name, handler_def) in handlers {
        map.insert(name, handler_def);
    }

    // Return formatted result
    let output = serde_json::json!({
        "summary": result.summary,
        "handler_count": handler_names.len(),
        "handler_names": handler_names,
        "event_actions": result.event_actions,
        "detected_constants": result.detected_constants,
        "warnings": result.warnings,
    });

    serde_json::to_string_pretty(&output)
        .map_err(|e| ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 3: Analyze Storage Layout
// ============================================================================

#[rig_tool(
    description = "Analyze smart contract storage layout to generate Storage and DynamicArray handler definitions. Fetches contract data from Etherscan and populates the global handler map.",
    params(
        contract_address = "Ethereum contract address",
        intent = "What storage slots you want to access (e.g., 'Access validator mappings and stake amounts')"
    )
)]
async fn analyze_layout(
    contract_address: String,
    intent: String,
) -> Result<String, rig::tool::ToolError> {

    // Fetch contract data from Etherscan
    let etherscan = EtherscanClient::new(Network::Mainnet)
        .map_err(|e| ToolError::ToolCallError(format!("Etherscan client error: {}", e).into()))?;

    let etherscan_results = etherscan
        .fetch_contract_data(&contract_address)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Failed to fetch from Etherscan: {}", e).into()))?;

    // Convert to ContractInfo
    let contract_info = lib::adapter::etherscan_to_contract_info(etherscan_results, None)
        .map_err(|e| ToolError::ToolCallError(format!("Failed to convert contract info: {}", e).into()))?;

    let baml_base_url = std::env::var("BAML_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:2024".to_string());

    let mut config = Configuration::new();
    config.base_path = baml_base_url;

    // First, analyze ABI
    let abi_request = AnalyzeAbiRequest::new(contract_info.clone());

    let abi_result = default_api::analyze_abi(&config, abi_request)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("ABI analysis failed: {:?}", e).into()))?;

    // Then analyze layout
    let layout_request = AnalyzeLayoutRequest::new( abi_result, contract_info, intent);

    let result = default_api::analyze_layout(&config, layout_request)
        .await
        .map_err(|e| {
            ToolError::ToolCallError(format!("Layout analysis failed: {:?}", e).into())
        })?;

    // Convert to handler definitions
    let handlers = lib::adapter::layout_analysis_to_storage_handlers(result.clone())
        .map_err(|e| ToolError::ToolCallError(format!("Handler conversion failed: {}", e).into()))?;

    // Populate global handler map
    let mut map = HANDLER_MAP.lock().await;

    let handler_names: Vec<String> = handlers.iter().map(|(name, _)| name.clone()).collect();

    for (name, handler_def) in handlers {
        map.insert(name, handler_def);
    }

    // Return formatted result
    let output = serde_json::json!({
        "contract_name": result.contract_name,
        "summary": result.summary,
        "handler_count": handler_names.len(),
        "handler_names": handler_names,
        "inheritance": result.inheritance,
        "slots": result.slots,
        "detected_constants": result.detected_constants,
        "warnings": result.warnings,
    });

    serde_json::to_string_pretty(&output)
        .map_err(|e| ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 4: Execute Handlers
// ============================================================================
// NOTE: Commented out due to Sync requirement incompatibility
// The async_trait-generated futures from Handler::execute() are only Send, not Sync
// But rig requires Send + Sync futures. This works in MCP which doesn't have Sync requirement.
// TODO: Either modify Handler trait to use manual async or use tokio::spawn wrapper
#[rig_tool(
    description = "Execute previously generated handlers by their names. Retrieves handler definitions from the global map and executes them against the specified contract using the DiscoveryRunner. Returns the extracted data for all handler types (Call, Storage, Event, AccessControl, DynamicArray).",
    params(
        contract_address = "Ethereum contract address to query",
        handler_names = "Comma-separated list of handler names to execute (e.g., 'owner,totalSupply,validators')",
        rpc_url = "Ethereum RPC endpoint URL (e.g., 'https://eth.llamarpc.com')"
    )
)]
async fn execute_handler(
    contract_address: String,
    handler_names: String,
) -> Result<String, rig::tool::ToolError> {
    tokio::spawn(execute_handlers_impl(contract_address, handler_names))
     .await
     .map_err(|e| ToolError::ToolCallError(format!("Task join error: {}", e).into()))?
}



async fn execute_handlers_impl(
    contract_address: String,
    handler_names: String,
) -> Result<String, rig::tool::ToolError> {


    // Parse handler names
    let names: Vec<String> = handler_names
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if names.is_empty() {
        return Err(ToolError::ToolCallError(
            "No handler names provided".into(),
        ));
    }

    // Get handlers from global map
    let map = HANDLER_MAP.lock().await;

    let mut handlers_to_execute = Vec::new();
    let mut missing_handlers = Vec::new();

    for name in &names {
        if let Some(handler_def) = map.get(name) {
            handlers_to_execute.push((name.clone(), handler_def.clone()));
        } else {
            missing_handlers.push(name.clone());
        }
    }

    drop(map); // Release lock

    if !missing_handlers.is_empty() {
        return Err(ToolError::ToolCallError(
            format!(
                "Handler(s) not found in map: {}. Run analyze_abi, analyze_events, or analyze_layout first to populate handlers.",
                missing_handlers.join(", ")
            )
            .into(),
        ));
    }

    // Setup provider
    let provider = RootProvider::<AnyNetwork>::new_http("http://localhost:8545".parse().unwrap());

    let contract_addr = AlloyAddress::from_str(&contract_address)
        .map_err(|e| ToolError::ToolCallError(format!("Invalid address: {}", e).into()))?;

    // Create DiscoveryRunner
    let runner = DiscoveryRunner::new(Network::Mainnet, provider)
        .map_err(|e| ToolError::ToolCallError(format!("Failed to create DiscoveryRunner: {}", e).into()))?;

    // Execute handlers
    let mut results = serde_json::Map::new();
    let mut previous_results = std::collections::HashMap::new();

    for (name, handler_def) in handlers_to_execute {

        let result = runner
            .execute_handler(name.clone(), handler_def, &contract_addr, &previous_results)
            .await;

        match result {
            Ok(handler_result) => {
                let json_result = if let Some(value) = &handler_result.value {
                    serde_json::json!({
                        "success": true,
                        "value": value,
                        "error": &handler_result.error,
                    })
                } else {
                    serde_json::json!({
                        "success": false,
                        "value": null,
                        "error": handler_result.error.as_ref().unwrap_or(&"No value returned".to_string()),
                    })
                };

                // Store result for potential dependencies
                previous_results.insert(name.clone(), handler_result);
                results.insert(name, json_result);
            }
            Err(e) => {
                results.insert(
                    name,
                    serde_json::json!({
                        "success": false,
                        "error": format!("{:?}", e),
                    }),
                );
            }
        }
    }

    // Return formatted results
    let output = serde_json::json!({
        "contract_address": contract_address,
        "handlers_executed": names.len(),
        "results": results,
    });

    serde_json::to_string_pretty(&output)
        .map_err(|e| rig::tool::ToolError::ToolCallError(e.into()))
}

