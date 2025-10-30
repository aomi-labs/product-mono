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

#[cfg(test)]
mod tests {
    use super::*;
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
                source_code TEXT NOT NULL,
                abi TEXT NOT NULL,
                PRIMARY KEY (chain, address)
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
            source_code: "contract Test {}".to_string(),
            abi: json!({"test": "abi"}),
        };

        // Store the contract
        store.store_contract(contract.clone()).await?;

        // Retrieve the contract
        let retrieved = store
            .get_contract("ethereum".to_string(), "0x123".to_string())
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
            source_code: "contract Test {}".to_string(),
            abi: json!({"inputs": [], "outputs": []}),
        };

        store.store_contract(contract).await?;

        let abi = store
            .get_abi("polygon".to_string(), "0x456".to_string())
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
            source_code: "contract A {}".to_string(),
            abi: json!({}),
        };

        let contract2 = Contract {
            address: "0x222".to_string(),
            chain: "ethereum".to_string(),
            source_code: "contract B {}".to_string(),
            abi: json!({}),
        };

        let contract3 = Contract {
            address: "0x333".to_string(),
            chain: "polygon".to_string(),
            source_code: "contract C {}".to_string(),
            abi: json!({}),
        };

        store.store_contract(contract1).await?;
        store.store_contract(contract2).await?;
        store.store_contract(contract3).await?;

        let ethereum_contracts = store.get_contracts_by_chain("ethereum".to_string()).await?;
        assert_eq!(ethereum_contracts.len(), 2);

        let polygon_contracts = store.get_contracts_by_chain("polygon".to_string()).await?;
        assert_eq!(polygon_contracts.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_contract() -> Result<()> {
        let store = setup_test_store().await?;

        let contract = Contract {
            address: "0x789".to_string(),
            chain: "arbitrum".to_string(),
            source_code: "contract Test {}".to_string(),
            abi: json!({}),
        };

        store.store_contract(contract).await?;

        // Verify it exists
        let retrieved = store
            .get_contract("arbitrum".to_string(), "0x789".to_string())
            .await?;
        assert!(retrieved.is_some());

        // Delete it
        store
            .delete_contract("arbitrum".to_string(), "0x789".to_string())
            .await?;

        // Verify it's gone
        let retrieved = store
            .get_contract("arbitrum".to_string(), "0x789".to_string())
            .await?;
        assert!(retrieved.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_contract() -> Result<()> {
        let store = setup_test_store().await?;

        let contract = Contract {
            address: "0xabc".to_string(),
            chain: "optimism".to_string(),
            source_code: "contract V1 {}".to_string(),
            abi: json!({"version": 1}),
        };

        // Insert
        store.store_contract(contract).await?;

        // Update with same address/chain
        let updated_contract = Contract {
            address: "0xabc".to_string(),
            chain: "optimism".to_string(),
            source_code: "contract V2 {}".to_string(),
            abi: json!({"version": 2}),
        };

        store.store_contract(updated_contract).await?;

        // Should only have one contract
        let contracts = store.get_contracts_by_chain("optimism".to_string()).await?;
        assert_eq!(contracts.len(), 1);

        // Should have updated values
        let retrieved = contracts[0].clone();
        assert_eq!(retrieved.source_code, "contract V2 {}");
        assert_eq!(retrieved.abi, json!({"version": 2}));

        Ok(())
    }

    // Integration test with actual PostgreSQL database using real USDC contract
    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_postgres_integration_usdc() -> Result<()> {
        // Install drivers for Any pool
        sqlx::any::install_default_drivers();

        // Connect to actual PostgreSQL
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kevin@localhost:5432/chatbot".to_string());

        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let store = ContractStore::new(pool);

        // Real USDC (Circle) contract source code (core implementation, simplified)
        let source_code = "// SPDX-License-Identifier: MIT\npragma solidity 0.6.12;\n\ncontract FiatTokenV2_1 {\n    string public name = \"USD Coin\";\n    string public symbol = \"USDC\";\n    uint8 public decimals = 6;\n    string public currency = \"USD\";\n    address public masterMinter;\n    bool internal initialized;\n    mapping(address => uint256) internal balances;\n    mapping(address => mapping(address => uint256)) internal allowed;\n    uint256 internal totalSupply_ = 0;\n    mapping(address => bool) internal minters;\n    mapping(address => uint256) internal minterAllowed;\n    address public pauser;\n    bool public paused = false;\n    address public blacklister;\n    mapping(address => bool) internal blacklisted;\n    address public owner;\n    function transfer(address _to, uint256 _value) external returns (bool) { require(_to != address(0)); require(_value <= balances[msg.sender]); balances[msg.sender] = balances[msg.sender] - _value; balances[_to] = balances[_to] + _value; return true; }\n    function approve(address _spender, uint256 _value) external returns (bool) { allowed[msg.sender][_spender] = _value; return true; }\n    function transferFrom(address _from, address _to, uint256 _value) external returns (bool) { require(_to != address(0)); require(_value <= balances[_from]); require(_value <= allowed[_from][msg.sender]); balances[_from] = balances[_from] - _value; balances[_to] = balances[_to] + _value; allowed[_from][msg.sender] = allowed[_from][msg.sender] - _value; return true; }\n    function balanceOf(address account) external view returns (uint256) { return balances[account]; }\n    function mint(address _to, uint256 _amount) external returns (bool) { require(_to != address(0)); require(_amount > 0); require(minters[msg.sender] == true); require(_amount <= minterAllowed[msg.sender]); totalSupply_ = totalSupply_ + _amount; balances[_to] = balances[_to] + _amount; minterAllowed[msg.sender] = minterAllowed[msg.sender] - _amount; return true; }\n    function burn(uint256 _amount) external { uint256 balance = balances[msg.sender]; require(_amount > 0); require(balance >= _amount); totalSupply_ = totalSupply_ - _amount; balances[msg.sender] = balance - _amount; }\n}";

        // Real USDC ABI (key functions)
        let abi = json!([{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"owner","type":"address"},{"indexed":true,"internalType":"address","name":"spender","type":"address"},{"indexed":false,"internalType":"uint256","name":"value","type":"uint256"}],"name":"Approval","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"from","type":"address"},{"indexed":true,"internalType":"address","name":"to","type":"address"},{"indexed":false,"internalType":"uint256","name":"value","type":"uint256"}],"name":"Transfer","type":"event"},{"inputs":[{"internalType":"address","name":"spender","type":"address"},{"internalType":"uint256","name":"value","type":"uint256"}],"name":"approve","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"account","type":"address"}],"name":"balanceOf","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"burn","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[],"name":"decimals","outputs":[{"internalType":"uint8","name":"","type":"uint8"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"_to","type":"address"},{"internalType":"uint256","name":"_amount","type":"uint256"}],"name":"mint","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[],"name":"name","outputs":[{"internalType":"string","name":"","type":"string"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"symbol","outputs":[{"internalType":"string","name":"","type":"string"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"totalSupply","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"to","type":"address"},{"internalType":"uint256","name":"value","type":"uint256"}],"name":"transfer","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"from","type":"address"},{"internalType":"address","name":"to","type":"address"},{"internalType":"uint256","name":"value","type":"uint256"}],"name":"transferFrom","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"}]);

        let contract = Contract {
            address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(), // Real USDC address on Ethereum
            chain: "ethereum".to_string(),
            source_code: source_code.to_string(),
            abi,
        };

        // Test: Store the contract
        println!("Storing USDC contract to PostgreSQL...");
        store.store_contract(contract.clone()).await?;

        // Test: Retrieve the contract
        println!("Retrieving USDC contract from PostgreSQL...");
        let retrieved = store
            .get_contract(
                "ethereum".to_string(),
                "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            )
            .await?;

        assert!(retrieved.is_some(), "USDC contract should be found");
        let retrieved = retrieved.unwrap();

        // Verify all fields
        assert_eq!(
            retrieved.address,
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        );
        assert_eq!(retrieved.chain, "ethereum");
        assert!(retrieved.source_code.contains("USD Coin"));
        assert!(retrieved.source_code.contains("USDC"));
        assert!(retrieved.source_code.contains("function mint"));
        assert!(retrieved.source_code.contains("function burn"));

        // Verify ABI structure
        assert!(retrieved.abi.is_array());
        let abi_array = retrieved.abi.as_array().unwrap();
        assert!(abi_array.len() > 0);

        // Check for USDC-specific functions in ABI
        let has_mint = abi_array.iter().any(|item| {
            item.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n == "mint")
                .unwrap_or(false)
        });
        assert!(has_mint, "ABI should contain mint function");

        let has_burn = abi_array.iter().any(|item| {
            item.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n == "burn")
                .unwrap_or(false)
        });
        assert!(has_burn, "ABI should contain burn function");

        // Test: Get just the ABI
        println!("Retrieving USDC ABI from PostgreSQL...");
        let abi_only = store
            .get_abi(
                "ethereum".to_string(),
                "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            )
            .await?;

        assert!(abi_only.is_some());
        let abi_only = abi_only.unwrap();
        assert!(abi_only.is_array());

        // Test: Get contracts by chain
        println!("Querying contracts by chain...");
        let ethereum_contracts = store.get_contracts_by_chain("ethereum".to_string()).await?;
        assert!(ethereum_contracts.len() >= 1);
        assert!(
            ethereum_contracts
                .iter()
                .any(|c| c.address == "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")
        );

        // Test: Update contract (UPSERT)
        println!("Testing UPSERT functionality...");
        let updated_contract = Contract {
            address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            chain: "ethereum".to_string(),
            source_code: "// Updated USDC Contract\n".to_string() + source_code,
            abi: contract.abi.clone(),
        };

        store.store_contract(updated_contract).await?;

        let after_update = store
            .get_contract(
                "ethereum".to_string(),
                "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            )
            .await?;

        assert!(after_update.is_some());
        assert!(
            after_update
                .unwrap()
                .source_code
                .contains("// Updated USDC Contract")
        );

        // Cleanup: Delete the test contract
        println!("Cleaning up test data...");
        store
            .delete_contract(
                "ethereum".to_string(),
                "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            )
            .await?;

        let after_delete = store
            .get_contract(
                "ethereum".to_string(),
                "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            )
            .await?;

        assert!(after_delete.is_none(), "USDC contract should be deleted");

        println!("✓ All PostgreSQL integration tests with USDC contract passed!");
        Ok(())
    }
}
