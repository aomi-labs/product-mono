use rig::tool::ToolError;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::any::AnyPoolOptions;
use std::future::Future;
use tokio::task;
use tracing::{debug, error, info};

use crate::db::{ContractStore, ContractStoreApi, ContractSearchParams};
use crate::etherscan::fetch_and_store_contract;

/// Retrieves contract ABI from the database
#[derive(Debug, Clone)]
pub struct GetContractABI;

/// Retrieves contract source code from the database
#[derive(Debug, Clone)]
pub struct GetContractSourceCode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContractArgs {
    /// One-line note on what this contract fetch is for
    pub topic: String,

    // Search parameters - at least one should be provided
    pub chain_id: Option<u32>,
    pub address: Option<String>,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub protocol: Option<String>,
    pub contract_type: Option<String>,
    pub version: Option<String>,
    pub tags: Option<String>,
}

fn run_sync<F, T>(future: F) -> Result<T, ToolError>
where
    F: Future<Output = Result<T, ToolError>> + Send + 'static,
    T: Send + 'static,
{
    task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

pub async fn execute_get_contract_abi(
args: GetContractArgs,) -> Result<serde_json::Value, ToolError> {
    info!("get_contract_abi tool called with args: {:?}", args);
    let contract =
        run_sync(async move { get_or_fetch_contract(args.chain_id, args.address).await })?;
    info!("get_contract_abi succeeded");
    Ok(json!({
        "found": true,
        "address": contract.address,
        "chain": contract.chain,
        "chain_id": contract.chain_id,
        "abi": contract.abi,
        "fetched_from_etherscan": contract.fetched_from_etherscan,
    }))
}

pub async fn execute_get_contract_source_code(
    args: GetContractArgs,
) -> Result<serde_json::Value, ToolError> {
    info!("get_contract_source_code tool called with args: {:?}", args);

    let contract =
        run_sync(async move { get_or_fetch_contract(args.chain_id, args.address).await })?;

    info!("get_contract_source_code succeeded");
    Ok(json!({
        "found": true,
        "address": contract.address,
        "chain": contract.chain,
        "chain_id": contract.chain_id,
        "source_code": contract.source_code,
        "fetched_from_etherscan": contract.fetched_from_etherscan,
    }))
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
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this contract info is for"
                    },
                    "chain_id": {
                        "type": "number",
                        "description": "The chain ID as an integer (e.g., 1 for Ethereum, 137 for Polygon, 42161 for Arbitrum)"
                    },
                    "address": {
                        "type": "string",
                        "description": "The contract's address on the blockchain (e.g., \"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48\"). Must be a valid hexadecimal address starting with 0x"
                    }
                },
                "required": ["topic", "chain_id", "address"]
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
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this contract info is for"
                    },
                    "chain_id": {
                        "type": "number",
                        "description": "The chain ID as an integer (e.g., 1 for Ethereum, 137 for Polygon, 42161 for Arbitrum)"
                    },
                    "address": {
                        "type": "string",
                        "description": "The contract's address on the blockchain (e.g., \"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48\"). Must be a valid hexadecimal address starting with 0x"
                    }
                },
                "required": ["topic", "chain_id", "address"]
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

    info!("get_contract_abi succeeded");
    Ok(json!({
        "found": !contracts.is_empty(),
        "count": contracts.len(),
        "contracts": contracts.iter().map(|c| json!({
            "address": c.address,
            "chain": c.chain,
            "chain_id": c.chain_id,
            "abi": c.abi,
        })).collect::<Vec<_>>(),
        "fetched_from_etherscan": fetched_from_etherscan,
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

    info!("get_contract_source_code succeeded");
    Ok(json!({
        "found": !contracts.is_empty(),
        "count": contracts.len(),
        "contracts": contracts.iter().map(|c| json!({
            "address": c.address,
            "chain": c.chain,
            "chain_id": c.chain_id,
            "source_code": c.source_code,
        })).collect::<Vec<_>>(),
        "fetched_from_etherscan": fetched_from_etherscan,
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

async fn search_contracts(args: GetContractArgs) -> Result<Vec<ContractData>, ToolError> {
    debug!("Searching contracts with args: {:?}", args);

    // Connect to database
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://ceciliazhang@localhost:5432/chatbot".to_string());

    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .map_err(|e| {
            let error_msg = format!("Database connection error: {}", e);
            error!("{}", error_msg);
            rig::tool::ToolError::ToolCallError(error_msg.into())
        })?;

    let store = ContractStore::new(pool);

    // Build search params
    let search_params = ContractSearchParams {
        chain_id: args.chain_id,
        address: args.address,
        name: args.name,
        symbol: args.symbol,
        protocol: args.protocol,
        contract_type: args.contract_type,
        version: args.version,
        tags: args.tags,
    };

    // Execute search
    let contracts = store.search_contracts(search_params).await.map_err(|e| {
        let error_msg = format!("Failed to search contracts: {}", e);
        error!("{}", error_msg);
        rig::tool::ToolError::ToolCallError(error_msg.into())
    })?;

    Ok(contracts
        .into_iter()
        .map(|c| ContractData {
            address: c.address,
            chain: c.chain,
            chain_id: c.chain_id,
            source_code: c.source_code,
            abi: c.abi,
            fetched_from_etherscan: false,
        })
        .collect())
}

async fn get_or_fetch_contract(chain_id: u32, address: String) -> Result<ContractData, ToolError> {
    // Normalize address to lowercase for database lookup
    let address = address.to_lowercase();
    debug!("Normalized address to lowercase: {}", address);

    // Connect to database
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://ceciliazhang@localhost:5432/chatbot".to_string());

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
