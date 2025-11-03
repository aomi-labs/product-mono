use super::traits::ContractStoreApi;
use super::Contract;
use anyhow::Result;
use async_trait::async_trait;
use sqlx::{any::Any, Pool};

pub struct ContractStore {
    pool: Pool<Any>,
}

impl ContractStore {
    pub fn new(pool: Pool<Any>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ContractStoreApi for ContractStore {
    async fn get_contract(&self, chain_id: u32, address: String) -> Result<Option<Contract>> {
        let query = "SELECT address, chain, chain_id, source_code, abi FROM contracts WHERE chain_id = $1 AND address = $2";

        let row = sqlx::query_as::<Any, Contract>(query)
            .bind(chain_id as i32)
            .bind(&address)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row)
    }

    async fn get_abi(&self, chain_id: u32, address: String) -> Result<Option<serde_json::Value>> {
        let query = "SELECT abi FROM contracts WHERE chain_id = $1 AND address = $2";

        let row: Option<String> = sqlx::query_scalar::<Any, String>(query)
            .bind(chain_id as i32)
            .bind(&address)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.and_then(|s| serde_json::from_str(&s).ok()))
    }

    async fn store_contract(&self, contract: Contract) -> Result<()> {
        let query = "INSERT INTO contracts (address, chain, chain_id, source_code, abi) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (chain_id, address) DO UPDATE SET chain = EXCLUDED.chain, source_code = EXCLUDED.source_code, abi = EXCLUDED.abi";

        let abi_string = serde_json::to_string(&contract.abi)?;

        sqlx::query::<Any>(query)
            .bind(&contract.address)
            .bind(&contract.chain)
            .bind(contract.chain_id as i32)
            .bind(&contract.source_code)
            .bind(&abi_string)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_contracts_by_chain(&self, chain_id: u32) -> Result<Vec<Contract>> {
        let query = "SELECT address, chain, chain_id, source_code, abi FROM contracts WHERE chain_id = $1";

        let contracts = sqlx::query_as::<Any, Contract>(query)
            .bind(chain_id as i32)
            .fetch_all(&self.pool)
            .await?;

        Ok(contracts)
    }

    async fn delete_contract(&self, chain_id: u32, address: String) -> Result<()> {
        let query = "DELETE FROM contracts WHERE chain_id = $1 AND address = $2";

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
    use crate::etherscan::{ARBITRUM, ETHEREUM_MAINNET, OPTIMISM, POLYGON};
    use serde_json::json;
    use sqlx::any::AnyPoolOptions;

    async fn setup_test_store() -> Result<ContractStore> {
        // Install SQLite driver for sqlx::Any
        sqlx::any::install_default_drivers();

        // Use sqlite: prefix to tell sqlx::Any which driver to use
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        // Create the contracts table
        sqlx::query(
            r#"
            CREATE TABLE contracts (
                address TEXT NOT NULL,
                chain TEXT NOT NULL,
                chain_id INTEGER NOT NULL,
                source_code TEXT NOT NULL,
                abi TEXT NOT NULL,
                PRIMARY KEY (chain_id, address)
            )
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(ContractStore::new(pool))
    }

    #[tokio::test]
    async fn test_store_and_get_contract() -> Result<()> {
        let store = setup_test_store().await?;

        let contract = Contract {
            address: "0x123".to_string(),
            chain: "ethereum".to_string(),
            chain_id: ETHEREUM_MAINNET,
            source_code: "contract Test {}".to_string(),
            abi: json!({"test": "abi"}),
        };

        // Store the contract
        store.store_contract(contract.clone()).await?;

        // Retrieve the contract
        let retrieved = store
            .get_contract(ETHEREUM_MAINNET, "0x123".to_string())
            .await?;

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.address, "0x123");
        assert_eq!(retrieved.chain, "ethereum");
        assert_eq!(retrieved.source_code, "contract Test {}");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_abi() -> Result<()> {
        let store = setup_test_store().await?;

        let contract = Contract {
            address: "0x456".to_string(),
            chain: "polygon".to_string(),
            chain_id: POLYGON,
            source_code: "contract Test {}".to_string(),
            abi: json!({"inputs": [], "outputs": []}),
        };

        store.store_contract(contract).await?;

        let abi = store
            .get_abi(POLYGON, "0x456".to_string())
            .await?;

        assert!(abi.is_some());
        assert_eq!(abi.unwrap(), json!({"inputs": [], "outputs": []}));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_contracts_by_chain() -> Result<()> {
        let store = setup_test_store().await?;

        // Store multiple contracts
        let contract1 = Contract {
            address: "0x111".to_string(),
            chain: "ethereum".to_string(),
            chain_id: ETHEREUM_MAINNET,
            source_code: "contract A {}".to_string(),
            abi: json!({}),
        };

        let contract2 = Contract {
            address: "0x222".to_string(),
            chain: "ethereum".to_string(),
            chain_id: ETHEREUM_MAINNET,
            source_code: "contract B {}".to_string(),
            abi: json!({}),
        };

        let contract3 = Contract {
            address: "0x333".to_string(),
            chain: "polygon".to_string(),
            chain_id: POLYGON,
            source_code: "contract C {}".to_string(),
            abi: json!({}),
        };

        store.store_contract(contract1).await?;
        store.store_contract(contract2).await?;
        store.store_contract(contract3).await?;

        let contracts = store
            .get_contracts_by_chain(ETHEREUM_MAINNET)
            .await?;

        assert_eq!(contracts.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_contract() -> Result<()> {
        let store = setup_test_store().await?;

        let contract = Contract {
            address: "0x789".to_string(),
            chain: "optimism".to_string(),
            chain_id: OPTIMISM,
            source_code: "contract Test {}".to_string(),
            abi: json!({}),
        };

        store.store_contract(contract.clone()).await?;

        store
            .delete_contract(OPTIMISM, "0x789".to_string())
            .await?;

        let retrieved = store
            .get_contract(OPTIMISM, "0x789".to_string())
            .await?;

        assert!(retrieved.is_none());

        Ok(())
    }
}
