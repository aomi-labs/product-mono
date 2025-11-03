use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::any::AnyPoolOptions;

use crate::db::{ContractStore, ContractStoreApi};
use crate::etherscan::fetch_and_store_contract;

/// Retrieves contract information including source code and ABI from the database
///
/// This tool uses tokio::spawn to run the implementation in a separate task.
/// This is necessary because database operations (sqlx) use interior mutability and are
/// not Sync, which means the store reference cannot be passed between threads. The Tool
/// trait requires Send + Sync bounds, so we spawn a new task to handle the async database
/// operations independently.
#[derive(Debug, Clone)]
pub struct GetContractInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContractInfoArgs {
    pub chain_id: u32,
    pub address: String,
}

impl Tool for GetContractInfo {
    const NAME: &'static str = "get_contract_info";

    type Error = rig::tool::ToolError;
    type Args = GetContractInfoArgs;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Retrieves smart contract information from the database including source code and ABI. Use this to fetch previously stored contract details for analysis or interaction. If the contract wasn't found in the database this will fetch it from etherscan and store the results in the database before returning the contract info.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "chain_id": {
                        "type": "number",
                        "description": "The chain ID as an integer (e.g., 1 for Ethereum, 137 for Polygon, 42161 for Arbitrum)"
                    },
                    "address": {
                        "type": "string",
                        "description": "The contract's address on the blockchain (e.g., \"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48\"). Must be a valid hexadecimal address starting with 0x"
                    }
                },
                "required": ["chain_id", "address"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tokio::spawn(get_contract_info_impl(args.chain_id, args.address))
            .await
            .map_err(|e| {
                rig::tool::ToolError::ToolCallError(format!("Task join error: {}", e).into())
            })?
    }
}

async fn get_contract_info_impl(
    chain_id: u32,
    address: String,
) -> Result<serde_json::Value, ToolError> {
    // Connect to database
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kevin@localhost:5432/chatbot".to_string());

    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .map_err(|e| {
            rig::tool::ToolError::ToolCallError(format!("Database connection error: {}", e).into())
        })?;

    let store = ContractStore::new(pool);

    // Get contract
    let contract = store
        .get_contract(chain_id, address.clone())
        .await
        .map_err(|e| {
            rig::tool::ToolError::ToolCallError(format!("Failed to get contract: {}", e).into())
        })?;

    match contract {
        Some(c) => Ok(json!({
            "found": true,
            "address": c.address,
            "chain": c.chain,
            "chain_id": c.chain_id,
            "source_code": c.source_code,
            "abi": c.abi,
        })),
        None => {
            // Not found in DB, fetch from Etherscan and store
            let fetched_contract = fetch_and_store_contract(chain_id, address.clone(), &store)
                .await
                .map_err(|e| {
                    rig::tool::ToolError::ToolCallError(
                        format!("Failed to fetch from Etherscan: {}", e).into(),
                    )
                })?;

            Ok(json!({
                "found": true,
                "fetched_from_etherscan": true,
                "address": fetched_contract.address,
                "chain": fetched_contract.chain,
                "chain_id": fetched_contract.chain_id,
                "source_code": fetched_contract.source_code,
                "abi": fetched_contract.abi,
            }))
        }
    }
}
