mod contract_store;
mod session_store;
mod traits;
mod transaction_store;

pub use contract_store::ContractStore;
pub use session_store::SessionStore;
pub use traits::{ContractStoreApi, SessionStoreApi, TransactionStoreApi};
pub use transaction_store::TransactionStore;

use sqlx::FromRow;

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

// Session persistence domain models
#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub public_key: String,
    pub username: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub public_key: Option<String>,
    pub started_at: i64,
    pub last_active_at: i64,
    pub title: Option<String>,
    pub pending_transaction: Option<serde_json::Value>,
}

// Custom FromRow for Session because pending_transaction JSONB needs special handling
impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for Session {
    fn from_row(row: &'r sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        // Handle pending_transaction JSONB field - stored as TEXT in DB
        let pending_transaction: Option<String> = row.try_get("pending_transaction").ok();
        let pending_transaction = pending_transaction.and_then(|s| serde_json::from_str(&s).ok());

        Ok(Session {
            id: row.try_get("id")?,
            public_key: row.try_get("public_key")?,
            started_at: row.try_get("started_at")?,
            last_active_at: row.try_get("last_active_at")?,
            title: row.try_get("title")?,
            pending_transaction,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingTransaction {
    pub created_at: i64,
    pub expires_at: i64,
    pub chain_id: u32,
    pub transaction: serde_json::Value,
    pub user_intent: String,
    pub signature: Option<String>,
}

impl Session {
    /// Get parsed pending transaction
    pub fn get_pending_transaction(&self) -> anyhow::Result<Option<PendingTransaction>> {
        match &self.pending_transaction {
            Some(json) => Ok(Some(serde_json::from_value(json.clone())?)),
            None => Ok(None),
        }
    }

    /// Set pending transaction
    pub fn set_pending_transaction(
        &mut self,
        tx: Option<PendingTransaction>,
    ) -> anyhow::Result<()> {
        self.pending_transaction = match tx {
            Some(t) => Some(serde_json::to_value(t)?),
            None => None,
        };
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: i64,
    pub session_id: String,
    pub message_type: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub timestamp: i64,
}

// Custom FromRow for Message because content JSONB needs special handling
impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for Message {
    fn from_row(row: &'r sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        // Handle content JSONB field - stored as TEXT in DB
        let content_str: String = row.try_get("content")?;
        let content =
            serde_json::from_str(&content_str).map_err(|e| sqlx::Error::ColumnDecode {
                index: "content".to_string(),
                source: Box::new(e),
            })?;

        Ok(Message {
            id: row.try_get("id")?,
            session_id: row.try_get("session_id")?,
            message_type: row.try_get("message_type")?,
            sender: row.try_get("sender")?,
            content,
            timestamp: row.try_get("timestamp")?,
        })
    }
}
