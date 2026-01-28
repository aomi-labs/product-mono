use aomi_tools::clients::EtherscanClient;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use tokio::task;

use crate::handlers::config::HandlerDefinition;
use crate::runner::DiscoveryRunner;
use alloy_primitives::Address as AlloyAddress;
use aomi_anvil::default_provider;
use aomi_baml::baml_client::async_client::B;
use aomi_tools::etherscan::Network;
use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, with_topic};
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// Global handler map that gets populated by the analysis tools
static HANDLER_MAP: LazyLock<Mutex<HashMap<String, HandlerDefinition>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// ============================================================================
// Tool parameter types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeAbiToCallHandlerParameters {
    pub contract_address: String,
    pub intent: Option<String>,
}

impl AomiToolArgs for AnalyzeAbiToCallHandlerParameters {
    fn schema() -> serde_json::Value {
        with_topic(serde_json::json!({
            "type": "object",
            "properties": {
                "contract_address": { "type": "string" },
                "intent": { "type": "string" }
            },
            "required": ["contract_address"]
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeEventsToEventHandlerParameters {
    pub contract_address: String,
    pub intent: Option<String>,
}

impl AomiToolArgs for AnalyzeEventsToEventHandlerParameters {
    fn schema() -> serde_json::Value {
        with_topic(serde_json::json!({
            "type": "object",
            "properties": {
                "contract_address": { "type": "string" },
                "intent": { "type": "string" }
            },
            "required": ["contract_address"]
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeLayoutToStorageHandlerParameters {
    pub contract_address: String,
    pub intent: String,
}

impl AomiToolArgs for AnalyzeLayoutToStorageHandlerParameters {
    fn schema() -> serde_json::Value {
        with_topic(serde_json::json!({
            "type": "object",
            "properties": {
                "contract_address": { "type": "string" },
                "intent": { "type": "string" }
            },
            "required": ["contract_address", "intent"]
        }))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetSavedHandlersParameters {}

impl AomiToolArgs for GetSavedHandlersParameters {
    fn schema() -> serde_json::Value {
        with_topic(serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteHandlerParameters {
    pub contract_address: String,
    pub handler_names: String,
}

impl AomiToolArgs for ExecuteHandlerParameters {
    fn schema() -> serde_json::Value {
        with_topic(serde_json::json!({
            "type": "object",
            "properties": {
                "contract_address": { "type": "string" },
                "handler_names": { "type": "string" }
            },
            "required": ["contract_address", "handler_names"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AnalyzeAbiToCallHandler;

#[derive(Debug, Clone)]
pub struct AnalyzeEventsToEventHandler;

#[derive(Debug, Clone)]
pub struct AnalyzeLayoutToStorageHandler;

#[derive(Debug, Clone)]
pub struct GetSavedHandlers;

#[derive(Debug, Clone)]
pub struct ExecuteHandler;

// ============================================================================
// Tool 1: Analyze ABI
// ============================================================================

pub async fn analyze_abi_to_call_handler(
    contract_address: String,
    intent: Option<String>,
) -> Result<String, rig::tool::ToolError> {
    // Fetch contract data from Etherscan
    let etherscan = EtherscanClient::from_env()
        .map_err(|e| ToolError::ToolCallError(format!("Etherscan client error: {}", e).into()))?;

    let contract = etherscan
        .fetch_contract(Network::Mainnet, &contract_address)
        .await
        .map_err(|e| {
            ToolError::ToolCallError(format!("Failed to fetch from Etherscan: {}", e).into())
        })?;

    // Convert to ContractInfo
    let contract_info =
        crate::adapter::etherscan_to_contract_info(contract, None).map_err(|e| {
            ToolError::ToolCallError(format!("Failed to convert contract info: {}", e).into())
        })?;

    // Call BAML function via native FFI (no HTTP server needed)
    let result = B
        .AnalyzeABI
        .with_client(aomi_baml::AomiModel::ClaudeOpus4.baml_client_name())
        .call(&contract_info, intent)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("BAML call failed: {:?}", e).into()))?;

    // Convert to handler definitions
    let definitions = crate::adapter::abi_analysis_to_call_handlers(result.clone());

    // Populate global handler map
    let handlers_map: HashMap<String, HandlerDefinition> = definitions
        .iter()
        .map(|(name, def)| (name.clone(), def.clone()))
        .collect();
    let mut map = HANDLER_MAP.lock().await;
    map.extend(handlers_map.clone());

    // Return formatted result
    let output = serde_json::json!({
        "summary": result.summary,
        "handler_count": handlers_map.len(),
        "handlers": handlers_map,
    });

    serde_json::to_string_pretty(&output).map_err(|e| ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 2: Analyze Events
// ============================================================================

pub async fn analyze_events_to_event_handler(
    contract_address: String,
    intent: Option<String>,
) -> Result<String, rig::tool::ToolError> {
    // Fetch contract data from Etherscan
    let etherscan = EtherscanClient::from_env()
        .map_err(|e| ToolError::ToolCallError(format!("Etherscan client error: {}", e).into()))?;

    let contract = etherscan
        .fetch_contract(Network::Mainnet, &contract_address)
        .await
        .map_err(|e| {
            ToolError::ToolCallError(format!("Failed to fetch from Etherscan: {}", e).into())
        })?;

    // Convert to ContractInfo
    let contract_info =
        crate::adapter::etherscan_to_contract_info(contract, None).map_err(|e| {
            ToolError::ToolCallError(format!("Failed to convert contract info: {}", e).into())
        })?;

    // First, analyze ABI to get events (native FFI - no HTTP)
    let abi_result = B
        .AnalyzeABI
        .with_client(aomi_baml::AomiModel::ClaudeOpus4.baml_client_name())
        .call(&contract_info, intent.clone())
        .await
        .map_err(|e| ToolError::ToolCallError(format!("ABI analysis failed: {:?}", e).into()))?;

    // Then analyze events (native FFI - no HTTP)
    let result = B
        .AnalyzeEvent
        .with_client(aomi_baml::AomiModel::ClaudeOpus4.baml_client_name())
        .call(&contract_info, &abi_result, intent)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Event analysis failed: {:?}", e).into()))?;

    // Convert to handler definitions
    let definitions = crate::adapter::event_analysis_to_event_handlers(result.clone());

    // Populate global handler map
    let handlers_map: HashMap<String, HandlerDefinition> = definitions
        .iter()
        .map(|(name, def)| (name.clone(), def.clone()))
        .collect();
    let mut map = HANDLER_MAP.lock().await;
    map.extend(handlers_map.clone());

    // Return formatted result (event_actions converted via handlers_map, not serialized directly)
    let output = serde_json::json!({
        "summary": result.summary,
        "handler_count": handlers_map.len(),
        "handlers": handlers_map,
        "event_action_count": result.event_actions.len(),
        "detected_constants": result.detected_constants,
        "warnings": result.warnings,
    });

    serde_json::to_string_pretty(&output).map_err(|e| ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 3: Analyze Storage Layout
// ============================================================================

pub async fn analyze_layout_to_storage_handler(
    contract_address: String,
    intent: String,
) -> Result<String, rig::tool::ToolError> {
    // Fetch contract data from Etherscan
    let etherscan = EtherscanClient::from_env()
        .map_err(|e| ToolError::ToolCallError(format!("Etherscan client error: {}", e).into()))?;

    let contract = etherscan
        .fetch_contract(Network::Mainnet, &contract_address)
        .await
        .map_err(|e| {
            ToolError::ToolCallError(format!("Failed to fetch from Etherscan: {}", e).into())
        })?;

    // Convert to ContractInfo
    let contract_info =
        crate::adapter::etherscan_to_contract_info(contract, None).map_err(|e| {
            ToolError::ToolCallError(format!("Failed to convert contract info: {}", e).into())
        })?;

    // First, analyze ABI (native FFI - no HTTP)
    let abi_result = B
        .AnalyzeABI
        .with_client(aomi_baml::AomiModel::ClaudeOpus4.baml_client_name())
        .call(&contract_info, Some(&intent))
        .await
        .map_err(|e| ToolError::ToolCallError(format!("ABI analysis failed: {:?}", e).into()))?;

    // Then analyze layout (native FFI - no HTTP)
    let result = B
        .AnalyzeLayout
        .with_client(aomi_baml::AomiModel::ClaudeOpus4.baml_client_name())
        .call(&contract_info, &abi_result, &intent)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Layout analysis failed: {:?}", e).into()))?;

    // Convert to handler definitions
    let handlers =
        crate::adapter::layout_analysis_to_storage_handlers(result.clone()).map_err(|e| {
            ToolError::ToolCallError(format!("Handler conversion failed: {}", e).into())
        })?;

    // Populate global handler map
    let handlers_map: HashMap<String, HandlerDefinition> = handlers
        .iter()
        .map(|(name, def)| (name.clone(), def.clone()))
        .collect();
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

    serde_json::to_string_pretty(&output).map_err(|e| ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 3.5: Get Saved Handlers
// ============================================================================
pub async fn get_saved_handlers() -> Result<String, rig::tool::ToolError> {
    let map: tokio::sync::MutexGuard<'_, HashMap<String, HandlerDefinition>> =
        HANDLER_MAP.lock().await;
    let handlers: Vec<(String, String)> = map
        .iter()
        .map(|(name, def)| (name.clone(), serde_json::to_string(&def).unwrap()))
        .collect();
    println!("Handlers: {:?}", handlers);
    serde_json::to_string_pretty(&handlers).map_err(|e| ToolError::ToolCallError(e.into()))
}

// ============================================================================
// Tool 4: Execute Handlers
// ============================================================================
// NOTE: Commented out due to Sync requirement incompatibility
// The async_trait-generated futures from Handler::execute() are only Send, not Sync
// But rig requires Send + Sync futures. This works in MCP which doesn't have Sync requirement.
// TODO: Either modify Handler trait to use manual async or use tokio::spawn wrapper
pub async fn execute_handler(
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
        return Err(ToolError::ToolCallError("No handler names provided".into()));
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
    let provider = default_provider()
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Load providers.toml: {}", e).into()))?;
    let contract_addr = AlloyAddress::from_str(&contract_address)
        .map_err(|e| ToolError::ToolCallError(format!("Invalid address: {}", e).into()))?;

    // Create DiscoveryRunner
    let runner = DiscoveryRunner::new(Network::Mainnet, provider).map_err(|e| {
        ToolError::ToolCallError(format!("Failed to create DiscoveryRunner: {}", e).into())
    })?;

    // Execute handlers
    let mut results = serde_json::Map::new();
    let mut previous_results = std::collections::HashMap::new();

    for (name, handler_def) in handlers_to_execute {
        let result = task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(runner.execute_handler(
                name.clone(),
                handler_def,
                &contract_addr,
                &previous_results,
            ))
        });

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

    serde_json::to_string_pretty(&output).map_err(|e| rig::tool::ToolError::ToolCallError(e.into()))
}

impl AomiTool for AnalyzeAbiToCallHandler {
    const NAME: &'static str = "analyze_abi_to_call_handler";
    const NAMESPACE: &'static str = "l2beat";

    type Args = AnalyzeAbiToCallHandlerParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Analyze a smart contract's ABI to identify view/pure functions and generate Call handler definitions."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            analyze_abi_to_call_handler(args.contract_address, args.intent)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for AnalyzeEventsToEventHandler {
    const NAME: &'static str = "analyze_events_to_event_handler";
    const NAMESPACE: &'static str = "l2beat";

    type Args = AnalyzeEventsToEventHandlerParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Analyze smart contract events to generate Event handler definitions."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            analyze_events_to_event_handler(args.contract_address, args.intent)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for AnalyzeLayoutToStorageHandler {
    const NAME: &'static str = "analyze_layout_to_storage_handler";
    const NAMESPACE: &'static str = "l2beat";

    type Args = AnalyzeLayoutToStorageHandlerParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Analyze smart contract storage layout to generate Storage handler definitions."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            analyze_layout_to_storage_handler(args.contract_address, args.intent)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for GetSavedHandlers {
    const NAME: &'static str = "get_saved_handlers";
    const NAMESPACE: &'static str = "l2beat";

    type Args = GetSavedHandlersParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Get the names and parameters of all saved handlers."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            get_saved_handlers()
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for ExecuteHandler {
    const NAME: &'static str = "execute_handler";
    const NAMESPACE: &'static str = "l2beat";

    type Args = ExecuteHandlerParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Execute previously generated handlers by their names."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_handler(args.contract_address, args.handler_names)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
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
        match analyze_events_to_event_handler(
            contract_address.clone(),
            Some("Track token transfers".to_string()),
        )
        .await
        {
            Ok(events_result) => {
                println!("Events analysis result: {}", events_result);

                let parsed: serde_json::Value =
                    serde_json::from_str(&events_result).expect("Should be valid JSON");

                if let Some(handlers) = parsed.get("handlers")
                    && let Some(handler_map) = handlers.as_object()
                {
                    let handler_names: Vec<String> = handler_map
                        .keys()
                        .take(2) // Limit to avoid long execution
                        .cloned()
                        .collect();
                    all_handler_names.extend(handler_names);
                    println!("Generated {} Event handlers", handler_map.len());
                }
            }
            Err(e) => {
                println!(
                    "Events analysis failed (expected if BAML server not available): {}",
                    e
                );
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

            match execute_handler(contract_address.clone(), handler_names_str).await {
                Ok(execution_result) => {
                    println!("Handler execution result: {}", execution_result);

                    // Verify the execution result contains expected fields
                    let exec_parsed: serde_json::Value = serde_json::from_str(&execution_result)
                        .expect("Execution result should be valid JSON");

                    assert!(exec_parsed.get("contract_address").is_some());
                    assert!(exec_parsed.get("handlers_executed").is_some());
                    assert!(exec_parsed.get("results").is_some());

                    println!(
                        "✅ Successfully executed {} handlers",
                        all_handler_names.len()
                    );
                }
                Err(e) => {
                    println!(
                        "Handler execution failed (this may be expected if providers.toml is missing): {}",
                        e
                    );
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

                let parsed: serde_json::Value =
                    serde_json::from_str(&handlers_result).expect("Should be valid JSON");

                // Should be an array
                assert!(parsed.is_array());
            }
            Err(e) => {
                println!("Get saved handlers failed: {}", e);
            }
        }
    }
}
