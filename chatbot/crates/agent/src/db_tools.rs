use rig_derive::rig_tool;
use serde_json::json;
use sqlx::any::AnyPoolOptions;

use crate::db::{Contract, ContractStore, ContractStoreApi};

/// Retrieves contract information including source code and ABI from the database
#[rig_tool(
    description = "Retrieves smart contract information from the database including source code and ABI. Use this to fetch previously stored contract details for analysis or interaction.",
    params(
        chain_id = "The chain ID as an integer (e.g., 1 for Ethereum, 137 for Polygon, 42161 for Arbitrum)",
        address = "The contract's address on the blockchain (e.g., \"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48\"). Must be a valid hexadecimal address starting with 0x"
    ),
    required(chain_id, address)
)]
pub fn get_contract_info(
    chain_id: u32,
    address: String,
) -> Result<serde_json::Value, rig::tool::ToolError> {
    // Spawn a tokio task to handle the async operation
    let handle = tokio::spawn(async move {
        // Connect to database
        sqlx::any::install_default_drivers();
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kevin@localhost:5432/chatbot".to_string());

        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .map_err(|e| rig::tool::ToolError::ToolCallError(format!("Database connection error: {}", e).into()))?;

        let store = ContractStore::new(pool);

        // Get contract
        let contract = store
            .get_contract(chain_id, address.clone())
            .await
            .map_err(|e| rig::tool::ToolError::ToolCallError(format!("Failed to get contract: {}", e).into()))?;

        match contract {
            Some(c) => Ok(json!({
                "found": true,
                "address": c.address,
                "chain": c.chain,
                "chain_id": c.chain_id,
                "source_code": c.source_code,
                "abi": c.abi,
            })),
            None => Ok(json!({
                "found": false,
                "message": format!("Contract not found in DB for address {} on chain_id {}", address, chain_id)
            })),
        }
    });

    // Wait for the task to complete
    tokio::runtime::Handle::current()
        .block_on(handle)
        .map_err(|e| rig::tool::ToolError::ToolCallError(format!("Task join error: {}", e).into()))?
}

/// Stores or updates contract information including source code and ABI in the database
#[rig_tool(
    description = "Stores or updates smart contract information in the database including source code and ABI. Use this to save contract details for future reference and interaction.",
    params(
        chain_id = "The chain ID as an integer (e.g., 1 for Ethereum, 137 for Polygon, 42161 for Arbitrum)",
        address = "The contract's address on the blockchain (e.g., \"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48\"). Must be a valid hexadecimal address starting with 0x",
        source_code = "The complete Solidity source code of the contract",
        abi = "The contract ABI as a JSON string (array format)"
    ),
    required(chain_id, address, source_code, abi)
)]
pub fn store_contract_info(
    chain_id: u32,
    address: String,
    source_code: String,
    abi: String,
) -> Result<serde_json::Value, rig::tool::ToolError> {
    // Spawn a tokio task to handle the async operation
    let handle = tokio::spawn(async move {
        // Connect to database
        sqlx::any::install_default_drivers();
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kevin@localhost:5432/chatbot".to_string());

        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .map_err(|e| rig::tool::ToolError::ToolCallError(format!("Database connection error: {}", e).into()))?;

        let store = ContractStore::new(pool);

        // Parse the ABI JSON string
        let abi_json: serde_json::Value = serde_json::from_str(&abi)
            .map_err(|e| rig::tool::ToolError::ToolCallError(format!("Failed to parse ABI JSON: {}", e).into()))?;

        let chain = crate::etherscan::chain_id_to_name(chain_id);

        let contract = Contract {
            address: address.clone(),
            chain: chain.clone(),
            chain_id,
            source_code,
            abi: abi_json,
        };

        store
            .store_contract(contract)
            .await
            .map_err(|e| rig::tool::ToolError::ToolCallError(format!("Failed to store contract: {}", e).into()))?;

        Ok(json!({
            "success": true,
            "message": format!("Contract {} on chain {} (chain_id: {}) stored in DB successfully", address, chain, chain_id)
        }))
    });

    // Wait for the task to complete
    tokio::runtime::Handle::current()
        .block_on(handle)
        .map_err(|e| rig::tool::ToolError::ToolCallError(format!("Task join error: {}", e).into()))?
}

// Manual Clone implementations for the generated structs
impl Clone for GetContractInfo {
    fn clone(&self) -> Self {
        Self
    }
}

impl Clone for GetContractInfoParameters {
    fn clone(&self) -> Self {
        Self {
            chain_id: self.chain_id,
            address: self.address.clone(),
        }
    }
}

impl Clone for StoreContractInfo {
    fn clone(&self) -> Self {
        Self
    }
}

impl Clone for StoreContractInfoParameters {
    fn clone(&self) -> Self {
        Self {
            chain_id: self.chain_id,
            address: self.address.clone(),
            source_code: self.source_code.clone(),
            abi: self.abi.clone(),
        }
    }
}
