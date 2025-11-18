use super::traits::ContractStoreApi;
use super::{Contract, ContractSearchParams};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::{FromRow, Pool, any::Any};

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
        let query = "SELECT address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, tags, is_proxy, data_source, created_at, updated_at FROM contracts WHERE chain_id = $1 AND address = $2";

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
        let query = "INSERT INTO contracts (address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, tags, is_proxy, data_source, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT (chain_id, address) DO UPDATE SET
                chain = EXCLUDED.chain,
                source_code = EXCLUDED.source_code,
                abi = EXCLUDED.abi,
                name = EXCLUDED.name,
                symbol = EXCLUDED.symbol,
                protocol = EXCLUDED.protocol,
                contract_type = EXCLUDED.contract_type,
                version = EXCLUDED.version,
                tags = EXCLUDED.tags,
                is_proxy = EXCLUDED.is_proxy,
                data_source = EXCLUDED.data_source,
                updated_at = EXCLUDED.updated_at";

        let abi_string = serde_json::to_string(&contract.abi)?;

        sqlx::query::<Any>(query)
            .bind(&contract.address)
            .bind(&contract.chain)
            .bind(contract.chain_id as i32)
            .bind(&contract.source_code)
            .bind(&abi_string)
            .bind(&contract.name)
            .bind(&contract.symbol)
            .bind(&contract.protocol)
            .bind(&contract.contract_type)
            .bind(&contract.version)
            .bind(&contract.tags)
            .bind(&contract.is_proxy)
            .bind(&contract.data_source)
            .bind(&contract.created_at)
            .bind(&contract.updated_at)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_contracts_by_chain(&self, chain_id: u32) -> Result<Vec<Contract>> {
        let query = "SELECT address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, tags, is_proxy, data_source, created_at, updated_at FROM contracts WHERE chain_id = $1";

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

    async fn search_contracts(&self, params: ContractSearchParams) -> Result<Vec<Contract>> {
        // Fuzzy search priority strategy (excluding address - use get_contract for that):
        // 1. Exact symbol match (fast, indexed)
        // 2. Combined filters: contract_type (exact) + protocol (fuzzy) + version (exact)
        // 3. Tag matching (CSV fuzzy contains)
        // 4. Name fuzzy search (fallback)

        // Strategy 1: Exact symbol match
        if let Some(ref sym) = params.symbol {
            let query = if params.chain_id.is_some() {
                "SELECT address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, tags, is_proxy, data_source, created_at, updated_at FROM contracts WHERE chain_id = $1 AND symbol = $2"
            } else {
                "SELECT address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, tags, is_proxy, data_source, created_at, updated_at FROM contracts WHERE symbol = $1"
            };

            let mut q = sqlx::query_as::<Any, Contract>(query);
            if let Some(cid) = params.chain_id {
                q = q.bind(cid as i32).bind(sym);
            } else {
                q = q.bind(sym);
            }

            let contracts = q.fetch_all(&self.pool).await?;

            if !contracts.is_empty() {
                return Ok(contracts);
            }
        }

        // Strategy 2: Combined filters (contract_type + protocol + version)
        if params.contract_type.is_some() || params.protocol.is_some() || params.version.is_some() {
            let mut query = "SELECT address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, tags, is_proxy, data_source, created_at, updated_at FROM contracts WHERE 1=1".to_string();
            let mut bind_idx = 1;

            if params.chain_id.is_some() {
                query.push_str(&format!(" AND chain_id = ${}", bind_idx));
                bind_idx += 1;
            }
            if params.contract_type.is_some() {
                query.push_str(&format!(" AND contract_type = ${}", bind_idx));
                bind_idx += 1;
            }
            if params.protocol.is_some() {
                query.push_str(&format!(" AND LOWER(protocol) LIKE LOWER(${})", bind_idx));
                bind_idx += 1;
            }
            if params.version.is_some() {
                query.push_str(&format!(" AND version = ${}", bind_idx));
            }

            let mut q = sqlx::query(&query);

            if let Some(cid) = params.chain_id {
                q = q.bind(cid as i32);
            }
            if let Some(ref ct) = params.contract_type {
                q = q.bind(ct);
            }
            if let Some(ref proto) = params.protocol {
                q = q.bind(format!("%{}%", proto));
            }
            if let Some(ref ver) = params.version {
                q = q.bind(ver);
            }

            let rows = q.fetch_all(&self.pool).await?;
            let contracts: Result<Vec<Contract>, sqlx::Error> =
                rows.iter().map(|row| Contract::from_row(row)).collect();

            let contracts = contracts?;
            if !contracts.is_empty() {
                return Ok(contracts);
            }
        }

        // Strategy 3: Tag matching (CSV contains)
        if let Some(ref tags) = params.tags {
            let tag_list: Vec<&str> = tags.split(',').map(|t| t.trim()).collect();
            let mut query = "SELECT address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, tags, is_proxy, data_source, created_at, updated_at FROM contracts WHERE 1=1".to_string();
            let mut bind_idx = 1;

            if params.chain_id.is_some() {
                query.push_str(&format!(" AND chain_id = ${}", bind_idx));
                bind_idx += 1;
            }

            query.push_str(" AND (");
            for (i, _) in tag_list.iter().enumerate() {
                if i > 0 {
                    query.push_str(" OR ");
                }
                query.push_str(&format!("tags LIKE ${}", bind_idx + i));
            }
            query.push(')');

            let mut q = sqlx::query(&query);
            if let Some(cid) = params.chain_id {
                q = q.bind(cid as i32);
            }
            for tag in &tag_list {
                q = q.bind(format!("%{}%", tag));
            }

            let rows = q.fetch_all(&self.pool).await?;
            let contracts: Result<Vec<Contract>, sqlx::Error> =
                rows.iter().map(|row| Contract::from_row(row)).collect();

            let contracts = contracts?;
            if !contracts.is_empty() {
                return Ok(contracts);
            }
        }

        // Strategy 4: Name fuzzy search (fallback)
        if let Some(ref name) = params.name {
            let query = if params.chain_id.is_some() {
                "SELECT address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, tags, is_proxy, data_source, created_at, updated_at FROM contracts WHERE chain_id = $1 AND LOWER(name) LIKE LOWER($2)"
            } else {
                "SELECT address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, tags, is_proxy, data_source, created_at, updated_at FROM contracts WHERE LOWER(name) LIKE LOWER($1)"
            };

            let mut q = sqlx::query_as::<Any, Contract>(query);
            if let Some(cid) = params.chain_id {
                q = q.bind(cid as i32).bind(format!("%{}%", name));
            } else {
                q = q.bind(format!("%{}%", name));
            }

            let contracts = q.fetch_all(&self.pool).await?;

            if !contracts.is_empty() {
                return Ok(contracts);
            }
        }

        // No results found
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::etherscan::{ETHEREUM_MAINNET, OPTIMISM, POLYGON};
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
                name TEXT,
                symbol TEXT,
                protocol TEXT,
                contract_type TEXT,
                version TEXT,
                tags TEXT,
                is_proxy INTEGER,
                data_source TEXT,
                created_at INTEGER,
                updated_at INTEGER,
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
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            tags: None,
            is_proxy: None,
            data_source: None,
            created_at: None,
            updated_at: None,
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
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            tags: None,
            is_proxy: None,
            data_source: None,
            created_at: None,
            updated_at: None,
        };

        store.store_contract(contract).await?;

        let abi = store.get_abi(POLYGON, "0x456".to_string()).await?;

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
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            tags: None,
            is_proxy: None,
            data_source: None,
            created_at: None,
            updated_at: None,
        };

        let contract2 = Contract {
            address: "0x222".to_string(),
            chain: "ethereum".to_string(),
            chain_id: ETHEREUM_MAINNET,
            source_code: "contract B {}".to_string(),
            abi: json!({}),
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            tags: None,
            is_proxy: None,
            data_source: None,
            created_at: None,
            updated_at: None,
        };

        let contract3 = Contract {
            address: "0x333".to_string(),
            chain: "polygon".to_string(),
            chain_id: POLYGON,
            source_code: "contract C {}".to_string(),
            abi: json!({}),
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            tags: None,
            is_proxy: None,
            data_source: None,
            created_at: None,
            updated_at: None,
        };

        store.store_contract(contract1).await?;
        store.store_contract(contract2).await?;
        store.store_contract(contract3).await?;

        let contracts = store.get_contracts_by_chain(ETHEREUM_MAINNET).await?;

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
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            tags: None,
            is_proxy: None,
            data_source: None,
            created_at: None,
            updated_at: None,
        };

        store.store_contract(contract.clone()).await?;

        store.delete_contract(OPTIMISM, "0x789".to_string()).await?;

        let retrieved = store.get_contract(OPTIMISM, "0x789".to_string()).await?;

        assert!(retrieved.is_none());

        Ok(())
    }
}
