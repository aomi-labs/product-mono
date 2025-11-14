use super::{Contract, ContractSearchParams, Transaction, TransactionRecord};
use anyhow::Result;
use async_trait::async_trait;

// Top-level interface for contract storage
#[async_trait]
pub trait ContractStoreApi: Send + Sync {
    async fn get_contract(&self, chain_id: u32, address: String) -> Result<Option<Contract>>;
    async fn get_abi(&self, chain_id: u32, address: String) -> Result<Option<serde_json::Value>>;
    async fn store_contract(&self, contract: Contract) -> Result<()>;
    async fn get_contracts_by_chain(&self, chain_id: u32) -> Result<Vec<Contract>>;
    async fn delete_contract(&self, chain_id: u32, address: String) -> Result<()>;
    async fn search_contracts(&self, params: ContractSearchParams) -> Result<Vec<Contract>>;
}

// Top-level interface for transaction storage
#[async_trait]
pub trait TransactionStoreApi: Send + Sync {
    // Transaction record operations
    async fn get_transaction_record(
        &self,
        chain_id: u32,
        address: String,
    ) -> Result<Option<TransactionRecord>>;
    async fn upsert_transaction_record(&self, record: TransactionRecord) -> Result<()>;

    // Transaction operations
    async fn store_transaction(&self, transaction: Transaction) -> Result<()>;
    async fn get_transactions(
        &self,
        chain_id: u32,
        address: String,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Transaction>>;
    async fn get_transaction_by_hash(
        &self,
        chain_id: u32,
        address: String,
        hash: String,
    ) -> Result<Option<Transaction>>;
    async fn get_transaction_count(&self, chain_id: u32, address: String) -> Result<i64>;
    async fn delete_transactions_for_address(&self, chain_id: u32, address: String) -> Result<()>;
}
