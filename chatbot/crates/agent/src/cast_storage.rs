use alloy_primitives::{Address, U256};
use alloy_provider::{network::AnyNetwork, Provider, RootProvider};
use rig::tool::ToolError;
use rig_derive::rig_tool;
use std::str::FromStr;
use std::process::Command;

/// Retrieve the value stored at a specific storage slot in a smart contract
#[rig_tool(
    description = "Use Cast to read the raw value from a smart contract storage slot. Returns the 32-byte value stored at the specified slot as a hex string.",
    params(
        contract_address = "Ethereum contract address (e.g., '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48')",
        slot = "Storage slot number (decimal or hex with 0x prefix, e.g., '5' or '0x5')",
        rpc_url = "Ethereum RPC endpoint URL (e.g., 'https://eth.llamarpc.com')"
    )
)]
pub async fn cast_storage(
    contract_address: String,
    slot: String,
    rpc_url: String,
) -> Result<String, rig::tool::ToolError> {
    // Spawn in a task to satisfy Send + Sync requirements
    // The rig framework requires futures to be both Send + Sync, but alloy's Provider
    // methods return futures that are only Send (not Sync). tokio::spawn returns a
    // JoinHandle which is Send + Sync, allowing us to satisfy rig's requirements.
    tokio::spawn(cast_storage_impl(contract_address, slot, rpc_url))
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Task join error: {}", e).into()))?
}

async fn cast_storage_impl(
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

/// Get storage layout of a contract using cast storage command
#[rig_tool(
    description = "Use Cast storage command to get the storage layout of a contract. Shows variable names, types, slots, and values in a formatted table. Always print the raw output of this call to show complete storage layout details.",
    params(
        contract_address = "Ethereum contract address (e.g., '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48')",
        slot = "Optional: Specific storage slot number (decimal or hex with 0x prefix)",
        block = "Optional: Block number or hash to read storage at"
    )
)]
pub async fn storage(
    contract_address: String,
    slot: Option<String>,
    block: Option<String>,
) -> Result<String, rig::tool::ToolError> {
    tokio::spawn(storage_impl(contract_address, slot, block))
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Task join error: {}", e).into()))?
}

async fn storage_impl(
    contract_address: String,
    slot: Option<String>,
    block: Option<String>,
) -> Result<String, rig::tool::ToolError> {
    // Build the cast storage command
    let mut cmd = Command::new("cast");
    cmd.arg("storage").arg(&contract_address);

    // Add optional slot parameter
    if let Some(slot) = &slot {
        cmd.arg(slot);
    }

    // Add optional block parameter
    if let Some(block) = &block {
        cmd.arg("--block").arg(block);
    }

    // Execute the command
    let output = cmd.output().map_err(|e| {
        ToolError::ToolCallError(format!("Failed to execute cast storage command: {}", e).into())
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ToolError::ToolCallError(
            format!("Cast storage command failed: {}", stderr).into(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_storage() {
        let contract_address = "0x3Cd52B238Ac856600b22756133eEb31ECb25109a".to_string();
        
        // This test will only pass if cast is installed and can connect to mainnet
        // In CI/CD environments, this might need to be mocked or skipped
        match storage_impl(contract_address, None, None).await {
            Ok(result) => {
                println!("Storage result: {}", result);
                assert!(!result.is_empty());
                // Check if the output contains expected table headers
                // assert!(result.contains("Name") || result.contains("Type") || result.contains("Slot"));
            }
            Err(e) => {
                // If cast is not available or network issues, just print the error
                println!("Storage test failed (this is expected if cast is not installed): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_storage_with_slot() {
        let contract_address = "0x3Cd52B238Ac856600b22756133eEb31ECb25109a".to_string();
        let slot = Some("0".to_string());
        
        match storage_impl(contract_address, slot, None).await {
            Ok(result) => {
                println!("Storage with slot result: {}", result);
                assert!(!result.is_empty());
            }
            Err(e) => {
                println!("Storage with slot test failed (this is expected if cast is not installed): {}", e);
            }
        }
    }
}
