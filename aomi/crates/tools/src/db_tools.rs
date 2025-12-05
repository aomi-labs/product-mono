use rig::tool::ToolError;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::any::AnyPoolOptions;
use std::future::Future;
use tokio::task;
use tracing::{debug, error, info, warn};

use crate::db::{ContractSearchParams, ContractStore, ContractStoreApi};
use crate::etherscan::{fetch_and_store_contract, fetch_contract_from_etherscan};

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
}

pub fn run_sync<F, T>(future: F) -> Result<T, ToolError>
where
    F: Future<Output = Result<T, ToolError>> + Send + 'static,
    T: Send + 'static,
{
    task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

/// Internal helper to fetch contracts and return JSON with optional ABI and/or source code
async fn get_contract_inner(
    args: GetContractArgs,
    need_abi: bool,
    need_code: bool,
) -> Result<serde_json::Value, ToolError> {
    let contracts = match (&args.chain_id, &args.address) {
        (Some(chain_id), Some(address)) => {
            vec![get_or_fetch_contract(*chain_id, address.clone()).await?]
        }
        _ => search_contracts(args).await?,
    };

    Ok(json!({
        "found": !contracts.is_empty(),
        "count": contracts.len(),
        "contracts": contracts.iter().map(|contract| contract.to_json(need_abi, need_code)).collect::<Vec<_>>()
    }))
}

pub async fn execute_get_contract_abi(
    args: GetContractArgs,
) -> Result<serde_json::Value, ToolError> {
    info!("get_contract_abi tool called with args: {:?}", args);
    get_contract_inner(args, true, false).await
}

pub async fn execute_get_contract_source_code(
    args: GetContractArgs,
) -> Result<serde_json::Value, ToolError> {
    info!("get_contract_source_code tool called with args: {:?}", args);
    get_contract_inner(args, false, true).await
}

pub struct ContractData {
    pub address: String,
    pub chain: String,
    pub chain_id: u32,
    pub source_code: String,
    pub abi: serde_json::Value,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub is_proxy: Option<bool>,
    pub implementation_address: Option<String>,
    pub fetched_from_etherscan: bool,
}

impl ContractData {
    /// Convert ContractData to JSON, optionally including ABI and/or source code
    fn to_json(&self, need_abi: bool, need_code: bool) -> serde_json::Value {
        let mut contract_json = json!({
            "address": self.address,
            "chain": self.chain,
            "chain_id": self.chain_id,
            "name": self.name,
            "symbol": self.symbol,
            "is_proxy": self.is_proxy,
            "implementation_address": self.implementation_address,
            "fetched_from_etherscan": self.fetched_from_etherscan,
        });

        if need_abi {
            contract_json["abi"] = self.abi.clone();
        }
        if need_code {
            contract_json["source_code"] = json!(self.source_code);
        }

        contract_json
    }
}

async fn search_contracts(args: GetContractArgs) -> Result<Vec<ContractData>, ToolError> {
    debug!("Searching contracts with args: {:?}", args);

    // Connect to database
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aomi@localhost:5432/chatbot".to_string());

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
            name: c.name,
            symbol: c.symbol,
            is_proxy: c.is_proxy,
            implementation_address: c.implementation_address,
            fetched_from_etherscan: false,
        })
        .collect())
}

pub async fn get_or_fetch_contract(chain_id: u32, address: String) -> Result<ContractData, ToolError> {
    // Normalize address to lowercase for database lookup
    let address = address.to_lowercase();
    debug!("Normalized address to lowercase: {}", address);

    // Connect to database
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aomi@localhost:5432/chatbot".to_string());

    debug!("Connecting to database: {}", database_url);

    let store = match AnyPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
    {
        Ok(pool) => {
            debug!("Database connection successful");
            Some(ContractStore::new(pool))
        }
        Err(e) => {
            warn!(
                "Database connection error: {}. Falling back to Etherscan-only fetch.",
                e
            );
            None
        }
    };

    // Get contract
    if let Some(store) = &store {
        debug!("Querying database for contract");
        match store.get_contract(chain_id, address.clone()).await {
            Ok(Some(c)) => {
                info!("Contract found in database: {}", c.address);
                return Ok(ContractData {
                    address: c.address,
                    chain: c.chain,
                    chain_id: c.chain_id,
                    source_code: c.source_code,
                    abi: c.abi,
                    name: c.name,
                    symbol: c.symbol,
                    is_proxy: c.is_proxy,
                    implementation_address: c.implementation_address,
                    fetched_from_etherscan: false,
                });
            }
            Ok(None) => {
                info!("Contract not found in database, fetching from Etherscan");
            }
            Err(e) => {
                warn!(
                    "Failed to query contract from database: {}. Falling back to Etherscan fetch.",
                    e
                );
            }
        }
    } else {
        info!("Skipping DB lookup; no database available. Fetching from Etherscan.");
    }

    // Not found (or DB unavailable) — fetch from Etherscan and persist if we can
    let fetched_contract = if let Some(store) = &store {
        match fetch_and_store_contract(chain_id, address.clone(), store).await {
            Ok(contract) => contract,
            Err(e) => {
                warn!(
                    "Failed to fetch-and-store contract via DB path: {}. Trying direct fetch.",
                    e
                );
                fetch_contract_from_etherscan(chain_id, address.clone())
                    .await
                    .map_err(|e| {
                        let error_msg =
                            format!("Failed to fetch from Etherscan after DB failure: {}", e);
                        error!("{}", error_msg);
                        rig::tool::ToolError::ToolCallError(error_msg.into())
                    })?
            }
        }
    } else {
        fetch_contract_from_etherscan(chain_id, address.clone())
            .await
            .map_err(|e| {
                let error_msg = format!("Failed to fetch from Etherscan: {}", e);
                error!("{}", error_msg);
                rig::tool::ToolError::ToolCallError(error_msg.into())
            })?
    };

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
        name: fetched_contract.name,
        symbol: fetched_contract.symbol,
        is_proxy: fetched_contract.is_proxy,
        implementation_address: fetched_contract.implementation_address,
        fetched_from_etherscan: true,
    })
}
