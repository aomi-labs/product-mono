use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::any::AnyPoolOptions;
use tracing::{debug, error, info};

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
        info!("GetContractInfo::definition called");
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
        info!("get_contract_info tool called with args: {:?}", args);

        let result = tokio::spawn(get_contract_info_impl(args.chain_id, args.address))
            .await
            .map_err(|e| {
                let error_msg = format!("Task join error: {}", e);
                error!("{}", error_msg);
                rig::tool::ToolError::ToolCallError(error_msg.into())
            })?;

        match &result {
            Ok(_) => info!("get_contract_info succeeded"),
            Err(e) => error!("get_contract_info failed: {:?}", e),
        }

        result
    }
}

async fn get_contract_info_impl(
    chain_id: u32,
    address: String,
) -> Result<serde_json::Value, ToolError> {
    info!(
        "get_contract_info called with chain_id={}, address={}",
        chain_id, address
    );

    // Normalize address to lowercase for database lookup
    let address = address.to_lowercase();
    debug!("Normalized address to lowercase: {}", address);

    // Connect to database
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kevin@localhost:5432/chatbot".to_string());

    debug!("Connecting to database: {}", database_url);

    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .map_err(|e| {
            let error_msg = format!("Database connection error: {}", e);
            error!("{}", error_msg);
            rig::tool::ToolError::ToolCallError(error_msg.into())
        })?;

    debug!("Database connection successful");

    let store = ContractStore::new(pool);

    // Get contract
    debug!("Querying database for contract");
    let contract = store
        .get_contract(chain_id, address.clone())
        .await
        .map_err(|e| {
            let error_msg = format!("Failed to query contract from database: {}", e);
            error!("{}", error_msg);
            rig::tool::ToolError::ToolCallError(error_msg.into())
        })?;

    match contract {
        Some(c) => {
            info!("Contract found in database: {}", c.address);
            Ok(json!({
                "found": true,
                "address": c.address,
                "chain": c.chain,
                "chain_id": c.chain_id,
                "source_code": c.source_code,
                "abi": c.abi,
            }))
        }
        None => {
            info!("Contract not found in database, fetching from Etherscan");
            // Not found in DB, fetch from Etherscan and store
            let fetched_contract = fetch_and_store_contract(chain_id, address.clone(), &store)
                .await
                .map_err(|e| {
                    let error_msg = format!("Failed to fetch from Etherscan: {}", e);
                    error!("{}", error_msg);
                    rig::tool::ToolError::ToolCallError(error_msg.into())
                })?;

            info!(
                "Successfully fetched contract from Etherscan: {}",
                fetched_contract.address
            );

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
