use serde::Deserialize;

mod account;
mod contract;

pub use account::*;
pub use contract::*;

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
pub(crate) struct EtherscanResponse<T> {
    pub status: String,
    pub message: String,
    pub result: T,
}

/// Maps chain ID to chain name for database storage
pub(crate) fn chain_id_to_name(chain_id: u32) -> String {
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
pub(crate) struct ContractSourceCode {
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
