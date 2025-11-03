use super::{chain_id_to_name, ContractSourceCode, EtherscanResponse};
use crate::db::{Contract, ContractStore, ContractStoreApi};
use anyhow::{Context, Result};

/// Fetches contract source code and ABI from Etherscan API and returns a Contract struct
/// API key is read from ETHERSCAN_API_KEY environment variable
pub async fn fetch_contract_from_etherscan(
    chainid: u32,
    address: String,
) -> Result<Contract> {
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
        anyhow::bail!(
            "Etherscan API error: {}",
            etherscan_response.message
        );
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
    let abi: serde_json::Value = serde_json::from_str(&contract_data.abi)
        .context("Failed to parse contract ABI")?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_fetch_usdc_from_etherscan() -> Result<()> {
        use super::super::ETHEREUM_MAINNET;

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
        use super::super::ETHEREUM_MAINNET;
        use sqlx::any::AnyPoolOptions;

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kevin@localhost:5432/chatbot".to_string());

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
}
