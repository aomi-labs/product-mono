use rig::tool::ToolError;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::any::AnyPoolOptions;
use std::future::Future;
use tokio::task;
use tracing::{debug, error, info};

use crate::db::{ContractSearchParams, ContractStore, ContractStoreApi};
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
    args: GetContractArgs,
) -> Result<serde_json::Value, ToolError> {
    info!("get_contract_abi tool called with args: {:?}", args);

    let contracts = match (&args.chain_id, &args.address) {
        (Some(chain_id), Some(address)) => {
            let chain_id = *chain_id;
            let address = address.clone();
            vec![run_sync(async move {
                get_or_fetch_contract(chain_id, address).await
            })?]
        }
        _ => run_sync(async move { search_contracts(args).await })?,
    };

    Ok(json!({
        "found": !contracts.is_empty(),
        "count": contracts.len(),
        "contracts": contracts.iter().map(|contract| json!({
            "address": contract.address,
            "chain": contract.chain,
            "chain_id": contract.chain_id,
            "abi": contract.abi,
            "fetched_from_etherscan": contract.fetched_from_etherscan,
        })).collect::<Vec<_>>()
    }))
}

pub async fn execute_get_contract_source_code(
    args: GetContractArgs,
) -> Result<serde_json::Value, ToolError> {
    info!("get_contract_source_code tool called with args: {:?}", args);

    let contracts = match (&args.chain_id, &args.address) {
        (Some(chain_id), Some(address)) => {
            let chain_id = *chain_id;
            let address = address.clone();
            vec![run_sync(async move {
                get_or_fetch_contract(chain_id, address).await
            })?]
        }
        _ => run_sync(async move { search_contracts(args).await })?,
    };

    Ok(json!({
        "found": !contracts.is_empty(),
        "count": contracts.len(),
        "contracts": contracts.iter().map(|contract| json!({
            "address": contract.address,
            "chain": contract.chain,
            "chain_id": contract.chain_id,
            "source_code": contract.source_code,
            "fetched_from_etherscan": contract.fetched_from_etherscan,
        })).collect::<Vec<_>>()
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
