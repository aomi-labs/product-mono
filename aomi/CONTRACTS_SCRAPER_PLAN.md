# Contract Scraper CLI - Implementation Plan

## Overview
Build a CLI tool to scrape top DeFi contracts from multiple sources, enrich with metadata, and store in the contracts database with enhanced schema.

## Location
- **Binary Path**: `/Users/kevin/foameo/product-mono/aomi/bin/contract-scraper`
- **Crate**: `contract-scraper` (new crate in `bin/`)

## Updated Schema

```sql
CREATE TABLE contracts (
    -- Core identification
    address TEXT NOT NULL,
    chain TEXT NOT NULL,
    chain_id INTEGER NOT NULL,

    -- Metadata
    name TEXT NOT NULL,
    symbol TEXT,
    description TEXT,

    -- Proxy information
    is_proxy BOOLEAN NOT NULL DEFAULT false,
    implementation_address TEXT,

    -- Contract code
    source_code TEXT NOT NULL,
    abi TEXT NOT NULL,

    -- Metrics (new)
    tvl DECIMAL,
    transaction_count BIGINT,
    last_activity_at BIGINT,

    -- Source tracking (new)
    data_source TEXT NOT NULL, -- 'defillama', 'coingecko', 'etherscan', 'manual'

    -- Timestamps (new)
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,

    PRIMARY KEY (chain_id, address)
);

CREATE INDEX IF NOT EXISTS idx_contracts_tvl ON contracts(tvl DESC) WHERE tvl IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_tx_count ON contracts(transaction_count DESC) WHERE transaction_count IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_updated ON contracts(updated_at DESC);
```

## Data Sources

### 1. DeFi Llama
- **Endpoint**: `https://api.llama.fi/protocols`
- **Data**: Protocol name, TVL, chains, contract addresses
- **Rate Limit**: Unknown, implement backoff
- **Docs**: https://defillama.com/docs/api

### 2. CoinGecko
- **Endpoint**: `https://api.coingecko.com/api/v3/coins/list?include_platform=true`
- **Data**: Token metadata, contract addresses per chain
- **Rate Limit**: 10-50 calls/min (free tier)
- **Docs**: https://www.coingecko.com/en/api/documentation

### 3. Etherscan (and chain-specific explorers)
- **Endpoints**:
  - `https://api.etherscan.io/api?module=contract&action=getsourcecode&address={address}`
  - `https://api.etherscan.io/api?module=account&action=txlist&address={address}`
- **Data**: Source code, ABI, transaction count, proxy detection
- **Rate Limit**: 5 calls/sec (free tier)
- **API Keys**: Need keys for each chain (mainnet, polygon, arbitrum, base, optimism)

## Implementation Phases

### Phase 1: Project Setup
**Goal**: Create basic CLI structure with dependencies

**Tasks**:
1. Create new binary crate `contract-scraper` in `/Users/kevin/foameo/product-mono/aomi/bin/`
2. Add dependencies to `Cargo.toml`:
   ```toml
   [dependencies]
   anyhow = { workspace = true }
   clap = { version = "4.0", features = ["derive"] }
   tokio = { workspace = true, features = ["full"] }
   reqwest = { version = "0.11", features = ["json"] }
   serde = { workspace = true, features = ["derive"] }
   serde_json = { workspace = true }
   sqlx = { workspace = true }
   tracing = { workspace = true }
   tracing-subscriber = { workspace = true }
   aomi-tools = { path = "../../crates/tools" }
   ```
3. Set up CLI structure with clap:
   ```rust
   #[derive(Parser)]
   struct Cli {
       #[command(subcommand)]
       command: Commands,
   }

   #[derive(Subcommand)]
   enum Commands {
       /// Scrape contracts from all sources
       Scrape {
           #[arg(short, long, default_value = "100")]
           limit: usize,

           #[arg(short, long)]
           chains: Vec<String>,
       },

       /// Update existing contracts
       Update {
           #[arg(short, long)]
           chain_id: Option<i32>,
       },

       /// Verify contract data
       Verify {
           #[arg(short, long)]
           address: String,

           #[arg(short, long)]
           chain_id: i32,
       },
   }
   ```

**Acceptance Criteria**:
- ✅ CLI binary compiles and runs
- ✅ `--help` flag shows usage
- ✅ All dependencies resolve

---

### Phase 2: Database Migration
**Goal**: Update contracts table schema

**Tasks**:
1. Create new migration: `/Users/kevin/foameo/product-mono/aomi/bin/backend/migrations/20250112000000_update_contracts_schema.sql`
2. Migration should:
   - Add new columns with defaults for existing data
   - Create new indexes
   - NOT drop existing data
3. Test migration on empty database
4. Test migration on database with existing contracts

**Migration SQL**:
```sql
-- Add new columns
ALTER TABLE contracts
    ADD COLUMN IF NOT EXISTS symbol TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS is_proxy BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS implementation_address TEXT,
    ADD COLUMN IF NOT EXISTS tvl DECIMAL,
    ADD COLUMN IF NOT EXISTS transaction_count BIGINT,
    ADD COLUMN IF NOT EXISTS last_activity_at BIGINT,
    ADD COLUMN IF NOT EXISTS data_source TEXT NOT NULL DEFAULT 'manual',
    ADD COLUMN IF NOT EXISTS created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    ADD COLUMN IF NOT EXISTS updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;

-- Update name column to NOT NULL with default
ALTER TABLE contracts
    ALTER COLUMN name SET NOT NULL,
    ALTER COLUMN name SET DEFAULT 'Unknown';

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_contracts_tvl ON contracts(tvl DESC) WHERE tvl IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_tx_count ON contracts(transaction_count DESC) WHERE transaction_count IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_updated ON contracts(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_contracts_symbol ON contracts(symbol) WHERE symbol IS NOT NULL;
```

**Acceptance Criteria**:
- ✅ Migration runs successfully on empty DB
- ✅ Migration runs successfully on DB with existing data
- ✅ No data loss
- ✅ Indexes created
- ✅ Backend still starts and runs migrations

---

### Phase 3: API Clients
**Goal**: Implement HTTP clients for each data source

**File Structure**:
```
bin/contract-scraper/
├── src/
│   ├── main.rs
│   ├── clients/
│   │   ├── mod.rs
│   │   ├── defillama.rs
│   │   ├── coingecko.rs
│   │   └── etherscan.rs
│   ├── models.rs
│   ├── db.rs
│   └── config.rs
```

**Tasks**:

#### 3.1: DeFi Llama Client (`clients/defillama.rs`)
```rust
pub struct DefiLlamaClient {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
pub struct Protocol {
    pub name: String,
    pub symbol: Option<String>,
    pub chains: Vec<String>,
    pub tvl: f64,
    // ... other fields
}

impl DefiLlamaClient {
    pub async fn get_protocols(&self) -> Result<Vec<Protocol>>;
    pub async fn get_protocol(&self, name: &str) -> Result<Protocol>;
}
```

#### 3.2: CoinGecko Client (`clients/coingecko.rs`)
```rust
pub struct CoinGeckoClient {
    client: reqwest::Client,
    api_key: Option<String>,
    rate_limiter: RateLimiter, // Custom rate limiter
}

#[derive(Debug, Deserialize)]
pub struct CoinData {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub platforms: HashMap<String, String>, // chain -> address
    // ... other fields
}

impl CoinGeckoClient {
    pub async fn get_coins_list(&self) -> Result<Vec<CoinData>>;
    pub async fn get_coin_by_id(&self, id: &str) -> Result<CoinData>;
}
```

#### 3.3: Etherscan Client (`clients/etherscan.rs`)
```rust
pub struct EtherscanClient {
    client: reqwest::Client,
    api_keys: HashMap<i32, String>, // chain_id -> api_key
    base_urls: HashMap<i32, String>, // chain_id -> base_url
}

#[derive(Debug, Deserialize)]
pub struct ContractSource {
    pub source_code: String,
    pub abi: String,
    pub contract_name: String,
    pub compiler_version: String,
    pub optimization_used: bool,
    pub is_proxy: bool,
    pub implementation: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionList {
    pub result: Vec<Transaction>,
}

impl EtherscanClient {
    pub async fn get_contract_source(&self, chain_id: i32, address: &str) -> Result<ContractSource>;
    pub async fn get_transaction_count(&self, chain_id: i32, address: &str) -> Result<u64>;
    pub async fn detect_proxy(&self, chain_id: i32, address: &str) -> Result<(bool, Option<String>)>;
}
```

**Rate Limiting**:
```rust
// Simple token bucket rate limiter
pub struct RateLimiter {
    tokens: Arc<Mutex<f64>>,
    rate: f64, // tokens per second
    capacity: f64,
    last_update: Arc<Mutex<Instant>>,
}

impl RateLimiter {
    pub async fn acquire(&self) -> Result<()>;
}
```

**Acceptance Criteria**:
- ✅ Each client compiles
- ✅ Unit tests for each client (with mocked responses)
- ✅ Rate limiting works correctly
- ✅ Error handling for network failures
- ✅ Retry logic with exponential backoff

---

### Phase 4: Data Models
**Goal**: Define shared data structures

**File**: `src/models.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub address: String,
    pub chain: String,
    pub chain_id: i32,
    pub name: String,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub is_proxy: bool,
    pub implementation_address: Option<String>,
    pub source_code: String,
    pub abi: String,
    pub tvl: Option<f64>,
    pub transaction_count: Option<i64>,
    pub last_activity_at: Option<i64>,
    pub data_source: DataSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSource {
    DefiLlama,
    CoinGecko,
    Etherscan,
    Manual,
}

impl ToString for DataSource {
    fn to_string(&self) -> String {
        match self {
            DataSource::DefiLlama => "defillama".to_string(),
            DataSource::CoinGecko => "coingecko".to_string(),
            DataSource::Etherscan => "etherscan".to_string(),
            DataSource::Manual => "manual".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnrichedContract {
    pub metadata: ContractMetadata,
    pub code: ContractCode,
    pub metrics: ContractMetrics,
}

#[derive(Debug, Clone)]
pub struct ContractMetadata {
    pub address: String,
    pub chain_id: i32,
    pub name: String,
    pub symbol: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContractCode {
    pub source_code: String,
    pub abi: String,
    pub is_proxy: bool,
    pub implementation_address: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContractMetrics {
    pub tvl: Option<f64>,
    pub transaction_count: Option<i64>,
    pub last_activity_at: Option<i64>,
}
```

**Acceptance Criteria**:
- ✅ Models compile
- ✅ Serialization/deserialization works
- ✅ Conversion methods between models

---

### Phase 5: Database Operations
**Goal**: Implement database insert/update operations

**File**: `src/db.rs`

```rust
use sqlx::{Pool, Postgres};
use crate::models::Contract;

pub struct ContractStore {
    pool: Pool<Postgres>,
}

impl ContractStore {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Insert or update a contract (UPSERT)
    pub async fn upsert_contract(&self, contract: &Contract) -> Result<()> {
        sqlx::query!(
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
            contract.address,
            contract.chain,
            contract.chain_id,
            contract.name,
            contract.symbol,
            contract.description,
            contract.is_proxy,
            contract.implementation_address,
            contract.source_code,
            contract.abi,
            contract.tvl,
            contract.transaction_count,
            contract.last_activity_at,
            contract.data_source.to_string(),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Batch upsert contracts
    pub async fn upsert_contracts_batch(&self, contracts: &[Contract]) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        for contract in contracts {
            // Same query as above, but use transaction
        }

        tx.commit().await?;
        Ok(())
    }

    /// Get existing contract for comparison
    pub async fn get_contract(&self, chain_id: i32, address: &str) -> Result<Option<Contract>> {
        // SELECT query
    }

    /// Get contracts needing update (older than X days)
    pub async fn get_stale_contracts(&self, days: i64) -> Result<Vec<Contract>> {
        // SELECT WHERE updated_at < NOW() - days
    }
}
```

**Acceptance Criteria**:
- ✅ Upsert works correctly (insert new, update existing)
- ✅ Batch operations work
- ✅ Queries compile and run
- ✅ Unit tests with test database

---

### Phase 6: Scraping Orchestration
**Goal**: Coordinate data collection from all sources

**File**: `src/scraper.rs`

```rust
pub struct ContractScraper {
    defillama: DefiLlamaClient,
    coingecko: CoinGeckoClient,
    etherscan: EtherscanClient,
    db: ContractStore,
}

impl ContractScraper {
    pub async fn scrape_top_contracts(
        &self,
        limit: usize,
        chains: &[String],
    ) -> Result<Vec<Contract>> {
        let mut contracts = Vec::new();

        // 1. Get protocols from DeFi Llama
        tracing::info!("Fetching protocols from DeFi Llama...");
        let protocols = self.defillama.get_protocols().await?;

        // 2. Filter by TVL and chains
        let filtered = self.filter_protocols(protocols, limit, chains);

        // 3. For each protocol, get contract addresses
        for protocol in filtered {
            // Extract contract addresses per chain
            for (chain, addresses) in protocol.contracts {
                for address in addresses {
                    // 4. Enrich with CoinGecko data (optional)
                    let metadata = self.get_metadata(&address, &chain).await?;

                    // 5. Get source code and ABI from Etherscan
                    let code = self.etherscan.get_contract_source(
                        chain_to_chain_id(&chain)?,
                        &address
                    ).await?;

                    // 6. Get transaction metrics
                    let tx_count = self.etherscan.get_transaction_count(
                        chain_to_chain_id(&chain)?,
                        &address
                    ).await?;

                    // 7. Detect proxy
                    let (is_proxy, impl_addr) = self.etherscan.detect_proxy(
                        chain_to_chain_id(&chain)?,
                        &address
                    ).await?;

                    // 8. Build Contract struct
                    let contract = Contract {
                        address,
                        chain: chain.clone(),
                        chain_id: chain_to_chain_id(&chain)?,
                        name: protocol.name.clone(),
                        symbol: protocol.symbol.clone(),
                        description: Some(protocol.description.clone()),
                        is_proxy,
                        implementation_address: impl_addr,
                        source_code: code.source_code,
                        abi: code.abi,
                        tvl: Some(protocol.tvl),
                        transaction_count: Some(tx_count as i64),
                        last_activity_at: Some(chrono::Utc::now().timestamp()),
                        data_source: DataSource::DefiLlama,
                    };

                    contracts.push(contract);
                }
            }
        }

        Ok(contracts)
    }

    pub async fn update_existing_contracts(&self) -> Result<()> {
        // Get stale contracts from DB
        let stale = self.db.get_stale_contracts(7).await?; // 7 days old

        // Update each
        for contract in stale {
            // Refresh data...
        }

        Ok(())
    }
}
```

**Acceptance Criteria**:
- ✅ Can scrape contracts end-to-end
- ✅ Progress logging works
- ✅ Error handling for individual failures (continue on error)
- ✅ Rate limiting respected
- ✅ Integration test with real APIs (feature-gated)

---

### Phase 7: CLI Commands Implementation
**Goal**: Wire up CLI commands to scraper

**File**: `src/main.rs`

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Load config (API keys, database URL)
    let config = Config::from_env()?;

    // Connect to database
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    // Initialize clients
    let defillama = DefiLlamaClient::new();
    let coingecko = CoinGeckoClient::new(config.coingecko_api_key);
    let etherscan = EtherscanClient::new(config.etherscan_api_keys);
    let db = ContractStore::new(pool);

    let scraper = ContractScraper::new(defillama, coingecko, etherscan, db);

    match cli.command {
        Commands::Scrape { limit, chains } => {
            tracing::info!("Starting scrape with limit={} chains={:?}", limit, chains);
            let contracts = scraper.scrape_top_contracts(limit, &chains).await?;
            tracing::info!("Scraped {} contracts", contracts.len());

            // Save to database
            scraper.db.upsert_contracts_batch(&contracts).await?;
            tracing::info!("Saved to database");
        }

        Commands::Update { chain_id } => {
            tracing::info!("Updating existing contracts");
            scraper.update_existing_contracts().await?;
        }

        Commands::Verify { address, chain_id } => {
            tracing::info!("Verifying contract {} on chain {}", address, chain_id);
            // Fetch and display contract data
        }
    }

    Ok(())
}
```

**Acceptance Criteria**:
- ✅ All commands work end-to-end
- ✅ Proper error messages
- ✅ Progress indicators
- ✅ Graceful shutdown on Ctrl+C

---

### Phase 8: Configuration
**Goal**: Manage API keys and settings

**File**: `src/config.rs`

```rust
use std::collections::HashMap;

pub struct Config {
    pub database_url: String,
    pub coingecko_api_key: Option<String>,
    pub etherscan_api_keys: HashMap<i32, String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")?,
            coingecko_api_key: std::env::var("COINGECKO_API_KEY").ok(),
            etherscan_api_keys: Self::parse_etherscan_keys()?,
        })
    }

    fn parse_etherscan_keys() -> Result<HashMap<i32, String>> {
        let mut keys = HashMap::new();

        // Mainnet
        if let Ok(key) = std::env::var("ETHERSCAN_API_KEY") {
            keys.insert(1, key);
        }

        // Polygon
        if let Ok(key) = std::env::var("POLYGONSCAN_API_KEY") {
            keys.insert(137, key);
        }

        // Arbitrum
        if let Ok(key) = std::env::var("ARBISCAN_API_KEY") {
            keys.insert(42161, key);
        }

        // Base
        if let Ok(key) = std::env::var("BASESCAN_API_KEY") {
            keys.insert(8453, key);
        }

        // Optimism
        if let Ok(key) = std::env::var("OPTIMISM_API_KEY") {
            keys.insert(10, key);
        }

        Ok(keys)
    }
}
```

**Environment Variables**:
```bash
DATABASE_URL=postgresql://user:pass@localhost:5432/chatbot
COINGECKO_API_KEY=<optional>
ETHERSCAN_API_KEY=<required>
POLYGONSCAN_API_KEY=<optional>
ARBISCAN_API_KEY=<optional>
BASESCAN_API_KEY=<optional>
OPTIMISM_API_KEY=<optional>
```

**Acceptance Criteria**:
- ✅ Config loads from environment
- ✅ Sensible defaults
- ✅ Clear error messages for missing required keys

---

### Phase 9: Testing
**Goal**: Comprehensive test coverage

#### Unit Tests
- ✅ Each client has unit tests with mocked HTTP responses
- ✅ Models serialization/deserialization
- ✅ Database operations with test DB
- ✅ Rate limiter behavior

#### Integration Tests
```rust
#[tokio::test]
#[ignore] // Run with --ignored flag
async fn test_scrape_real_api() {
    // Test against real APIs (rate limited)
}

#[tokio::test]
async fn test_end_to_end() {
    // Full workflow with test database
}
```

**Test Database Setup**:
```bash
# Create test database
createdb chatbot_test

# Run migrations
DATABASE_URL=postgresql://localhost/chatbot_test cargo run -p backend -- migrate
```

**Acceptance Criteria**:
- ✅ All unit tests pass
- ✅ Integration tests pass (with --ignored flag)
- ✅ Test coverage > 70%
- ✅ No warnings or clippy errors

---

### Phase 10: Documentation
**Goal**: Document usage and maintenance

**Files**:
- `bin/contract-scraper/README.md` - Usage guide
- `bin/contract-scraper/ARCHITECTURE.md` - Technical design

**README.md Contents**:
```markdown
# Contract Scraper

CLI tool to scrape and maintain top DeFi contracts in the database.

## Setup

1. Set environment variables
2. Run: `cargo run -p contract-scraper -- scrape --limit 100`

## Commands

- `scrape` - Scrape new contracts
- `update` - Update existing contracts
- `verify` - Verify a specific contract

## Examples
...
```

**Acceptance Criteria**:
- ✅ README is clear and complete
- ✅ Architecture documented
- ✅ API references included

---

## Development Workflow

### Step 1: Create the crate
```bash
cd /Users/kevin/foameo/product-mono/aomi/bin
cargo new contract-scraper --bin
```

### Step 2: Update workspace Cargo.toml
Add to `/Users/kevin/foameo/product-mono/aomi/Cargo.toml`:
```toml
members = [
    # ... existing members
    "bin/contract-scraper",
]
```

### Step 3: Implement phases in order
- Follow phases 1-10 sequentially
- Test each phase before moving to the next
- Commit after each working phase

### Step 4: Run the scraper
```bash
# Dry run (no database writes)
cargo run -p contract-scraper -- scrape --limit 10 --dry-run

# Real scrape
DATABASE_URL=postgresql://localhost:5432/chatbot \
ETHERSCAN_API_KEY=your_key \
cargo run -p contract-scraper -- scrape --limit 100 --chains ethereum,polygon
```

---

## Testing Strategy

### Before Each Phase
1. ✅ Define acceptance criteria
2. ✅ Write tests first (TDD)
3. ✅ Implement feature
4. ✅ All tests pass
5. ✅ Run clippy
6. ✅ Commit

### Final Verification
```bash
# Run all tests
cargo test -p contract-scraper

# Run clippy
cargo clippy -p contract-scraper

# Build release
cargo build -p contract-scraper --release

# Test actual scrape (small)
cargo run -p contract-scraper -- scrape --limit 5
```

---

## Success Criteria

### Functional
- ✅ Can scrape top 100 contracts from DeFi Llama
- ✅ Enriches with metadata from CoinGecko
- ✅ Fetches source code and ABI from Etherscan
- ✅ Stores correctly in database
- ✅ Updates existing contracts without duplicates
- ✅ Handles failures gracefully

### Quality
- ✅ All tests pass
- ✅ No clippy warnings
- ✅ Test coverage > 70%
- ✅ Documentation complete
- ✅ Error messages are helpful

### Performance
- ✅ Rate limiting prevents API bans
- ✅ Batch operations for database
- ✅ Can scrape 100 contracts in < 10 minutes

---

## Future Enhancements (Out of Scope)

- [ ] Web UI for contract management
- [ ] Scheduled automatic updates (cron job)
- [ ] Support for more data sources (The Graph, etc.)
- [ ] Contract risk scoring
- [ ] Duplicate detection across chains
- [ ] Export to CSV/JSON

---

## Notes

- Start with Ethereum mainnet only, then add other chains
- Use `tracing` for structured logging
- Implement retry logic with exponential backoff
- Cache API responses to avoid duplicate calls
- Consider using a queue system for large scrapes (future)

---

## Questions to Resolve

1. Should we prioritize contracts by TVL or transaction count?
   - **Suggestion**: TVL for initial scrape, tx count for updates

2. How often should we update existing contracts?
   - **Suggestion**: Weekly for metrics, monthly for source code

3. What to do with proxy contracts - store both or just implementation?
   - **Suggestion**: Store both with relationship

4. Should we validate ABI format?
   - **Suggestion**: Yes, parse and validate before storing
