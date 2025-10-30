use super::Contract;
use super::traits::ContractStoreApi;
use anyhow::Result;
use async_trait::async_trait;
use sqlx::{Pool, any::Any};

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
    async fn get_contract(&self, chain: String, address: String) -> Result<Option<Contract>> {
        let query = "SELECT address, chain, source_code, abi FROM contracts WHERE chain = $1 AND address = $2";

        let row = sqlx::query_as::<Any, Contract>(query)
            .bind(&chain)
            .bind(&address)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row)
    }

    async fn get_abi(&self, chain: String, address: String) -> Result<Option<serde_json::Value>> {
        let query = "SELECT abi FROM contracts WHERE chain = $1 AND address = $2";

        let row: Option<String> = sqlx::query_scalar::<Any, String>(query)
            .bind(&chain)
            .bind(&address)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.and_then(|s| serde_json::from_str(&s).ok()))
    }

    async fn store_contract(&self, contract: Contract) -> Result<()> {
        let query = "INSERT INTO contracts (address, chain, source_code, abi) VALUES ($1, $2, $3, $4)
             ON CONFLICT (chain, address) DO UPDATE SET source_code = EXCLUDED.source_code, abi = EXCLUDED.abi";

        let abi_string = serde_json::to_string(&contract.abi)?;

        sqlx::query::<Any>(query)
            .bind(&contract.address)
            .bind(&contract.chain)
            .bind(&contract.source_code)
            .bind(&abi_string)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_contracts_by_chain(&self, chain: String) -> Result<Vec<Contract>> {
        let query = "SELECT address, chain, source_code, abi FROM contracts WHERE chain = $1";

        let contracts = sqlx::query_as::<Any, Contract>(query)
            .bind(&chain)
            .fetch_all(&self.pool)
            .await?;

        Ok(contracts)
    }

    async fn delete_contract(&self, chain: String, address: String) -> Result<()> {
        let query = "DELETE FROM contracts WHERE chain = $1 AND address = $2";

        sqlx::query::<Any>(query)
            .bind(&chain)
            .bind(&address)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
