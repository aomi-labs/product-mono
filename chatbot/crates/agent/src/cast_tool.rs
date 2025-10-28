use alloy_primitives::{Address, U256};
use alloy_provider::{network::AnyNetwork, Provider, RootProvider};
use rig::tool::ToolError;
use rig_derive::rig_tool;
use std::str::FromStr;

/// Retrieve the value stored at a specific storage slot in a smart contract
#[rig_tool(
    description = "Read the raw value from a smart contract storage slot. Returns the 32-byte value stored at the specified slot as a hex string.",
    params(
        contract_address = "Ethereum contract address (e.g., '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48')",
        slot = "Storage slot number (decimal or hex with 0x prefix, e.g., '5' or '0x5')",
        rpc_url = "Ethereum RPC endpoint URL (e.g., 'https://eth.llamarpc.com')"
    )
)]
pub async fn get_storage(
    contract_address: String,
    slot: String,
    rpc_url: String,
) -> Result<String, rig::tool::ToolError> {
    // Spawn in a task to satisfy Send + Sync requirements
    // The rig framework requires futures to be both Send + Sync, but alloy's Provider
    // methods return futures that are only Send (not Sync). tokio::spawn returns a
    // JoinHandle which is Send + Sync, allowing us to satisfy rig's requirements.
    tokio::spawn(get_storage_impl(contract_address, slot, rpc_url))
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Task join error: {}", e).into()))?
}

async fn get_storage_impl(
    contract_address: String,
    slot: String,
    rpc_url: String,
) -> Result<String, rig::tool::ToolError> {
    // Parse contract address
    let address = Address::from_str(&contract_address)
        .map_err(|e| ToolError::ToolCallError(format!("Invalid contract address: {}", e).into()))?;

    // Parse slot number (supports both decimal and hex)
    let slot_u256 = if slot.starts_with("0x") {
        U256::from_str_radix(&slot[2..], 16)
            .map_err(|e| ToolError::ToolCallError(format!("Invalid hex slot number: {}", e).into()))?
    } else {
        U256::from_str_radix(&slot, 10)
            .map_err(|e| ToolError::ToolCallError(format!("Invalid decimal slot number: {}", e).into()))?
    };

    // Create provider
    let provider = RootProvider::<AnyNetwork>::new_http(
        rpc_url
            .parse()
            .map_err(|e| ToolError::ToolCallError(format!("Invalid RPC URL: {}", e).into()))?
    );

    // Read storage
    let storage_value = provider
        .get_storage_at(address, slot_u256)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Failed to read storage: {}", e).into()))?;

    // Return as hex string
    let result = serde_json::json!({
        "contract_address": contract_address,
        "slot": slot,
        "value": format!("0x{:064x}", storage_value),
        "value_decimal": storage_value.to_string(),
    });

    serde_json::to_string_pretty(&result)
        .map_err(|e| ToolError::ToolCallError(e.into()))
}
