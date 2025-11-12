# Contract Scraper Architecture

## Overview

The Contract Scraper is a CLI application built in Rust that aggregates DeFi contract data from multiple sources and maintains it in a PostgreSQL database. It's designed to be run periodically to keep contract information up-to-date.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI Layer                            │
│                        (main.rs)                             │
│   Commands: scrape, update, verify                           │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│                    Orchestration Layer                       │
│                      (scraper.rs)                            │
│   • scrape_top_contracts()                                   │
│   • update_existing_contracts()                              │
│   • verify_contract()                                        │
└──┬─────────────────┬──────────────────┬────────────────────┘
   │                 │                  │
   ▼                 ▼                  ▼
┌────────────┐  ┌──────────────┐  ┌──────────────┐
│  DeFi      │  │   CoinGecko  │  │  Etherscan   │
│  Llama     │  │   Client     │  │  Client      │
│  Client    │  │              │  │              │
└──────┬─────┘  └──────┬───────┘  └──────┬───────┘
       │                │                  │
       │ (Protocols)    │ (Metadata)      │ (Source Code)
       │                │                  │
       └────────────────┴──────────────────┘
                         │
                         ▼
                  ┌────────────┐
                  │   Models   │
                  │            │
                  │ • Contract │
                  └──────┬─────┘
                         │
                         ▼
                  ┌────────────┐
                  │  Database  │
                  │  Layer     │
                  │  (db.rs)   │
                  └──────┬─────┘
                         │
                         ▼
                  ┌────────────┐
                  │ PostgreSQL │
                  │  Database  │
                  └────────────┘
```

## Component Details

### 1. CLI Layer (`main.rs`)

**Responsibility**: Parse command-line arguments and route to appropriate handlers.

**Key Components**:
- `Cli` struct: Defines the CLI structure using clap
- `Commands` enum: Defines available subcommands (scrape, update, verify)
- `main()`: Entry point that:
  1. Loads configuration
  2. Connects to database
  3. Initializes clients
  4. Routes to command handlers

**Dependencies**:
- clap: CLI argument parsing
- tokio: Async runtime
- tracing: Logging

### 2. Orchestration Layer (`scraper.rs`)

**Responsibility**: Coordinate data collection from multiple sources and manage the scraping workflow.

**Key Types**:
```rust
pub struct ContractScraper {
    defillama: DefiLlamaClient,
    coingecko: CoinGeckoClient,
    etherscan: EtherscanClient,
    db: ContractStore,
}
```

**Key Methods**:
- `scrape_top_contracts()`: Main scraping workflow
  1. Fetch protocols from DeFi Llama
  2. Filter and sort by TVL
  3. For each protocol:
     - Get contract addresses
     - Fetch source code from Etherscan
     - Collect metrics
     - Build Contract model
  4. Return collected contracts

- `update_existing_contracts()`: Refresh stale contracts
  - Query database for old contracts
  - Re-fetch data from Etherscan
  - Update database records

- `verify_contract()`: Inspect a specific contract
  - Check database
  - Verify against Etherscan
  - Display details

### 3. API Clients (`clients/`)

#### 3.1 DeFi Llama Client (`clients/defillama.rs`)

**Purpose**: Fetch DeFi protocol data including TVL and contract addresses.

**Key Methods**:
- `get_protocols()`: Get all protocols with TVL data
- `get_protocol(name)`: Get detailed protocol info including contract addresses
- `filter_by_tvl_and_chains()`: Filter protocols by criteria
- `sort_by_tvl()`: Sort protocols by TVL descending

**Rate Limiting**: Automatic backoff on errors

**API Endpoints**:
- `https://api.llama.fi/protocols` - List all protocols
- `https://api.llama.fi/protocol/{name}` - Protocol details

#### 3.2 CoinGecko Client (`clients/coingecko.rs`)

**Purpose**: Enrich contract data with additional metadata (currently unused in scraper but available for future use).

**Key Methods**:
- `get_coins_list()`: Get all coins with platform addresses
- `get_coin_by_id(id)`: Get detailed coin information
- `normalize_chain_name()`: Convert CoinGecko chain names to internal format

**Rate Limiting**: Token bucket algorithm
- Free tier: ~12 requests/minute
- Configurable with RateLimiter struct

**API Endpoints**:
- `https://api.coingecko.com/api/v3/coins/list?include_platform=true`
- `https://api.coingecko.com/api/v3/coins/{id}`

#### 3.3 Etherscan Client (`clients/etherscan.rs`)

**Purpose**: Fetch verified contract source code, ABI, and transaction data from blockchain explorers.

**Key Methods**:
- `get_contract_source()`: Get source code and ABI
- `get_transaction_count()`: Get number of transactions
- `detect_proxy()`: Detect if contract is a proxy
- `get_last_activity()`: Get timestamp of last transaction

**Rate Limiting**: 200ms delay between requests (5 req/sec for free tier)

**Supported Chains**:
- Ethereum (chain_id: 1) - api.etherscan.io
- Polygon (chain_id: 137) - api.polygonscan.com
- Arbitrum (chain_id: 42161) - api.arbiscan.io
- Base (chain_id: 8453) - api.basescan.org
- Optimism (chain_id: 10) - api-optimistic.etherscan.io

**Helper Functions**:
- `chain_to_chain_id()`: Convert chain name to numeric ID
- `chain_id_to_chain_name()`: Convert chain ID to name

### 4. Data Models (`models.rs`)

**Purpose**: Define data structures for contracts and related entities.

**Key Types**:

```rust
pub struct Contract {
    // Identity
    pub address: String,
    pub chain: String,
    pub chain_id: i32,

    // Metadata
    pub name: String,
    pub symbol: Option<String>,
    pub description: Option<String>,

    // Proxy info
    pub is_proxy: bool,
    pub implementation_address: Option<String>,

    // Code
    pub source_code: String,
    pub abi: String,

    // Metrics
    pub tvl: Option<f64>,
    pub transaction_count: Option<i64>,
    pub last_activity_at: Option<i64>,

    // Tracking
    pub data_source: DataSource,
}

pub enum DataSource {
    DefiLlama,
    CoinGecko,
    Etherscan,
    Manual,
}
```

**Builder Pattern**: Contract type supports fluent builder methods:
```rust
Contract::new(...)
    .with_symbol("UNI")
    .with_tvl(1000000.0)
    .with_transaction_count(500)
```

### 5. Database Layer (`db.rs`)

**Purpose**: Handle all database operations with PostgreSQL.

**Key Type**:
```rust
pub struct ContractStore {
    pool: Pool<Postgres>,
}
```

**Key Methods**:
- `upsert_contract()`: Insert or update a single contract
- `upsert_contracts_batch()`: Bulk insert/update with transaction
- `get_contract()`: Fetch contract by chain_id and address
- `get_stale_contracts()`: Get contracts older than N days
- `get_contracts_by_chain()`: Get all contracts on a chain
- `get_contract_count()`: Get total count
- `get_top_by_tvl()`: Get top N contracts by TVL

**Implementation Notes**:
- Uses runtime queries (not compile-time checked) to avoid DATABASE_URL requirement at build time
- All upserts use `ON CONFLICT` clause for idempotency
- Batch operations use transactions for consistency
- Timestamps stored as BIGINT (Unix epoch)

### 6. Configuration (`config.rs`)

**Purpose**: Load and validate configuration from environment variables.

**Key Type**:
```rust
pub struct Config {
    pub database_url: String,
    pub coingecko_api_key: Option<String>,
    pub etherscan_api_key: Option<String>,
}
```

**Environment Variables**:
- `DATABASE_URL` (required)
- `ETHERSCAN_API_KEY` (required) - Single key for all Etherscan v2 compatible explorers
- `COINGECKO_API_KEY` (optional)

**Key Methods**:
- `from_env()`: Load configuration from environment
- `validate()`: Check configuration is valid
- `has_etherscan_key()`: Check if Etherscan API key is configured

## Data Flow

### Scrape Flow

```
1. User runs: contract-scraper scrape -l 100 -c ethereum

2. Main loads config and initializes clients

3. Scraper.scrape_top_contracts():
   │
   ├─> DeFi Llama: Get top 100 protocols by TVL
   │   └─> Returns: [{name, tvl, chains, ...}]
   │
   ├─> For each protocol:
   │   │
   │   ├─> DeFi Llama: Get protocol details
   │   │   └─> Returns: {contracts: {ethereum: [0x123...], ...}}
   │   │
   │   └─> For each contract address:
   │       │
   │       ├─> Etherscan: Get source code
   │       │   └─> Returns: {source_code, abi, ...}
   │       │
   │       ├─> Etherscan: Get transaction count
   │       │   └─> Returns: 12345
   │       │
   │       ├─> Etherscan: Detect proxy
   │       │   └─> Returns: (is_proxy, implementation)
   │       │
   │       └─> Build Contract model
   │
   └─> Save all contracts to database
       └─> DB: Batch upsert with transaction
```

### Update Flow

```
1. User runs: contract-scraper update

2. Scraper.update_existing_contracts():
   │
   ├─> DB: Get stale contracts (updated_at < now - 7 days)
   │   └─> Returns: [Contract, ...]
   │
   └─> For each contract:
       │
       ├─> Etherscan: Get fresh source code
       │   └─> Returns: {source_code, abi}
       │
       ├─> Etherscan: Get transaction count
       │   └─> Returns: 15678
       │
       ├─> Etherscan: Get last activity
       │   └─> Returns: 1673891234
       │
       └─> DB: Upsert updated contract
```

## Database Schema

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

    -- Metrics
    tvl DECIMAL,
    transaction_count BIGINT,
    last_activity_at BIGINT,

    -- Source tracking
    data_source TEXT NOT NULL,

    -- Timestamps
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,

    PRIMARY KEY (chain_id, address)
);

-- Indexes for performance
CREATE INDEX idx_contracts_tvl ON contracts(tvl DESC) WHERE tvl IS NOT NULL;
CREATE INDEX idx_contracts_tx_count ON contracts(transaction_count DESC);
CREATE INDEX idx_contracts_updated ON contracts(updated_at DESC);
CREATE INDEX idx_contracts_symbol ON contracts(symbol) WHERE symbol IS NOT NULL;
```

## Error Handling

### Strategy

1. **Graceful Degradation**: Individual contract failures don't stop the entire scrape
2. **Retry Logic**: API clients automatically retry with exponential backoff
3. **Rate Limiting**: Respect API limits to avoid bans
4. **Logging**: Use tracing for structured logging at multiple levels

### Error Types

- **Configuration Errors**: Fail fast if DATABASE_URL or required API keys missing
- **Network Errors**: Retry with backoff, log warning and continue
- **API Errors**: Log error, skip contract, continue with next
- **Database Errors**: Fail the batch operation, transaction rollback

### Logging Levels

- `ERROR`: Critical failures (config, database connection)
- `WARN`: Recoverable failures (API errors, contract not found)
- `INFO`: Progress updates, success messages
- `DEBUG`: Detailed operation info (API calls, queries)

## Performance Considerations

### Rate Limiting

- **DeFi Llama**: No explicit limits, uses automatic backoff on 429/5xx
- **CoinGecko**: Token bucket limiter (12 req/min free tier)
- **Etherscan**: Fixed 200ms delay between calls (5 req/sec)

### Optimization Opportunities

1. **Parallel Fetching**: Currently sequential, could parallelize with:
   - Per-protocol concurrency
   - Per-chain concurrency
   - Respect rate limits with semaphore

2. **Caching**: Cache DeFi Llama responses (rarely change)

3. **Incremental Updates**: Only fetch changed data

4. **Database Batching**: Already implemented for inserts

### Scaling

Current implementation handles:
- ~100 contracts in ~10 minutes (with rate limits)
- Limited by Etherscan rate limits (200ms per call)

For larger scale:
- Use paid API tiers (higher rate limits)
- Implement parallel processing
- Add queue system for background processing

## Testing

### Unit Tests

Each module has unit tests:
- `clients/`: Mock API responses, test parsing
- `models/`: Test serialization, builder pattern
- `config/`: Test environment parsing
- `db/`: Would need test database (not implemented)

### Integration Tests

Not implemented but recommended:
- End-to-end scrape with test data
- Database operations with test DB
- Real API calls (feature-gated)

### Running Tests

```bash
cargo test -p contract-scraper
```

Current coverage: 14 unit tests passing

## Security Considerations

1. **API Keys**: Loaded from environment, never logged
2. **SQL Injection**: Protected by sqlx parameterized queries
3. **Rate Limiting**: Prevents API bans
4. **Input Validation**: Chain IDs and addresses validated

## Future Improvements

### Short Term
- [ ] Parallel scraping with rate limit semaphore
- [ ] Better error recovery (resume failed scrapes)
- [ ] Progress bars for long operations
- [ ] Dry-run mode (don't save to DB)

### Medium Term
- [ ] Webhook notifications on completion
- [ ] Metrics export (Prometheus)
- [ ] Contract diffing (detect source changes)
- [ ] Scheduled runs with cron

### Long Term
- [ ] Web UI for management
- [ ] GraphQL API
- [ ] Real-time updates via websockets
- [ ] AI-powered contract analysis

## Dependencies

### Core
- `tokio`: Async runtime
- `sqlx`: Database operations (PostgreSQL)
- `anyhow`: Error handling
- `clap`: CLI argument parsing
- `tracing`: Structured logging

### API Clients
- `reqwest`: HTTP client
- `serde`: Serialization/deserialization

### Utilities
- `chrono`: Timestamp handling

All dependencies use workspace versions for consistency.
