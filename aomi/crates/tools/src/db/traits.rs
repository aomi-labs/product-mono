use super::{
    ApiKey, ApiKeyUpdate, Contract, ContractSearchParams, Message, PendingTransaction, Session,
    Transaction, TransactionRecord, User,
};
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
    async fn list_contracts(
        &self,
        params: ContractSearchParams,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Contract>>;
    async fn update_contract(&self, update: super::ContractUpdate) -> Result<Contract>;
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

// Top-level interface for session storage
#[async_trait]
pub trait SessionStoreApi: Send + Sync {
    // User operations
    async fn get_or_create_user(&self, public_key: &str) -> Result<User>;
    async fn get_user(&self, public_key: &str) -> Result<Option<User>>;
    async fn update_user_username(&self, public_key: &str, username: Option<String>) -> Result<()>;
    async fn update_user_namespaces(&self, public_key: &str, namespaces: Vec<String>)
    -> Result<()>;
    async fn list_users(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<User>>;
    async fn delete_user(&self, public_key: &str) -> Result<u64>;

    // Session operations
    async fn create_session(&self, session: &Session) -> Result<()>;
    async fn get_session(&self, session_id: &str) -> Result<Option<Session>>;
    async fn update_session_activity(&self, session_id: &str) -> Result<()>;
    async fn update_session_public_key(
        &self,
        session_id: &str,
        public_key: Option<String>,
    ) -> Result<()>;
    async fn update_session_title(&self, session_id: &str, title: String) -> Result<()>;
    async fn set_session_title(&self, session_id: &str, title: Option<String>) -> Result<()>;
    async fn update_messages_persisted(&self, session_id: &str, persisted: bool) -> Result<()>;
    async fn get_messages_persisted(&self, session_id: &str) -> Result<Option<bool>>;
    async fn get_user_sessions(&self, public_key: &str, limit: i32) -> Result<Vec<Session>>;
    async fn list_sessions(
        &self,
        public_key: Option<String>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Session>>;
    async fn delete_old_sessions(&self, inactive_since: i64) -> Result<u64>;
    async fn delete_session(&self, session_id: &str) -> Result<()>;

    // Pending transaction operations
    async fn update_pending_transaction(
        &self,
        session_id: &str,
        tx: Option<PendingTransaction>,
    ) -> Result<()>;

    // Message operations
    async fn save_message(&self, message: &Message) -> Result<i64>;
    async fn get_messages(
        &self,
        session_id: &str,
        message_type: Option<&str>,
        limit: Option<i32>,
    ) -> Result<Vec<Message>>;
    async fn get_user_message_history(&self, public_key: &str, limit: i32) -> Result<Vec<Message>>;
}

// Top-level interface for api key storage
#[async_trait]
pub trait ApiKeyStoreApi: Send + Sync {
    async fn create_api_key(
        &self,
        api_key: String,
        label: Option<String>,
        allowed_namespaces: Vec<String>,
    ) -> Result<ApiKey>;
    async fn list_api_keys(
        &self,
        active_only: bool,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<ApiKey>>;
    async fn update_api_key(&self, update: ApiKeyUpdate) -> Result<ApiKey>;
}
