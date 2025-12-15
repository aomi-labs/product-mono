use super::traits::TransactionStoreApi;
use super::{Transaction, TransactionRecord};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::{Pool, any::Any};

pub struct TransactionStore {
    pub(crate) pool: Pool<Any>,
}

impl TransactionStore {
    pub fn new(pool: Pool<Any>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TransactionStoreApi for TransactionStore {
    async fn get_transaction_record(
        &self,
        chain_id: u32,
        address: String,
    ) -> Result<Option<TransactionRecord>> {
        let query = "SELECT chain_id, address, nonce, last_fetched_at, last_block_number, total_transactions
                     FROM transaction_records
                     WHERE chain_id = $1 AND address = $2";

        let record = sqlx::query_as::<Any, TransactionRecord>(query)
            .bind(chain_id as i32)
            .bind(&address)
            .fetch_optional(&self.pool)
            .await?;

        Ok(record)
    }

    async fn upsert_transaction_record(&self, record: TransactionRecord) -> Result<()> {
        let query = "INSERT INTO transaction_records (chain_id, address, nonce, last_fetched_at, last_block_number, total_transactions)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (chain_id, address) DO UPDATE SET
                         nonce = EXCLUDED.nonce,
                         last_fetched_at = EXCLUDED.last_fetched_at,
                         last_block_number = EXCLUDED.last_block_number,
                         total_transactions = EXCLUDED.total_transactions";

        sqlx::query::<Any>(query)
            .bind(record.chain_id as i32)
            .bind(&record.address)
            .bind(record.nonce)
            .bind(record.last_fetched_at)
            .bind(record.last_block_number)
            .bind(record.total_transactions)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn store_transaction(&self, transaction: Transaction) -> Result<()> {
        let query = "INSERT INTO transactions (chain_id, address, hash, block_number, timestamp, from_address, to_address, value, gas, gas_price, gas_used, is_error, input, contract_address)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                     ON CONFLICT (chain_id, address, hash) DO UPDATE SET
                         block_number = EXCLUDED.block_number,
                         timestamp = EXCLUDED.timestamp,
                         from_address = EXCLUDED.from_address,
                         to_address = EXCLUDED.to_address,
                         value = EXCLUDED.value,
                         gas = EXCLUDED.gas,
                         gas_price = EXCLUDED.gas_price,
                         gas_used = EXCLUDED.gas_used,
                         is_error = EXCLUDED.is_error,
                         input = EXCLUDED.input,
                         contract_address = EXCLUDED.contract_address";

        sqlx::query::<Any>(query)
            .bind(transaction.chain_id as i32)
            .bind(&transaction.address)
            .bind(&transaction.hash)
            .bind(transaction.block_number)
            .bind(transaction.timestamp)
            .bind(&transaction.from_address)
            .bind(&transaction.to_address)
            .bind(&transaction.value)
            .bind(&transaction.gas)
            .bind(&transaction.gas_price)
            .bind(&transaction.gas_used)
            .bind(&transaction.is_error)
            .bind(&transaction.input)
            .bind(&transaction.contract_address)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_transactions(
        &self,
        chain_id: u32,
        address: String,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Transaction>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let query = "SELECT id, chain_id, address, hash, block_number, timestamp, from_address, to_address, value, gas, gas_price, gas_used, is_error, input, contract_address
                     FROM transactions
                     WHERE chain_id = $1 AND address = $2
                     ORDER BY block_number DESC
                     LIMIT $3 OFFSET $4";

        let transactions = sqlx::query_as::<Any, Transaction>(query)
            .bind(chain_id as i32)
            .bind(&address)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        Ok(transactions)
    }

    async fn get_transaction_by_hash(
        &self,
        chain_id: u32,
        address: String,
        hash: String,
    ) -> Result<Option<Transaction>> {
        let query = "SELECT id, chain_id, address, hash, block_number, timestamp, from_address, to_address, value, gas, gas_price, gas_used, is_error, input, contract_address
                     FROM transactions
                     WHERE chain_id = $1 AND address = $2 AND hash = $3";

        let transaction = sqlx::query_as::<Any, Transaction>(query)
            .bind(chain_id as i32)
            .bind(&address)
            .bind(&hash)
            .fetch_optional(&self.pool)
            .await?;

        Ok(transaction)
    }

    async fn get_transaction_count(&self, chain_id: u32, address: String) -> Result<i64> {
        let query = "SELECT COUNT(*) FROM transactions WHERE chain_id = $1 AND address = $2";

        let count: i64 = sqlx::query_scalar::<Any, i64>(query)
            .bind(chain_id as i32)
            .bind(&address)
            .fetch_one(&self.pool)
            .await?;

        Ok(count)
    }

    async fn delete_transactions_for_address(&self, chain_id: u32, address: String) -> Result<()> {
        let query = "DELETE FROM transactions WHERE chain_id = $1 AND address = $2";

        sqlx::query::<Any>(query)
            .bind(chain_id as i32)
            .bind(&address)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::any::AnyPoolOptions;

    async fn setup_test_store() -> Result<TransactionStore> {
        // Install SQLite driver for sqlx::Any
        sqlx::any::install_default_drivers();

        // Use sqlite: prefix to tell sqlx::Any which driver to use
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        // Create the transaction_records table
        sqlx::query(
            r#"
            CREATE TABLE transaction_records (
                chain_id INTEGER NOT NULL,
                address TEXT NOT NULL,
                nonce BIGINT,
                last_fetched_at BIGINT,
                last_block_number BIGINT,
                total_transactions INTEGER,
                PRIMARY KEY (chain_id, address)
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Create the transactions table
        sqlx::query(
            r#"
            CREATE TABLE transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                chain_id INTEGER NOT NULL,
                address TEXT NOT NULL,
                hash TEXT NOT NULL,
                block_number BIGINT NOT NULL,
                timestamp BIGINT NOT NULL,
                from_address TEXT NOT NULL,
                to_address TEXT NOT NULL,
                value TEXT NOT NULL,
                gas TEXT NOT NULL,
                gas_price TEXT NOT NULL,
                gas_used TEXT NOT NULL,
                is_error TEXT NOT NULL,
                input TEXT NOT NULL,
                contract_address TEXT,
                FOREIGN KEY (chain_id, address) REFERENCES transaction_records(chain_id, address),
                UNIQUE (chain_id, address, hash)
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Create indexes
        sqlx::query("CREATE INDEX idx_tx_chain_address_block ON transactions(chain_id, address, block_number DESC)")
            .execute(&pool)
            .await?;

        sqlx::query("CREATE INDEX idx_tx_hash ON transactions(hash)")
            .execute(&pool)
            .await?;

        sqlx::query(
            "CREATE INDEX idx_tx_timestamp ON transactions(chain_id, address, timestamp DESC)",
        )
        .execute(&pool)
        .await?;

        Ok(TransactionStore::new(pool))
    }

    #[tokio::test]
    async fn test_upsert_and_get_transaction_record() -> Result<()> {
        let store = setup_test_store().await?;

        let record = TransactionRecord {
            chain_id: 1,
            address: "0x123".to_string(),
            nonce: Some(10),
            last_fetched_at: None,
            last_block_number: Some(1000),
            total_transactions: Some(5),
        };

        store.upsert_transaction_record(record.clone()).await?;

        let retrieved = store.get_transaction_record(1, "0x123".to_string()).await?;

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.chain_id, 1);
        assert_eq!(retrieved.address, "0x123");
        assert_eq!(retrieved.nonce, Some(10));
        assert_eq!(retrieved.last_block_number, Some(1000));
        assert_eq!(retrieved.total_transactions, Some(5));

        Ok(())
    }

    #[tokio::test]
    async fn test_store_and_get_transaction() -> Result<()> {
        let store = setup_test_store().await?;

        // First create a transaction record
        let record = TransactionRecord {
            chain_id: 1,
            address: "0x456".to_string(),
            nonce: Some(5),
            last_fetched_at: None,
            last_block_number: Some(500),
            total_transactions: Some(1),
        };
        store.upsert_transaction_record(record).await?;

        // Now create a transaction
        let transaction = Transaction {
            id: None,
            chain_id: 1,
            address: "0x456".to_string(),
            hash: "0xabc123".to_string(),
            block_number: 500,
            timestamp: 1234567890,
            from_address: "0xfrom".to_string(),
            to_address: "0xto".to_string(),
            value: "1000000000000000000".to_string(),
            gas: "21000".to_string(),
            gas_price: "20000000000".to_string(),
            gas_used: "21000".to_string(),
            is_error: "0".to_string(),
            input: "0x".to_string(),
            contract_address: None,
        };

        store.store_transaction(transaction.clone()).await?;

        let retrieved = store
            .get_transaction_by_hash(1, "0x456".to_string(), "0xabc123".to_string())
            .await?;

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.hash, "0xabc123");
        assert_eq!(retrieved.block_number, 500);
        assert_eq!(retrieved.value, "1000000000000000000");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transactions_with_pagination() -> Result<()> {
        let store = setup_test_store().await?;

        // Create transaction record
        let record = TransactionRecord {
            chain_id: 1,
            address: "0x789".to_string(),
            nonce: Some(10),
            last_fetched_at: None,
            last_block_number: Some(1000),
            total_transactions: Some(3),
        };
        store.upsert_transaction_record(record).await?;

        // Store multiple transactions
        for i in 1..=3 {
            let transaction = Transaction {
                id: None,
                chain_id: 1,
                address: "0x789".to_string(),
                hash: format!("0xhash{}", i),
                block_number: 1000 + i,
                timestamp: 1234567890 + i,
                from_address: "0xfrom".to_string(),
                to_address: "0xto".to_string(),
                value: "1000".to_string(),
                gas: "21000".to_string(),
                gas_price: "20000000000".to_string(),
                gas_used: "21000".to_string(),
                is_error: "0".to_string(),
                input: "0x".to_string(),
                contract_address: None,
            };
            store.store_transaction(transaction).await?;
        }

        let transactions = store
            .get_transactions(1, "0x789".to_string(), Some(2), Some(0))
            .await?;

        assert_eq!(transactions.len(), 2);
        // Should be ordered by block_number DESC
        assert_eq!(transactions[0].block_number, 1003);
        assert_eq!(transactions[1].block_number, 1002);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transaction_count() -> Result<()> {
        let store = setup_test_store().await?;

        // Create transaction record
        let record = TransactionRecord {
            chain_id: 1,
            address: "0xabc".to_string(),
            nonce: Some(5),
            last_fetched_at: None,
            last_block_number: Some(500),
            total_transactions: Some(2),
        };
        store.upsert_transaction_record(record).await?;

        // Store transactions
        for i in 1..=2 {
            let transaction = Transaction {
                id: None,
                chain_id: 1,
                address: "0xabc".to_string(),
                hash: format!("0xhash{}", i),
                block_number: 500 + i,
                timestamp: 1234567890 + i,
                from_address: "0xfrom".to_string(),
                to_address: "0xto".to_string(),
                value: "1000".to_string(),
                gas: "21000".to_string(),
                gas_price: "20000000000".to_string(),
                gas_used: "21000".to_string(),
                is_error: "0".to_string(),
                input: "0x".to_string(),
                contract_address: None,
            };
            store.store_transaction(transaction).await?;
        }

        let count = store.get_transaction_count(1, "0xabc".to_string()).await?;
        assert_eq!(count, 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_transactions_for_address() -> Result<()> {
        let store = setup_test_store().await?;

        // Create transaction record
        let record = TransactionRecord {
            chain_id: 1,
            address: "0xdef".to_string(),
            nonce: Some(3),
            last_fetched_at: None,
            last_block_number: Some(300),
            total_transactions: Some(1),
        };
        store.upsert_transaction_record(record).await?;

        // Store a transaction
        let transaction = Transaction {
            id: None,
            chain_id: 1,
            address: "0xdef".to_string(),
            hash: "0xtest".to_string(),
            block_number: 300,
            timestamp: 1234567890,
            from_address: "0xfrom".to_string(),
            to_address: "0xto".to_string(),
            value: "1000".to_string(),
            gas: "21000".to_string(),
            gas_price: "20000000000".to_string(),
            gas_used: "21000".to_string(),
            is_error: "0".to_string(),
            input: "0x".to_string(),
            contract_address: None,
        };
        store.store_transaction(transaction).await?;

        // Delete transactions
        store
            .delete_transactions_for_address(1, "0xdef".to_string())
            .await?;

        let count = store.get_transaction_count(1, "0xdef".to_string()).await?;
        assert_eq!(count, 0);

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires reachable Postgres (DATABASE_URL) and real network
    async fn test_transaction_store_postgres_integration() -> Result<()> {
        // - Install: brew install postgresql@16
        // - Start service: brew services start postgresql@16
        // - Create DB/user (adjust names as needed):
        //     - createdb chatbot
        //     - createuser -s "$(whoami)" (or use your preferred user)
        // - Connection URL example: postgres://$(whoami)@localhost:5432/chatbot
        // - export DATABASE_URL=postgres://$(whoami)@localhost:5432/chatbot

        // Connect to real PostgreSQL database
        sqlx::any::install_default_drivers();
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://aomi@localhost:5432/chatbot".to_string());

        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let store = TransactionStore::new(pool);

        // Use a unique test address to avoid conflicts
        let test_address = format!("0xtest{}", chrono::Utc::now().timestamp());
        let chain_id = 1;

        // 1. Create transaction record
        let record = TransactionRecord {
            chain_id,
            address: test_address.clone(),
            nonce: Some(100),
            last_fetched_at: Some(chrono::Utc::now().timestamp()),
            last_block_number: Some(18500000),
            total_transactions: Some(3),
        };

        store.upsert_transaction_record(record.clone()).await?;
        println!("✓ Created transaction record");

        // 2. Verify record was stored
        let retrieved_record = store
            .get_transaction_record(chain_id, test_address.clone())
            .await?;

        assert!(retrieved_record.is_some());
        let retrieved_record = retrieved_record.unwrap();
        assert_eq!(retrieved_record.chain_id, chain_id);
        assert_eq!(retrieved_record.address, test_address);
        assert_eq!(retrieved_record.nonce, Some(100));
        println!("✓ Retrieved transaction record");

        // 3. Store multiple transactions
        for i in 1..=3 {
            let transaction = Transaction {
                id: None,
                chain_id,
                address: test_address.clone(),
                hash: format!("0xintegrationtest{}{}", chrono::Utc::now().timestamp(), i),
                block_number: 18500000 + i,
                timestamp: 1699564800 + i,
                from_address: "0xfromaddress".to_string(),
                to_address: "0xtoaddress".to_string(),
                value: format!("{}", 1000000000000000000_i64 * i), // i ETH in wei
                gas: "21000".to_string(),
                gas_price: "20000000000".to_string(),
                gas_used: "21000".to_string(),
                is_error: "0".to_string(),
                input: "0x".to_string(),
                contract_address: None,
            };

            store.store_transaction(transaction).await?;
        }
        println!("✓ Stored 3 transactions");

        // 4. Get transaction count
        let count = store
            .get_transaction_count(chain_id, test_address.clone())
            .await?;
        assert_eq!(count, 3);
        println!("✓ Verified transaction count: {}", count);

        // 5. Get transactions with pagination
        let transactions = store
            .get_transactions(chain_id, test_address.clone(), Some(2), Some(0))
            .await?;

        assert_eq!(transactions.len(), 2);
        // Should be ordered by block_number DESC
        assert!(transactions[0].block_number >= transactions[1].block_number);
        println!("✓ Retrieved paginated transactions");

        // 6. Test upsert (update existing record)
        let updated_record = TransactionRecord {
            chain_id,
            address: test_address.clone(),
            nonce: Some(150), // Updated nonce
            last_fetched_at: Some(chrono::Utc::now().timestamp()),
            last_block_number: Some(18500010),
            total_transactions: Some(3),
        };

        store.upsert_transaction_record(updated_record).await?;

        let retrieved_updated = store
            .get_transaction_record(chain_id, test_address.clone())
            .await?;

        assert!(retrieved_updated.is_some());
        assert_eq!(retrieved_updated.unwrap().nonce, Some(150));
        println!("✓ Updated transaction record via upsert");

        // 7. Test get_transaction_by_hash
        let first_tx_hash = transactions[0].hash.clone();
        let tx_by_hash = store
            .get_transaction_by_hash(chain_id, test_address.clone(), first_tx_hash.clone())
            .await?;

        assert!(tx_by_hash.is_some());
        assert_eq!(tx_by_hash.unwrap().hash, first_tx_hash);
        println!("✓ Retrieved transaction by hash");

        // 8. Clean up - delete all test transactions
        store
            .delete_transactions_for_address(chain_id, test_address.clone())
            .await?;

        let count_after_delete = store
            .get_transaction_count(chain_id, test_address.clone())
            .await?;
        assert_eq!(count_after_delete, 0);
        println!("✓ Deleted all test transactions");

        // 9. Clean up - delete transaction record
        // Note: We need to manually delete the record since there's no delete method in the API
        sqlx::query("DELETE FROM transaction_records WHERE chain_id = $1 AND address = $2")
            .bind(chain_id as i32)
            .bind(&test_address)
            .execute(&store.pool)
            .await?;
        println!("✓ Deleted transaction record");

        println!("\n✅ All PostgreSQL integration tests passed!");

        Ok(())
    }
}
