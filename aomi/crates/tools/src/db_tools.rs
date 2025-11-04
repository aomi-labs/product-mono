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

/// Retrieves contract ABI from the database
///
/// This tool uses tokio::spawn to run the implementation in a separate task.
/// This is necessary because database operations (sqlx) use interior mutability and are
/// not Sync, which means the store reference cannot be passed between threads. The Tool
/// trait requires Send + Sync bounds, so we spawn a new task to handle the async database
/// operations independently.
#[derive(Debug, Clone)]
pub struct GetContractABI;

/// Retrieves contract source code from the database
///
/// This tool uses tokio::spawn to run the implementation in a separate task.
/// This is necessary because database operations (sqlx) use interior mutability and are
/// not Sync, which means the store reference cannot be passed between threads. The Tool
/// trait requires Send + Sync bounds, so we spawn a new task to handle the async database
/// operations independently.
#[derive(Debug, Clone)]
pub struct GetContractSourceCode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContractArgs {
    pub chain_id: u32,
    pub address: String,
}

impl Tool for GetContractABI {
    const NAME: &'static str = "get_contract_abi";

    type Error = rig::tool::ToolError;
    type Args = GetContractArgs;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        info!("GetContractABI::definition called");
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Retrieves smart contract ABI from the database. Use this to fetch the contract's ABI for interaction or analysis. If the contract wasn't found in the database this will fetch it from etherscan and store the results in the database before returning the ABI.".to_string(),
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
        info!("get_contract_abi tool called with args: {:?}", args);

        let result = tokio::spawn(get_contract_abi_impl(args.chain_id, args.address))
            .await
            .map_err(|e| {
                let error_msg = format!("Task join error: {}", e);
                error!("{}", error_msg);
                rig::tool::ToolError::ToolCallError(error_msg.into())
            })?;

        match &result {
            Ok(_) => info!("get_contract_abi succeeded"),
            Err(e) => error!("get_contract_abi failed: {:?}", e),
        }

        result
    }
}

impl Tool for GetContractSourceCode {
    const NAME: &'static str = "get_contract_source_code";

    type Error = rig::tool::ToolError;
    type Args = GetContractArgs;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        info!("GetContractSourceCode::definition called");
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Retrieves smart contract source code from the database. Use this to fetch the contract's source code for analysis or review. If the contract wasn't found in the database this will fetch it from etherscan and store the results in the database before returning the source code.".to_string(),
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
        info!("get_contract_source_code tool called with args: {:?}", args);

        let result = tokio::spawn(get_contract_source_code_impl(args.chain_id, args.address))
            .await
            .map_err(|e| {
                let error_msg = format!("Task join error: {}", e);
                error!("{}", error_msg);
                rig::tool::ToolError::ToolCallError(error_msg.into())
            })?;

        match &result {
            Ok(_) => info!("get_contract_source_code succeeded"),
            Err(e) => error!("get_contract_source_code failed: {:?}", e),
        }

        result
    }
}

async fn get_contract_abi_impl(
    chain_id: u32,
    address: String,
) -> Result<serde_json::Value, ToolError> {
    info!(
        "get_contract_abi called with chain_id={}, address={}",
        chain_id, address
    );

    let contract = get_or_fetch_contract(chain_id, address).await?;

    Ok(json!({
        "found": true,
        "address": contract.address,
        "chain": contract.chain,
        "chain_id": contract.chain_id,
        "abi": contract.abi,
        "fetched_from_etherscan": contract.fetched_from_etherscan,
    }))
}

async fn get_contract_source_code_impl(
    chain_id: u32,
    address: String,
) -> Result<serde_json::Value, ToolError> {
    info!(
        "get_contract_source_code called with chain_id={}, address={}",
        chain_id, address
    );

    let contract = get_or_fetch_contract(chain_id, address).await?;

    Ok(json!({
        "found": true,
        "address": contract.address,
        "chain": contract.chain,
        "chain_id": contract.chain_id,
        "source_code": contract.source_code,
        "fetched_from_etherscan": contract.fetched_from_etherscan,
    }))
}

struct ContractData {
    address: String,
    chain: String,
    chain_id: u32,
    source_code: String,
    abi: serde_json::Value,
    fetched_from_etherscan: bool,
}

async fn get_or_fetch_contract(
    chain_id: u32,
    address: String,
) -> Result<ContractData, ToolError> {
    // Normalize address to lowercase for database lookup
    let address = address.to_lowercase();
    debug!("Normalized address to lowercase: {}", address);

    // Connect to database
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aomi@localhost:5432/chatbot".to_string());

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
            Ok(ContractData {
                address: c.address,
                chain: c.chain,
                chain_id: c.chain_id,
                source_code: c.source_code,
                abi: c.abi,
                fetched_from_etherscan: false,
            })
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

            Ok(ContractData {
                address: fetched_contract.address,
                chain: fetched_contract.chain,
                chain_id: fetched_contract.chain_id,
                source_code: fetched_contract.source_code,
                abi: fetched_contract.abi,
                fetched_from_etherscan: true,
            })
        }
    }
}
