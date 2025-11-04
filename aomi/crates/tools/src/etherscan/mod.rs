use crate::db::{Contract, ContractStore, ContractStoreApi};
use anyhow::{Context, Result};
use serde::Deserialize;

// Chain ID constants
pub const ETHEREUM_MAINNET: u32 = 1;
pub const GOERLI: u32 = 5;
pub const SEPOLIA: u32 = 11155111;
pub const POLYGON: u32 = 137;
pub const ARBITRUM: u32 = 42161;
pub const OPTIMISM: u32 = 10;
pub const BASE: u32 = 8453;

// Shared Etherscan API response structure
#[derive(Debug, Deserialize)]
pub struct EtherscanResponse<T> {
    pub status: String,
    pub message: String,
    pub result: T,
}

/// Maps chain ID to chain name for database storage
fn chain_id_to_name(chain_id: u32) -> String {
    match chain_id {
        ETHEREUM_MAINNET => "ethereum".to_string(),
        GOERLI => "goerli".to_string(),
        SEPOLIA => "sepolia".to_string(),
        POLYGON => "polygon".to_string(),
        ARBITRUM => "arbitrum".to_string(),
        OPTIMISM => "optimism".to_string(),
        BASE => "base".to_string(),
        _ => format!("chain_{}", chain_id),
    }
}

// Contract-specific structures
#[derive(Debug, Deserialize)]
struct ContractSourceCode {
    #[serde(rename = "SourceCode")]
    pub source_code: String,
    #[serde(rename = "ABI")]
    pub abi: String,
    #[serde(rename = "ContractName")]
    #[allow(dead_code)]
    pub contract_name: String,
}

// Account/Transaction structures
#[derive(Debug, Clone, Deserialize)]
pub struct Transaction {
    #[serde(rename = "blockNumber")]
    pub block_number: String,
    #[serde(rename = "timeStamp")]
    pub timestamp: String,
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: String,
    #[serde(rename = "gasPrice")]
    pub gas_price: String,
    #[serde(rename = "gasUsed")]
    pub gas_used: String,
    #[serde(rename = "isError")]
    pub is_error: String,
    pub input: String,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
}

// ============================================================================
// Contract Functions
// ============================================================================

/// Fetches contract source code and ABI from Etherscan API and returns a Contract struct
/// API key is read from ETHERSCAN_API_KEY environment variable
pub async fn fetch_contract_from_etherscan(chainid: u32, address: String) -> Result<Contract> {
    let api_key = std::env::var("ETHERSCAN_API_KEY")
        .context("ETHERSCAN_API_KEY environment variable not set")?;

    let url = format!(
        "https://api.etherscan.io/v2/api?chainid={}&module=contract&action=getsourcecode&address={}&apikey={}",
        chainid, address, api_key
    );

    let response = reqwest::get(&url)
        .await
        .context("Failed to send request to Etherscan")?;

    let etherscan_response: EtherscanResponse<Vec<ContractSourceCode>> = response
        .json()
        .await
        .context("Failed to parse Etherscan response")?;

    if etherscan_response.status != "1" {
        anyhow::bail!("Etherscan API error: {}", etherscan_response.message);
    }

    let contract_data = etherscan_response
        .result
        .first()
        .context("No contract data returned from Etherscan")?;

    if contract_data.source_code.is_empty()
        || contract_data.source_code == "Contract source code not verified"
    {
        anyhow::bail!("Contract source code not verified on Etherscan");
    }

    if contract_data.abi.is_empty() || contract_data.abi == "Contract source code not verified" {
        anyhow::bail!("Contract ABI not available on Etherscan");
    }

    // Parse ABI JSON
    let abi: serde_json::Value =
        serde_json::from_str(&contract_data.abi).context("Failed to parse contract ABI")?;

    // Verify ABI is a non-empty array
    if !abi.is_array() || abi.as_array().is_none_or(|arr| arr.is_empty()) {
        anyhow::bail!("Contract ABI is empty or invalid");
    }

    Ok(Contract {
        address: address.to_lowercase(),
        chain: chain_id_to_name(chainid),
        chain_id: chainid,
        source_code: contract_data.source_code.clone(),
        abi,
    })
}

/// Fetches contract from Etherscan and saves it to the database
/// API key is read from ETHERSCAN_API_KEY environment variable
pub async fn fetch_and_store_contract(
    chainid: u32,
    address: String,
    store: &ContractStore,
) -> Result<Contract> {
    // Fetch from Etherscan
    let contract = fetch_contract_from_etherscan(chainid, address).await?;

    // Store in database
    store
        .store_contract(contract.clone())
        .await
        .context("Failed to store contract in database")?;

    Ok(contract)
}

// ============================================================================
// Account Functions
// ============================================================================

/// Fetches transaction history for an address from Etherscan API
/// API key is read from ETHERSCAN_API_KEY environment variable
///
/// Returns up to 1000 most recent transactions (Etherscan API limit per request)
pub async fn fetch_transaction_history(address: String, chainid: u32) -> Result<Vec<Transaction>> {
    let api_key = std::env::var("ETHERSCAN_API_KEY")
        .context("ETHERSCAN_API_KEY environment variable not set")?;

    // Validate address format
    if !address.starts_with("0x") || address.len() != 42 {
        anyhow::bail!("Invalid address format. Must be a 42-character hex string starting with 0x");
    }

    let client = reqwest::Client::new();
    let response = client
        .get("https://api.etherscan.io/v2/api")
        .query(&[
            ("chainid", chainid.to_string().as_str()),
            ("module", "account"),
            ("action", "txlist"),
            ("address", address.as_str()),
            ("startblock", "0"),
            ("endblock", "latest"),
            ("page", "1"),
            ("offset", "1000"),
            ("sort", "desc"),
            ("apikey", api_key.as_str()),
        ])
        .send()
        .await
        .context("Failed to send request to Etherscan")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Etherscan API request failed with status: {}",
            response.status()
        );
    }

    let tx_response: EtherscanResponse<Vec<Transaction>> = response
        .json()
        .await
        .context("Failed to parse Etherscan transaction response")?;

    if tx_response.status != "1" {
        anyhow::bail!("Etherscan API error: {}", tx_response.message);
    }

    Ok(tx_response.result)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Contract tests
    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_fetch_usdc_from_etherscan() -> Result<()> {
        // Test with Ethereum mainnet
        let contract = fetch_contract_from_etherscan(
            ETHEREUM_MAINNET,
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
        )
        .await?;

        assert_eq!(
            contract.address,
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
        assert_eq!(contract.chain, "ethereum");
        assert!(!contract.source_code.is_empty());
        assert!(contract.abi.is_array());

        println!("Fetched contract: {} bytes", contract.source_code.len());
        Ok(())
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_fetch_and_store_usdc() -> Result<()> {
        use sqlx::any::AnyPoolOptions;

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://ceciliazhang@localhost:5432/chatbot".to_string());

        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let store = ContractStore::new(pool);

        // Test with Ethereum mainnet
        let contract = fetch_and_store_contract(
            ETHEREUM_MAINNET,
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            &store,
        )
        .await?;

        // Verify it was stored
        let retrieved = store
            .get_contract(ETHEREUM_MAINNET, contract.address.clone())
            .await?;

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.address, contract.address);
        assert!(!retrieved.source_code.is_empty());

        println!("Successfully stored and retrieved contract");
        Ok(())
    }

    // Account tests
    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_fetch_transaction_history() -> Result<()> {
        // Test with Vitalik's address on Ethereum mainnet
        let transactions = fetch_transaction_history(
            "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            ETHEREUM_MAINNET,
        )
        .await?;

        assert!(!transactions.is_empty());

        // Verify first transaction has expected fields
        let first_tx = &transactions[0];
        assert!(!first_tx.hash.is_empty());
        assert!(!first_tx.from.is_empty());
        assert!(!first_tx.block_number.is_empty());

        println!("Fetched {} transactions", transactions.len());
        Ok(())
    }
}
