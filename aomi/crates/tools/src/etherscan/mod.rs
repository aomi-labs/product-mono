use crate::clients::EtherscanClient;
use crate::db::{Contract, ContractStore, ContractStoreApi};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::str::FromStr;
use std::sync::Arc;

pub(crate) const ETHERSCAN_V2_URL: &str = "https://api.etherscan.io/v2/api";

// Chain ID constants
pub const ETHEREUM_MAINNET: u32 = 1;
pub const GOERLI: u32 = 5;
pub const SEPOLIA: u32 = 11155111;
pub const POLYGON: u32 = 137;
pub const ARBITRUM: u32 = 42161;
pub const OPTIMISM: u32 = 10;
pub const BASE: u32 = 8453;

/// Supported networks for the unified Etherscan API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Goerli,
    Sepolia,
    Polygon,
    Arbitrum,
    Optimism,
    Base,
}

impl Network {
    pub fn chain_id(self) -> u32 {
        match self {
            Network::Mainnet => ETHEREUM_MAINNET,
            Network::Goerli => GOERLI,
            Network::Sepolia => SEPOLIA,
            Network::Polygon => POLYGON,
            Network::Arbitrum => ARBITRUM,
            Network::Optimism => OPTIMISM,
            Network::Base => BASE,
        }
    }

    pub fn canonical_name(self) -> &'static str {
        match self {
            Network::Mainnet => "mainnet",
            Network::Goerli => "goerli",
            Network::Sepolia => "sepolia",
            Network::Polygon => "polygon",
            Network::Arbitrum => "arbitrum",
            Network::Optimism => "optimism",
            Network::Base => "base",
        }
    }
}

impl FromStr for Network {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "mainnet" | "ethereum" | "eth" => Ok(Network::Mainnet),
            "goerli" => Ok(Network::Goerli),
            "sepolia" => Ok(Network::Sepolia),
            "polygon" | "matic" => Ok(Network::Polygon),
            "arbitrum" | "arb" => Ok(Network::Arbitrum),
            "optimism" | "op" => Ok(Network::Optimism),
            "base" => Ok(Network::Base),
            other => anyhow::bail!("Unsupported network: {}", other),
        }
    }
}

impl TryFrom<u32> for Network {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            ETHEREUM_MAINNET => Ok(Network::Mainnet),
            GOERLI => Ok(Network::Goerli),
            SEPOLIA => Ok(Network::Sepolia),
            POLYGON => Ok(Network::Polygon),
            ARBITRUM => Ok(Network::Arbitrum),
            OPTIMISM => Ok(Network::Optimism),
            BASE => Ok(Network::Base),
            _ => anyhow::bail!("Unsupported chain id: {}", value),
        }
    }
}

/// Maps chain ID to chain name for database storage
pub fn chain_id_to_name(chain_id: u32) -> String {
    Network::try_from(chain_id)
        .map(|network| match network {
            Network::Mainnet => "ethereum".to_string(),
            other => other.canonical_name().to_string(),
        })
        .unwrap_or_else(|_| format!("chain_{}", chain_id))
}

/// Convert a user-supplied network string (e.g. "mainnet") into a chain ID.
pub fn network_name_to_chain_id(name: &str) -> Result<u32> {
    Ok(Network::from_str(name)?.chain_id())
}

impl EtherscanClient {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ETHERSCAN_API_KEY")
            .context("ETHERSCAN_API_KEY environment variable not set")?;
        let builder = Arc::new(reqwest::Client::new().get(ETHERSCAN_V2_URL));
        Ok(Self::new(builder, api_key))
    }

    fn validate_address(address: &str) -> Result<()> {
        if address.starts_with("0x") && address.len() == 42 {
            Ok(())
        } else {
            anyhow::bail!(
                "Invalid address format. Must be a 42-character hex string starting with 0x"
            )
        }
    }

    fn build_params(
        &self,
        chain_id: u32,
        mut params: Vec<(String, String)>,
    ) -> Vec<(String, String)> {
        params.push(("chainid".to_string(), chain_id.to_string()));
        params.push(("apikey".to_string(), self.api_key.clone()));
        params
    }

    async fn send_request<T>(&self, params: Vec<(String, String)>) -> Result<T>
    where
        T: DeserializeOwned,
    {

        let base = self
            .builder
            .try_clone()
            .unwrap_or_else(|| reqwest::Client::new().get(ETHERSCAN_V2_URL));
        let response = base.query(&params).send().await.context("Failed to send request to Etherscan")?;

        let response = response
            .error_for_status()
            .context("Etherscan API request failed")?;

        response
            .json::<T>()
            .await
            .context("Failed to parse Etherscan response")
    }

    /// Fetch contract metadata (source + ABI) for the supplied chain ID.
    pub async fn fetch_contract_by_chain_id(
        &self,
        chain_id: u32,
        address: &str,
    ) -> Result<Contract> {
        Self::validate_address(address)?;

        let params = self.build_params(
            chain_id,
            vec![
                ("module".to_string(), "contract".to_string()),
                ("action".to_string(), "getsourcecode".to_string()),
                ("address".to_string(), address.to_string()),
            ],
        );

        let response: EtherscanResponse<Vec<ContractSourceCode>> =
            self.send_request(params).await?;

        if response.status != "1" {
            anyhow::bail!("Etherscan API error: {}", response.message);
        }

        let contract_data = response
            .result
            .first()
            .context("No contract data returned from Etherscan")?;

        if contract_data.source_code.is_empty()
            || contract_data.source_code == "Contract source code not verified"
        {
            anyhow::bail!("Contract source code not verified on Etherscan");
        }

        if contract_data.abi.is_empty() || contract_data.abi == "Contract source code not verified"
        {
            anyhow::bail!("Contract ABI not available on Etherscan");
        }

        let abi: serde_json::Value =
            serde_json::from_str(&contract_data.abi).context("Failed to parse contract ABI")?;

        if !abi.is_array() || abi.as_array().is_none_or(|arr| arr.is_empty()) {
            anyhow::bail!("Contract ABI is empty or invalid");
        }

        Ok(Contract {
            address: address.to_lowercase(),
            chain: chain_id_to_name(chain_id),
            chain_id,
            source_code: contract_data.source_code.clone(),
            abi,
        })
    }

    pub async fn fetch_contract(&self, network: Network, address: &str) -> Result<Contract> {
        self.fetch_contract_by_chain_id(network.chain_id(), address)
            .await
    }

    pub async fn fetch_transaction_history_by_chain_id(
        &self,
        chain_id: u32,
        address: &str,
        sort: SortOrder,
    ) -> Result<Vec<Transaction>> {
        Self::validate_address(address)?;

        let params = self.build_params(
            chain_id,
            vec![
                ("module".to_string(), "account".to_string()),
                ("action".to_string(), "txlist".to_string()),
                ("address".to_string(), address.to_string()),
                ("startblock".to_string(), "0".to_string()),
                ("endblock".to_string(), "latest".to_string()),
                ("page".to_string(), "1".to_string()),
                ("offset".to_string(), "1000".to_string()),
                ("sort".to_string(), sort.as_str().to_string()),
            ],
        );

        let response: EtherscanResponse<Vec<Transaction>> = self.send_request(params).await?;

        if response.status != "1" {
            anyhow::bail!("Etherscan API error: {}", response.message);
        }

        Ok(response.result)
    }

    pub async fn fetch_transaction_history(
        &self,
        network: Network,
        address: &str,
        sort: SortOrder,
    ) -> Result<Vec<Transaction>> {
        self.fetch_transaction_history_by_chain_id(network.chain_id(), address, sort)
            .await
    }

    pub async fn get_account_balance(&self, chain_id: u32, address: &str) -> Result<String> {
        Self::validate_address(address)?;

        let params = self.build_params(
            chain_id,
            vec![
                ("module".to_string(), "account".to_string()),
                ("action".to_string(), "balance".to_string()),
                ("address".to_string(), address.to_string()),
                ("tag".to_string(), "latest".to_string()),
            ],
        );

        let response: EtherscanResponse<String> = self.send_request(params).await?;

        if response.status != "1" {
            anyhow::bail!("Etherscan API error: {}", response.message);
        }

        Ok(response.result)
    }

    pub async fn get_transaction_count(&self, chain_id: u32, address: &str) -> Result<u64> {
        Self::validate_address(address)?;

        let params = self.build_params(
            chain_id,
            vec![
                ("module".to_string(), "proxy".to_string()),
                ("action".to_string(), "eth_getTransactionCount".to_string()),
                ("address".to_string(), address.to_string()),
                ("tag".to_string(), "latest".to_string()),
            ],
        );

        let response: JsonRpcResponse<String> = self.send_request(params).await?;
        let nonce_hex = response.result.trim_start_matches("0x");
        u64::from_str_radix(nonce_hex, 16).context("Failed to parse nonce from hex")
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        }
    }
}

// Shared Etherscan API response structure
#[derive(Debug, Deserialize)]
pub struct EtherscanResponse<T> {
    pub status: String,
    pub message: String,
    pub result: T,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[allow(dead_code)]
    id: Option<i32>,
    result: T,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
// Convenience helpers that pull the API key from the environment
// ============================================================================

/// Fetches contract source code and ABI from Etherscan API and returns a Contract struct
/// API key is read from ETHERSCAN_API_KEY environment variable
pub async fn fetch_contract_from_etherscan(chainid: u32, address: String) -> Result<Contract> {
    external_clients()
        .etherscan_client()
        .context("ETHERSCAN_API_KEY environment variable not set")?
        .fetch_contract_by_chain_id(chainid, &address)
        .await
}

/// Fetches contract from Etherscan and saves it to the database
/// API key is read from ETHERSCAN_API_KEY environment variable
pub async fn fetch_and_store_contract(
    chainid: u32,
    address: String,
    store: &ContractStore,
) -> Result<Contract> {
    let contract = fetch_contract_from_etherscan(chainid, address).await?;

    store
        .store_contract(contract.clone())
        .await
        .context("Failed to store contract in database")?;

    Ok(contract)
}

/// Fetches transaction history for an address from Etherscan API
/// API key is read from ETHERSCAN_API_KEY environment variable
///
/// Returns up to 1000 most recent transactions (Etherscan API limit per request)
pub async fn fetch_transaction_history(address: String, chainid: u32) -> Result<Vec<Transaction>> {
    external_clients()
        .etherscan_client()
        .context("ETHERSCAN_API_KEY environment variable not set")?
        .fetch_transaction_history_by_chain_id(chainid, &address, SortOrder::Desc)
        .await
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Contract tests
    #[tokio::test]
    // #[ignore] // Run with: cargo test -- --ignored
    async fn test_fetch_usdc_from_etherscan() -> Result<()> {
        let contract = fetch_contract_from_etherscan(
            ETHEREUM_MAINNET,
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
        )
        .await
        .unwrap();

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
    // #[ignore] // Run with: cargo test -- --ignored
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

        let contract = fetch_and_store_contract(
            ETHEREUM_MAINNET,
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            &store,
        )
        .await?;

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
    // #[ignore] // Run with: cargo test -- --ignored
    async fn test_fetch_transaction_history() -> Result<()> {
        let transactions = fetch_transaction_history(
            "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            ETHEREUM_MAINNET,
        )
        .await?;

        assert!(!transactions.is_empty());

        let first_tx = &transactions[0];
        assert!(!first_tx.hash.is_empty());
        assert!(!first_tx.from.is_empty());
        assert!(!first_tx.block_number.is_empty());

        println!("Fetched {} transactions", transactions.len());
        Ok(())
    }
}
use crate::clients::external_clients;
