use super::traits::ContractStoreApi;
use super::{Contract, ContractSearchParams, ContractUpdate};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, Pool, QueryBuilder, Row, any::Any};
use tracing::warn;

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
        let query = "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at FROM contracts WHERE chain_id = $1 AND address = $2";

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
        let query = "INSERT INTO contracts (address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT (chain_id, address) DO UPDATE SET
                chain = EXCLUDED.chain,
                source_code = EXCLUDED.source_code,
                abi = EXCLUDED.abi,
                description = EXCLUDED.description,
                name = EXCLUDED.name,
                symbol = EXCLUDED.symbol,
                protocol = EXCLUDED.protocol,
                contract_type = EXCLUDED.contract_type,
                version = EXCLUDED.version,
                is_proxy = EXCLUDED.is_proxy,
                implementation_address = EXCLUDED.implementation_address,
                updated_at = EXCLUDED.updated_at";

        let abi_string = serde_json::to_string(&contract.abi)?;
        let name = contract
            .name
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        let is_proxy = contract.is_proxy.unwrap_or(false);
        let created_at = contract
            .created_at
            .unwrap_or_else(|| Utc::now().timestamp());
        let updated_at = contract
            .updated_at
            .unwrap_or_else(|| Utc::now().timestamp());

        sqlx::query::<Any>(query)
            .bind(&contract.address)
            .bind(&contract.chain)
            .bind(contract.chain_id as i32)
            .bind(&contract.source_code)
            .bind(&abi_string)
            .bind(&contract.description)
            .bind(&name)
            .bind(&contract.symbol)
            .bind(&contract.protocol)
            .bind(&contract.contract_type)
            .bind(&contract.version)
            .bind(is_proxy)
            .bind(&contract.implementation_address)
            .bind(created_at)
            .bind(updated_at)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_contracts_by_chain(&self, chain_id: u32) -> Result<Vec<Contract>> {
        let query = "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at FROM contracts WHERE chain_id = $1";

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
        // Fuzzy search priority strategy:
        // 1. Address match (exact, optional chain_id)
        // 2. Symbol match (case-insensitive)
        // 3. Combined filters: contract_type (exact) + protocol (fuzzy) + version (exact)
        // 4. Name fuzzy search (fallback)

        // Strategy 1: Address match
        if let Some(ref addr) = params.address {
            let query = if params.chain_id.is_some() {
                "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at FROM contracts WHERE chain_id = $1 AND LOWER(address) = LOWER($2)"
            } else {
                "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at FROM contracts WHERE LOWER(address) = LOWER($1)"
            };

            let mut q = sqlx::query_as::<Any, Contract>(query);
            if let Some(cid) = params.chain_id {
                q = q.bind(cid as i32).bind(addr);
            } else {
                q = q.bind(addr);
            }

            let contracts = q.fetch_all(&self.pool).await?;

            if !contracts.is_empty() {
                return Ok(contracts);
            }
        }

        // Strategy 2: Symbol match (case-insensitive)
        if let Some(ref sym) = params.symbol {
            let query = if params.chain_id.is_some() {
                "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at FROM contracts WHERE chain_id = $1 AND LOWER(symbol) = LOWER($2)"
            } else {
                "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at FROM contracts WHERE LOWER(symbol) = LOWER($1)"
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
            let mut query = "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at FROM contracts WHERE 1=1".to_string();
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
                rows.iter().map(Contract::from_row).collect();

            let contracts = contracts?;
            if !contracts.is_empty() {
                return Ok(contracts);
            }
        }

        // Strategy 3: Name fuzzy search (fallback)
        if let Some(ref name) = params.name {
            let normalized = normalize_name(name);
            let query = if params.chain_id.is_some() {
                "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at
                 FROM contracts
                 WHERE chain_id = $1
                   AND (LOWER(name) LIKE LOWER($2)
                        OR LOWER(REPLACE(REPLACE(REPLACE(name, ' ', ''), '-', ''), '_', '')) LIKE LOWER($3))"
            } else {
                "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at
                 FROM contracts
                 WHERE LOWER(name) LIKE LOWER($1)
                    OR LOWER(REPLACE(REPLACE(REPLACE(name, ' ', ''), '-', ''), '_', '')) LIKE LOWER($2)"
            };

            let mut q = sqlx::query_as::<Any, Contract>(query);
            if let Some(cid) = params.chain_id {
                q = q
                    .bind(cid as i32)
                    .bind(format!("%{}%", name))
                    .bind(format!("%{}%", normalized));
            } else {
                q = q
                    .bind(format!("%{}%", name))
                    .bind(format!("%{}%", normalized));
            }

            let contracts = q.fetch_all(&self.pool).await?;

            if !contracts.is_empty() {
                return Ok(contracts);
            }
        }

        // No results found
        Ok(vec![])
    }

    async fn list_contracts(
        &self,
        params: ContractSearchParams,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Contract>> {
        let mut query = QueryBuilder::<Any>::new(
            "SELECT address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at FROM contracts WHERE 1=1",
        );

        if let Some(chain_id) = params.chain_id {
            query.push(" AND chain_id = ").push_bind(chain_id as i32);
        }

        if let Some(address) = params.address {
            query.push(" AND address = ").push_bind(address);
        }

        if let Some(symbol) = params.symbol {
            query.push(" AND symbol = ").push_bind(symbol);
        }

        if let Some(name) = params.name {
            let normalized = normalize_name(&name);
            query
                .push(" AND (LOWER(name) LIKE LOWER(")
                .push_bind(format!("%{name}%"))
                .push(")")
                .push(" OR LOWER(REPLACE(REPLACE(REPLACE(name, ' ', ''), '-', ''), '_', '')) LIKE LOWER(")
                .push_bind(format!("%{normalized}%"))
                .push("))");
        }

        if let Some(protocol) = params.protocol {
            query
                .push(" AND LOWER(protocol) LIKE LOWER(")
                .push_bind(format!("%{protocol}%"))
                .push(")");
        }

        if let Some(contract_type) = params.contract_type {
            query.push(" AND contract_type = ").push_bind(contract_type);
        }

        if let Some(version) = params.version {
            query.push(" AND version = ").push_bind(version);
        }

        query.push(" ORDER BY updated_at DESC");

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit);
        }

        if let Some(offset) = offset {
            query.push(" OFFSET ").push_bind(offset);
        }

        let rows = query.build().fetch_all(&self.pool).await?;
        let mut contracts = Vec::with_capacity(rows.len());

        for row in rows {
            match Contract::from_row(&row) {
                Ok(contract) => contracts.push(contract),
                Err(err) => {
                    let address: Option<String> = row.try_get("address").ok();
                    let chain_id: Option<i32> = row.try_get("chain_id").ok();
                    warn!(
                        ?address,
                        ?chain_id,
                        error = %err,
                        "invalid contract row; skipping"
                    );
                }
            }
        }

        Ok(contracts)
    }

    async fn update_contract(&self, update: ContractUpdate) -> Result<Contract> {
        if update.clear_symbol && update.symbol.is_some() {
            anyhow::bail!("cannot set symbol and clear_symbol together");
        }

        if update.clear_protocol && update.protocol.is_some() {
            anyhow::bail!("cannot set protocol and clear_protocol together");
        }

        if update.clear_contract_type && update.contract_type.is_some() {
            anyhow::bail!("cannot set contract_type and clear_contract_type together");
        }

        if update.clear_version && update.version.is_some() {
            anyhow::bail!("cannot set version and clear_version together");
        }

        if update.clear_implementation_address && update.implementation_address.is_some() {
            anyhow::bail!(
                "cannot set implementation_address and clear_implementation_address together"
            );
        }

        if update.clear_description && update.description.is_some() {
            anyhow::bail!("cannot set description and clear_description together");
        }

        let mut updates = 0;

        if update.name.is_some() {
            updates += 1;
        }
        if update.symbol.is_some() || update.clear_symbol {
            updates += 1;
        }
        if update.protocol.is_some() || update.clear_protocol {
            updates += 1;
        }
        if update.contract_type.is_some() || update.clear_contract_type {
            updates += 1;
        }
        if update.version.is_some() || update.clear_version {
            updates += 1;
        }
        if update.is_proxy.is_some() {
            updates += 1;
        }
        if update.implementation_address.is_some() || update.clear_implementation_address {
            updates += 1;
        }
        if update.description.is_some() || update.clear_description {
            updates += 1;
        }

        if updates == 0 {
            anyhow::bail!("no fields provided to update");
        }

        let now = Utc::now().timestamp();
        let row = sqlx::query_as::<Any, Contract>(
            r#"
            UPDATE contracts SET
                name = COALESCE($1, name),
                symbol = CASE WHEN $2 THEN NULL ELSE COALESCE($3, symbol) END,
                protocol = CASE WHEN $4 THEN NULL ELSE COALESCE($5, protocol) END,
                contract_type = CASE WHEN $6 THEN NULL ELSE COALESCE($7, contract_type) END,
                version = CASE WHEN $8 THEN NULL ELSE COALESCE($9, version) END,
                is_proxy = COALESCE($10, is_proxy),
                implementation_address = CASE WHEN $11 THEN NULL ELSE COALESCE($12, implementation_address) END,
                description = CASE WHEN $13 THEN NULL ELSE COALESCE($14, description) END,
                updated_at = $15
            WHERE chain_id = $16 AND address = $17
            RETURNING address, chain, chain_id, source_code, abi, description, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, created_at, updated_at
            "#,
        )
        .bind(update.name)
        .bind(update.clear_symbol)
        .bind(update.symbol)
        .bind(update.clear_protocol)
        .bind(update.protocol)
        .bind(update.clear_contract_type)
        .bind(update.contract_type)
        .bind(update.clear_version)
        .bind(update.version)
        .bind(update.is_proxy)
        .bind(update.clear_implementation_address)
        .bind(update.implementation_address)
        .bind(update.clear_description)
        .bind(update.description)
        .bind(now)
        .bind(update.chain_id as i32)
        .bind(update.address)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != '-' && *c != '_')
        .collect()
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
                description TEXT,
                name TEXT,
                symbol TEXT,
                protocol TEXT,
                contract_type TEXT,
                version TEXT,
                is_proxy INTEGER,
                implementation_address TEXT,
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
            description: None,
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: None,
            implementation_address: None,
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
            description: None,
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: None,
            implementation_address: None,
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
            description: None,
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: None,
            implementation_address: None,
            created_at: None,
            updated_at: None,
        };

        let contract2 = Contract {
            address: "0x222".to_string(),
            chain: "ethereum".to_string(),
            chain_id: ETHEREUM_MAINNET,
            source_code: "contract B {}".to_string(),
            abi: json!({}),
            description: None,
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: None,
            implementation_address: None,
            created_at: None,
            updated_at: None,
        };

        let contract3 = Contract {
            address: "0x333".to_string(),
            chain: "polygon".to_string(),
            chain_id: POLYGON,
            source_code: "contract C {}".to_string(),
            abi: json!({}),
            description: None,
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: None,
            implementation_address: None,
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
            description: None,
            name: None,
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: None,
            implementation_address: None,
            created_at: None,
            updated_at: None,
        };

        store.store_contract(contract.clone()).await?;

        store.delete_contract(OPTIMISM, "0x789".to_string()).await?;

        let retrieved = store.get_contract(OPTIMISM, "0x789".to_string()).await?;

        assert!(retrieved.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_update_contract_metadata() -> Result<()> {
        let store = setup_test_store().await?;

        let contract = Contract {
            address: "0xabc".to_string(),
            chain: "ethereum".to_string(),
            chain_id: ETHEREUM_MAINNET,
            source_code: "contract Test {}".to_string(),
            abi: json!({}),
            description: None,
            name: Some("Unknown".to_string()),
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: Some(false),
            implementation_address: None,
            created_at: None,
            updated_at: None,
        };

        store.store_contract(contract).await?;

        let updated = store
            .update_contract(ContractUpdate {
                chain_id: ETHEREUM_MAINNET,
                address: "0xabc".to_string(),
                name: Some("USD Coin".to_string()),
                symbol: Some("USDC".to_string()),
                clear_symbol: false,
                protocol: Some("Centre".to_string()),
                clear_protocol: false,
                contract_type: Some("ERC20".to_string()),
                clear_contract_type: false,
                version: None,
                clear_version: false,
                is_proxy: Some(false),
                implementation_address: None,
                clear_implementation_address: false,
                description: Some("USD Coin stablecoin".to_string()),
                clear_description: false,
            })
            .await?;

        assert_eq!(updated.name.as_deref(), Some("USD Coin"));
        assert_eq!(updated.symbol.as_deref(), Some("USDC"));
        assert_eq!(updated.protocol.as_deref(), Some("Centre"));
        assert_eq!(updated.contract_type.as_deref(), Some("ERC20"));
        assert_eq!(updated.description.as_deref(), Some("USD Coin stablecoin"));

        Ok(())
    }

    #[tokio::test]
    async fn test_update_contract_clear_fields() -> Result<()> {
        let store = setup_test_store().await?;

        let contract = Contract {
            address: "0xdef".to_string(),
            chain: "ethereum".to_string(),
            chain_id: ETHEREUM_MAINNET,
            source_code: "contract Test {}".to_string(),
            abi: json!({}),
            description: Some("Old description".to_string()),
            name: Some("Old Name".to_string()),
            symbol: Some("OLD".to_string()),
            protocol: Some("Old Protocol".to_string()),
            contract_type: Some("ERC20".to_string()),
            version: Some("v1".to_string()),
            is_proxy: Some(false),
            implementation_address: Some("0ximpl".to_string()),
            created_at: None,
            updated_at: None,
        };

        store.store_contract(contract).await?;

        let updated = store
            .update_contract(ContractUpdate {
                chain_id: ETHEREUM_MAINNET,
                address: "0xdef".to_string(),
                name: None,
                symbol: None,
                clear_symbol: true,
                protocol: None,
                clear_protocol: true,
                contract_type: None,
                clear_contract_type: true,
                version: None,
                clear_version: true,
                is_proxy: Some(true),
                implementation_address: None,
                clear_implementation_address: true,
                description: None,
                clear_description: true,
            })
            .await?;

        assert_eq!(updated.symbol, None);
        assert_eq!(updated.protocol, None);
        assert_eq!(updated.contract_type, None);
        assert_eq!(updated.version, None);
        assert_eq!(updated.implementation_address, None);
        assert_eq!(updated.description, None);
        assert_eq!(updated.is_proxy, Some(true));

        Ok(())
    }
}
