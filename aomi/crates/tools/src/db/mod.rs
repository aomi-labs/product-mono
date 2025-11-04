mod contract_store;
mod traits;
mod transaction_store;

pub use contract_store::ContractStore;
pub use traits::{ContractStoreApi, TransactionStoreApi};
pub use transaction_store::TransactionStore;

// Domain model
#[derive(Debug, Clone)]
pub struct Contract {
    pub address: String,
    pub chain: String,
    pub chain_id: u32,
    pub source_code: String,
    pub abi: serde_json::Value,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for Contract {
    fn from_row(row: &'r sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Contract {
            address: row.try_get("address")?,
            chain: row.try_get("chain")?,
            chain_id: row.try_get::<i32, _>("chain_id")? as u32,
            source_code: row.try_get("source_code")?,
            abi: serde_json::from_str(row.try_get("abi")?).map_err(|e| {
                sqlx::Error::ColumnDecode {
                    index: "abi".to_string(),
                    source: Box::new(e),
                }
            })?,
        })
    }
}

// Transaction history domain models
#[derive(Debug, Clone)]
pub struct TransactionRecord {
    pub chain_id: u32,
    pub address: String,
    pub nonce: Option<i64>,
    pub last_fetched_at: Option<i64>, // Unix timestamp
    pub last_block_number: Option<i64>,
    pub total_transactions: Option<i32>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for TransactionRecord {
    fn from_row(row: &'r sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(TransactionRecord {
            chain_id: row.try_get::<i32, _>("chain_id")? as u32,
            address: row.try_get("address")?,
            nonce: row.try_get("nonce")?,
            last_fetched_at: row.try_get("last_fetched_at")?,
            last_block_number: row.try_get("last_block_number")?,
            total_transactions: row.try_get("total_transactions")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: Option<i64>,
    pub chain_id: u32,
    pub address: String,
    pub hash: String,
    pub block_number: i64,
    pub timestamp: i64,
    pub from_address: String,
    pub to_address: String,
    pub value: String,
    pub gas: String,
    pub gas_price: String,
    pub gas_used: String,
    pub is_error: String,
    pub input: String,
    pub contract_address: Option<String>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for Transaction {
    fn from_row(row: &'r sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Transaction {
            id: row.try_get("id")?,
            chain_id: row.try_get::<i32, _>("chain_id")? as u32,
            address: row.try_get("address")?,
            hash: row.try_get("hash")?,
            block_number: row.try_get("block_number")?,
            timestamp: row.try_get("timestamp")?,
            from_address: row.try_get("from_address")?,
            to_address: row.try_get("to_address")?,
            value: row.try_get("value")?,
            gas: row.try_get("gas")?,
            gas_price: row.try_get("gas_price")?,
            gas_used: row.try_get("gas_used")?,
            is_error: row.try_get("is_error")?,
            input: row.try_get("input")?,
            contract_address: row.try_get("contract_address")?,
        })
    }
}
