use super::Contract;
use anyhow::Result;
use async_trait::async_trait;

// Top-level interface for contract storage
#[async_trait]
pub trait ContractStoreApi: Send + Sync {
    async fn get_contract(&self, chain: String, address: String) -> Result<Option<Contract>>;
    async fn get_abi(&self, chain: String, address: String) -> Result<Option<serde_json::Value>>;
    async fn store_contract(&self, contract: Contract) -> Result<()>;
    async fn get_contracts_by_chain(&self, chain: String) -> Result<Vec<Contract>>;
    async fn delete_contract(&self, chain: String, address: String) -> Result<()>;
}
