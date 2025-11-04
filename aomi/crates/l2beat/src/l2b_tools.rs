use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;

use rig::tool::ToolError;
use rig_derive::rig_tool;
use baml_client::apis::{configuration::Configuration, default_api};
use baml_client::models::{AnalyzeAbiRequest, AnalyzeEventRequest, AnalyzeLayoutRequest};
use crate::etherscan::{EtherscanClient, Network};
use alloy_primitives::Address as AlloyAddress;
use alloy_provider::{network::AnyNetwork, RootProvider};
use crate::runner::DiscoveryRunner;
use std::str::FromStr;

use crate::handlers::config::HandlerDefinition;
use aomi_tools::impl_rig_tool_clone;

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
pub async fn analyze_abi_to_call_handler(
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
    let contract_info = crate::adapter::etherscan_to_contract_info(etherscan_results, None)
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
    let definitions = crate::adapter::abi_analysis_to_call_handlers(result.clone());

    // Populate global handler map
    let handlers_map: HashMap<String, HandlerDefinition> = definitions.iter().map(|(name, def)| (name.clone(), def.clone())).collect();
    let mut map = HANDLER_MAP.lock().await;
    map.extend(handlers_map.clone());


    // Return formatted result
    let output = serde_json::json!({
        "summary": result.summary,
        "handler_count": handlers_map.len(),
        "handlers": handlers_map,
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
pub async fn analyze_events_to_event_handler(
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
    let contract_info = crate::adapter::etherscan_to_contract_info(etherscan_results, None)
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
    let definitions = crate::adapter::event_analysis_to_event_handlers(result.clone());

    // Populate global handler map
    let handlers_map: HashMap<String, HandlerDefinition> = definitions.iter().map(|(name, def)| (name.clone(), def.clone())).collect();
    let mut map = HANDLER_MAP.lock().await;
    map.extend(handlers_map.clone());

    // Return formatted result
    let output = serde_json::json!({
        "summary": result.summary,
        "handler_count": handlers_map.len(),
        "handlers": handlers_map,
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
    description = "Analyze smart contract storage layout to generate Storage and DynamicArray handler definitions. Fetches contract data from Etherscan and populates the global handler map. ALWAYS print the raw output of this call to show complete analysis details including all handler definitions.",
    params(
        contract_address = "Ethereum contract address",
        intent = "What storage slots you want to access (e.g., 'Access validator mappings and stake amounts')"
    )
)]
pub async fn analyze_layout_to_storage_handler(
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
    let contract_info = crate::adapter::etherscan_to_contract_info(etherscan_results, None)
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
    let handlers = crate::adapter::layout_analysis_to_storage_handlers(result.clone())
        .map_err(|e| ToolError::ToolCallError(format!("Handler conversion failed: {}", e).into()))?;

    // Populate global handler map
    let handlers_map: HashMap<String, HandlerDefinition> = handlers.iter().map(|(name, def)| (name.clone(), def.clone())).collect();
    let mut map = HANDLER_MAP.lock().await;
    map.extend(handlers_map.clone());


    // Return formatted result with handler definitions
    let output = serde_json::json!({
        "contract_name": result.contract_name,
        "summary": result.summary,
        "handler_count": handlers_map.len(),
        "handlers": handlers_map,
        "inheritance": result.inheritance,
        "detected_constants": result.detected_constants,
        "warnings": result.warnings,
    });

    serde_json::to_string_pretty(&output)
        .map_err(|e| ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 3.5: Get Saved Handlers
// ============================================================================
#[rig_tool(
    description = "Get the names and parameters of all saved handlers.",
)]
pub async fn get_saved_handlers() -> Result<String, rig::tool::ToolError> {
    let map: tokio::sync::MutexGuard<'_, HashMap<String, HandlerDefinition>> = HANDLER_MAP.lock().await;
    let handlers: Vec<(String, String)> = map.iter().map(
        |(name, def)| ((name.clone(), serde_json::to_string(&def).unwrap()))
    ).collect();
    println!("Handlers: {:?}", handlers);
    serde_json::to_string_pretty(&handlers)
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
    description = "Execute previously generated handlers by their names. Retrieves handler definitions from the global map and executes them against the specified contract. Returns the extracted data for all handler types (Call, Storage, Event, AccessControl, DynamicArray).",
    params(
        contract_address = "Ethereum contract address to query",
        handler_names = "Comma-separated list of handler names to execute (e.g., 'owner,totalSupply,validators')",
    )
)]
pub async fn execute_handler(
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
    let rpc = std::env::var("ETH_RPC_URL").expect("Set ETH_RPC_URL when running with EXECUTE=true");
    let rpc_url = rpc.parse().expect("ETH_RPC_URL must be a valid URL");
    let provider = RootProvider::<AnyNetwork>::new_http(rpc_url);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_all_handlers_and_execute() {
        let contract_address = "0x3Cd52B238Ac856600b22756133eEb31ECb25109a".to_string();
        
        let mut all_handler_names = Vec::new();

        // Step 1: Analyze ABI to generate call handlers
        println!("=== Step 1: Analyzing ABI ===");
        // match analyze_abi_to_call_handler(contract_address.clone(), Some("Get token data".to_string())).await {
        //     Ok(abi_result) => {
        //         println!("ABI analysis result: {}", abi_result);
                
        //         let parsed: serde_json::Value = serde_json::from_str(&abi_result)
        //             .expect("Should be valid JSON");
                
        //         if let Some(handlers) = parsed.get("handlers") {
        //             if let Some(handler_map) = handlers.as_object() {
        //                 let handler_names: Vec<String> = handler_map.keys()
        //                     .take(2) // Limit to avoid long execution
        //                     .map(|k| k.clone())
        //                     .collect();
        //                 all_handler_names.extend(handler_names);
        //                 println!("Generated {} ABI handlers", handler_map.len());
        //             }
        //         }
        //     }
        //     Err(e) => {
        //         println!("ABI analysis failed (expected if BAML server not available): {}", e);
        //     }
        // }

        // Step 2: Analyze Events to generate event handlers
        println!("\n=== Step 2: Analyzing Events ===");
        match analyze_events_to_event_handler(contract_address.clone(), Some("Track token transfers".to_string())).await {
            Ok(events_result) => {
                println!("Events analysis result: {}", events_result);
                
                let parsed: serde_json::Value = serde_json::from_str(&events_result)
                    .expect("Should be valid JSON");
                
                if let Some(handlers) = parsed.get("handlers") {
                    if let Some(handler_map) = handlers.as_object() {
                        let handler_names: Vec<String> = handler_map.keys()
                            .take(2) // Limit to avoid long execution
                            .map(|k| k.clone())
                            .collect();
                        all_handler_names.extend(handler_names);
                        println!("Generated {} Event handlers", handler_map.len());
                    }
                }
            }
            Err(e) => {
                println!("Events analysis failed (expected if BAML server not available): {}", e);
            }
        }

        // Step 3: Analyze Storage Layout to generate storage handlers
        println!("\n=== Step 3: Analyzing Storage Layout ===");
        // match analyze_layout_to_storage_handler(contract_address.clone(), "Access token storage slots and mappings".to_string()).await {
        //     Ok(layout_result) => {
        //         println!("Layout analysis result: {}", layout_result);
                
        //         let parsed: serde_json::Value = serde_json::from_str(&layout_result)
        //             .expect("Should be valid JSON");
                
        //         if let Some(handlers) = parsed.get("handlers") {
        //             if let Some(handler_map) = handlers.as_object() {
        //                 let handler_names: Vec<String> = handler_map.keys()
        //                     .take(2) // Limit to avoid long execution
        //                     .map(|k| k.clone())
        //                     .collect();
        //                 all_handler_names.extend(handler_names);
        //                 println!("Generated {} Storage handlers", handler_map.len());
        //             }
        //         }
        //     }
        //     Err(e) => {
        //         println!("Layout analysis failed (expected if BAML server not available): {}", e);
        //     }
        // }

        // Step 4: Check saved handlers
        println!("\n=== Step 4: Checking Saved Handlers ===");
        match get_saved_handlers().await {
            Ok(saved_result) => {
                println!("All saved handlers: {}", saved_result);
            }
            Err(e) => {
                println!("Failed to get saved handlers: {}", e);
            }
        }

        // Step 5: Execute all generated handlers if any exist
        if !all_handler_names.is_empty() {
            println!("\n=== Step 5: Executing All Handlers ===");
            let handler_names_str = all_handler_names.join(",");
            println!("Executing handlers: {}", handler_names_str);
            
            match execute_handlers_impl(contract_address.clone(), handler_names_str).await {
                Ok(execution_result) => {
                    println!("Handler execution result: {}", execution_result);
                    
                    // Verify the execution result contains expected fields
                    let exec_parsed: serde_json::Value = serde_json::from_str(&execution_result)
                        .expect("Execution result should be valid JSON");
                    
                    assert!(exec_parsed.get("contract_address").is_some());
                    assert!(exec_parsed.get("handlers_executed").is_some());
                    assert!(exec_parsed.get("results").is_some());
                    
                    println!("✅ Successfully executed {} handlers", all_handler_names.len());
                }
                Err(e) => {
                    println!("Handler execution failed (this may be expected if RPC/ETH_RPC_URL not available): {}", e);
                }
            }
        } else {
            println!("\n=== No handlers were generated, skipping execution ===");
        }
    }

    #[tokio::test] 
    async fn test_get_saved_handlers() {
        // This test checks if we can retrieve saved handlers
        match get_saved_handlers().await {
            Ok(handlers_result) => {
                println!("Saved handlers: {}", handlers_result);
                
                let parsed: serde_json::Value = serde_json::from_str(&handlers_result)
                    .expect("Should be valid JSON");
                
                // Should be an array
                assert!(parsed.is_array());
            }
            Err(e) => {
                println!("Get saved handlers failed: {}", e);
            }
        }
    }
}

// Implement Clone for all rig_tool functions
impl_rig_tool_clone!(AnalyzeAbiToCallHandler, AnalyzeAbiToCallHandlerParameters, [contract_address, _intent]);
impl_rig_tool_clone!(AnalyzeEventsToEventHandler, AnalyzeEventsToEventHandlerParameters, [contract_address, _intent]);
impl_rig_tool_clone!(AnalyzeLayoutToStorageHandler, AnalyzeLayoutToStorageHandlerParameters, [contract_address, intent]);
impl_rig_tool_clone!(GetSavedHandlers, GetSavedHandlersParameters, []);
impl_rig_tool_clone!(ExecuteHandler, ExecuteHandlerParameters, [contract_address, handler_names]);

