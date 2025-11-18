use anyhow::Result;
use sqlx::{Pool, Postgres, Row};

use crate::models::Contract;

#[derive(Debug, Clone)]
pub struct ContractStore {
    pool: Pool<Postgres>,
}

impl ContractStore {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Insert or update a contract (UPSERT)
    pub async fn upsert_contract(&self, contract: &Contract) -> Result<()> {
        let data_source = contract.data_source.as_str();

        sqlx::query(
            r#"
            INSERT INTO contracts (
                address, chain, chain_id, name, symbol, description,
                is_proxy, implementation_address, source_code, abi,
                tvl, transaction_count, last_activity_at, data_source,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                    EXTRACT(EPOCH FROM NOW())::BIGINT,
                    EXTRACT(EPOCH FROM NOW())::BIGINT)
            ON CONFLICT (chain_id, address) DO UPDATE SET
                name = EXCLUDED.name,
                symbol = EXCLUDED.symbol,
                description = EXCLUDED.description,
                is_proxy = EXCLUDED.is_proxy,
                implementation_address = EXCLUDED.implementation_address,
                source_code = EXCLUDED.source_code,
                abi = EXCLUDED.abi,
                tvl = EXCLUDED.tvl,
                transaction_count = EXCLUDED.transaction_count,
                last_activity_at = EXCLUDED.last_activity_at,
                data_source = EXCLUDED.data_source,
                updated_at = EXTRACT(EPOCH FROM NOW())::BIGINT
            "#,
        )
        .bind(&contract.address)
        .bind(&contract.chain)
        .bind(contract.chain_id)
        .bind(&contract.name)
        .bind(&contract.symbol)
        .bind(&contract.description)
        .bind(contract.is_proxy)
        .bind(&contract.implementation_address)
        .bind(&contract.source_code)
        .bind(&contract.abi)
        .bind(contract.tvl)
        .bind(contract.transaction_count)
        .bind(contract.last_activity_at)
        .bind(data_source)
        .execute(&self.pool)
        .await?;

        tracing::debug!(
            "Upserted contract {} on chain {}",
            contract.address,
            contract.chain_id
        );

        Ok(())
    }

    /// Batch upsert contracts using a transaction
    pub async fn upsert_contracts_batch(&self, contracts: &[Contract]) -> Result<()> {
        if contracts.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for contract in contracts {
            let data_source = contract.data_source.as_str();

            sqlx::query(
                r#"
                INSERT INTO contracts (
                    address, chain, chain_id, name, symbol, description,
                    is_proxy, implementation_address, source_code, abi,
                    tvl, transaction_count, last_activity_at, data_source,
                    created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                        EXTRACT(EPOCH FROM NOW())::BIGINT,
                        EXTRACT(EPOCH FROM NOW())::BIGINT)
                ON CONFLICT (chain_id, address) DO UPDATE SET
                    name = EXCLUDED.name,
                    symbol = EXCLUDED.symbol,
                    description = EXCLUDED.description,
                    is_proxy = EXCLUDED.is_proxy,
                    implementation_address = EXCLUDED.implementation_address,
                    source_code = EXCLUDED.source_code,
                    abi = EXCLUDED.abi,
                    tvl = EXCLUDED.tvl,
                    transaction_count = EXCLUDED.transaction_count,
                    last_activity_at = EXCLUDED.last_activity_at,
                    data_source = EXCLUDED.data_source,
                    updated_at = EXTRACT(EPOCH FROM NOW())::BIGINT
                "#,
            )
            .bind(&contract.address)
            .bind(&contract.chain)
            .bind(contract.chain_id)
            .bind(&contract.name)
            .bind(&contract.symbol)
            .bind(&contract.description)
            .bind(contract.is_proxy)
            .bind(&contract.implementation_address)
            .bind(&contract.source_code)
            .bind(&contract.abi)
            .bind(contract.tvl)
            .bind(contract.transaction_count)
            .bind(contract.last_activity_at)
            .bind(data_source)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        tracing::info!("Batch upserted {} contracts", contracts.len());

        Ok(())
    }

    /// Get existing contract for comparison
    pub async fn get_contract(&self, chain_id: i32, address: &str) -> Result<Option<Contract>> {
        let row = sqlx::query(
            r#"
            SELECT
                address, chain, chain_id, name, symbol, description,
                is_proxy, implementation_address, source_code, abi,
                tvl, transaction_count, last_activity_at, data_source
            FROM contracts
            WHERE chain_id = $1 AND address = $2
            "#,
        )
        .bind(chain_id)
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let data_source_str: String = row.try_get("data_source")?;
            let data_source = crate::models::DataSource::from_str(&data_source_str)
                .unwrap_or(crate::models::DataSource::Manual);

            Ok(Some(Contract {
                address: row.try_get("address")?,
                chain: row.try_get("chain")?,
                chain_id: row.try_get("chain_id")?,
                name: row.try_get("name")?,
                symbol: row.try_get("symbol")?,
                description: row.try_get("description")?,
                is_proxy: row.try_get("is_proxy")?,
                implementation_address: row.try_get("implementation_address")?,
                source_code: row.try_get("source_code")?,
                abi: row.try_get("abi")?,
                tvl: row.try_get("tvl")?,
                transaction_count: row.try_get("transaction_count")?,
                last_activity_at: row.try_get("last_activity_at")?,
                data_source,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get contracts needing update (older than X days)
    pub async fn get_stale_contracts(&self, days: i64) -> Result<Vec<Contract>> {
        let cutoff_timestamp = chrono::Utc::now().timestamp() - (days * 24 * 60 * 60);

        let rows = sqlx::query(
            r#"
            SELECT
                address, chain, chain_id, name, symbol, description,
                is_proxy, implementation_address, source_code, abi,
                tvl, transaction_count, last_activity_at, data_source
            FROM contracts
            WHERE updated_at < $1
            ORDER BY updated_at ASC
            "#,
        )
        .bind(cutoff_timestamp)
        .fetch_all(&self.pool)
        .await?;

        let contracts = rows
            .into_iter()
            .map(|row| {
                let data_source_str: String = row.try_get("data_source")?;
                let data_source = crate::models::DataSource::from_str(&data_source_str)
                    .unwrap_or(crate::models::DataSource::Manual);

                Ok(Contract {
                    address: row.try_get("address")?,
                    chain: row.try_get("chain")?,
                    chain_id: row.try_get("chain_id")?,
                    name: row.try_get("name")?,
                    symbol: row.try_get("symbol")?,
                    description: row.try_get("description")?,
                    is_proxy: row.try_get("is_proxy")?,
                    implementation_address: row.try_get("implementation_address")?,
                    source_code: row.try_get("source_code")?,
                    abi: row.try_get("abi")?,
                    tvl: row.try_get("tvl")?,
                    transaction_count: row.try_get("transaction_count")?,
                    last_activity_at: row.try_get("last_activity_at")?,
                    data_source,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(contracts)
    }

    /// Get contracts by chain ID
    pub async fn get_contracts_by_chain(&self, chain_id: i32) -> Result<Vec<Contract>> {
        let rows = sqlx::query(
            r#"
            SELECT
                address, chain, chain_id, name, symbol, description,
                is_proxy, implementation_address, source_code, abi,
                tvl, transaction_count, last_activity_at, data_source
            FROM contracts
            WHERE chain_id = $1
            ORDER BY tvl DESC NULLS LAST
            "#,
        )
        .bind(chain_id)
        .fetch_all(&self.pool)
        .await?;

        let contracts = rows
            .into_iter()
            .map(|row| {
                let data_source_str: String = row.try_get("data_source")?;
                let data_source = crate::models::DataSource::from_str(&data_source_str)
                    .unwrap_or(crate::models::DataSource::Manual);

                Ok(Contract {
                    address: row.try_get("address")?,
                    chain: row.try_get("chain")?,
                    chain_id: row.try_get("chain_id")?,
                    name: row.try_get("name")?,
                    symbol: row.try_get("symbol")?,
                    description: row.try_get("description")?,
                    is_proxy: row.try_get("is_proxy")?,
                    implementation_address: row.try_get("implementation_address")?,
                    source_code: row.try_get("source_code")?,
                    abi: row.try_get("abi")?,
                    tvl: row.try_get("tvl")?,
                    transaction_count: row.try_get("transaction_count")?,
                    last_activity_at: row.try_get("last_activity_at")?,
                    data_source,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(contracts)
    }

    /// Get total contract count
    pub async fn get_contract_count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM contracts")
            .fetch_one(&self.pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        Ok(count)
    }

    /// Get top contracts by TVL
    pub async fn get_top_by_tvl(&self, limit: i64) -> Result<Vec<Contract>> {
        let rows = sqlx::query(
            r#"
            SELECT
                address, chain, chain_id, name, symbol, description,
                is_proxy, implementation_address, source_code, abi,
                tvl, transaction_count, last_activity_at, data_source
            FROM contracts
            WHERE tvl IS NOT NULL
            ORDER BY tvl DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let contracts = rows
            .into_iter()
            .map(|row| {
                let data_source_str: String = row.try_get("data_source")?;
                let data_source = crate::models::DataSource::from_str(&data_source_str)
                    .unwrap_or(crate::models::DataSource::Manual);

                Ok(Contract {
                    address: row.try_get("address")?,
                    chain: row.try_get("chain")?,
                    chain_id: row.try_get("chain_id")?,
                    name: row.try_get("name")?,
                    symbol: row.try_get("symbol")?,
                    description: row.try_get("description")?,
                    is_proxy: row.try_get("is_proxy")?,
                    implementation_address: row.try_get("implementation_address")?,
                    source_code: row.try_get("source_code")?,
                    abi: row.try_get("abi")?,
                    tvl: row.try_get("tvl")?,
                    transaction_count: row.try_get("transaction_count")?,
                    last_activity_at: row.try_get("last_activity_at")?,
                    data_source,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(contracts)
    }
}
