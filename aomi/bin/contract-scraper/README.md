# Contract Scraper

CLI tool to scrape and maintain top DeFi contracts from multiple sources in the database.

## Features

- 🔄 Scrape top DeFi contracts from DeFi Llama
- 📊 Enrich with metadata (TVL, transaction counts, source code)
- 🔍 Verify contracts against blockchain explorers
- 💾 Store and update contracts in PostgreSQL database
- 🔄 Update existing contracts with fresh data
- 🔗 Support for multiple chains (Ethereum, Polygon, Arbitrum, Base, Optimism)

## Installation

From the project root:

```bash
cargo build --release -p contract-scraper
```

The binary will be available at `target/release/contract-scraper`.

## Configuration

### Required Environment Variables

```bash
# Database connection (required)
DATABASE_URL=postgresql://user:pass@localhost:5432/chatbot

# Etherscan API key (required for scraping)
# Works for all Etherscan v2 compatible explorers (Ethereum, Polygon, Arbitrum, Base, Optimism)
ETHERSCAN_API_KEY=your_api_key

# Optional: CoinGecko API key (for better rate limits)
COINGECKO_API_KEY=your_coingecko_key
```

### Getting API Keys

- **Etherscan**: https://etherscan.io/myapikey
  - A single API key works across all Etherscan v2 compatible explorers:
    - Ethereum Mainnet (etherscan.io)
    - Polygon (polygonscan.com)
    - Arbitrum (arbiscan.io)
    - Base (basescan.org)
    - Optimism (optimistic.etherscan.io)
- **CoinGecko**: https://www.coingecko.com/en/api/pricing (optional)

## Usage

### Scrape Top Contracts

Scrape the top N contracts from DeFi Llama:

```bash
# Scrape top 100 contracts (all chains)
cargo run -p contract-scraper -- scrape --limit 100

# Scrape top 50 contracts from specific chains
cargo run -p contract-scraper -- scrape --limit 50 --chains ethereum,polygon

# Short form
cargo run -p contract-scraper -- scrape -l 10 -c ethereum
```

### Update Existing Contracts

Update contracts already in the database with fresh data:

```bash
# Update all stale contracts (not updated in last 7 days)
cargo run -p contract-scraper -- update

# Update contracts on a specific chain
cargo run -p contract-scraper -- update --chain-id 1  # Ethereum
cargo run -p contract-scraper -- update -c 137       # Polygon
```

### Verify a Contract

Verify and inspect a specific contract:

```bash
# Verify contract on Ethereum
cargo run -p contract-scraper -- verify \
    --address 0x1234... \
    --chain-id 1

# Short form
cargo run -p contract-scraper -- verify -a 0x1234... -c 1
```

### Query the Database

After scraping, you can query the database to inspect the results. A collection of useful queries is provided in `queries.sql`:

```bash
# Connect to database
psql $DATABASE_URL

# Run a specific query (e.g., count total contracts)
psql $DATABASE_URL -c "SELECT COUNT(*) as total_contracts FROM contracts;"

# Run queries from the file
psql $DATABASE_URL -f bin/contract-scraper/queries.sql

# Or interactively
psql $DATABASE_URL
\i bin/contract-scraper/queries.sql
```

Useful quick queries:
```sql
-- Check total contracts
SELECT COUNT(*) FROM contracts;

-- Recent contracts
SELECT name, chain, address, tvl FROM contracts ORDER BY created_at DESC LIMIT 10;

-- Contracts by chain
SELECT chain, COUNT(*) FROM contracts GROUP BY chain;
```

## Command Reference

### `scrape`

Scrape contracts from DeFi Llama and store in database.

**Options:**
- `-l, --limit <LIMIT>`: Number of top protocols to scrape (default: 100)
- `-c, --chains <CHAINS>`: Comma-separated list of chains to scrape (default: all)

**Supported chains:** `ethereum`, `polygon`, `arbitrum`, `base`, `optimism`

### `update`

Update existing contracts in the database.

**Options:**
- `-c, --chain-id <CHAIN_ID>`: Only update contracts on this chain (optional)

### `verify`

Verify and inspect a specific contract.

**Options:**
- `-a, --address <ADDRESS>`: Contract address (required)
- `-c, --chain-id <CHAIN_ID>`: Chain ID (required)

**Chain IDs:**
- Ethereum: 1
- Polygon: 137
- Arbitrum: 42161
- Base: 8453
- Optimism: 10

## How It Works

1. **Fetch Protocols**: Gets top DeFi protocols from DeFi Llama API
2. **Filter & Sort**: Filters by TVL and selected chains
3. **Get Contract Addresses**: Extracts contract addresses per protocol
4. **Enrich Data**:
   - Fetches source code and ABI from Etherscan
   - Gets transaction counts
   - Detects proxy contracts
   - Gets last activity timestamps
5. **Store**: Upserts contracts into PostgreSQL database
6. **Report**: Displays a summary showing:
   - Total contracts scraped
   - Breakdown by chain
   - Metadata coverage (TVL, transaction counts)
   - Sample contracts with key details
   - Database confirmation with total count

## Database Schema

Contracts are stored with the following information:

- **Identity**: address, chain, chain_id
- **Metadata**: name, symbol, description
- **Code**: source_code, abi
- **Proxy Info**: is_proxy, implementation_address
- **Metrics**: tvl, transaction_count, last_activity_at
- **Tracking**: data_source, created_at, updated_at

See `migrations/20250112000000_update_contracts_schema.sql` for full schema.

## Rate Limiting

The scraper respects API rate limits:

- **DeFi Llama**: Automatic backoff on errors
- **CoinGecko**: ~12 requests/minute (free tier)
- **Etherscan**: 5 requests/second (free tier), 200ms delay between calls

## Example Workflows

### Initial Setup

```bash
# 1. Set environment variables
export DATABASE_URL="postgresql://localhost/chatbot"
export ETHERSCAN_API_KEY="your_key_here"

# 2. Scrape top 10 Ethereum contracts
cargo run -p contract-scraper -- scrape -l 10 -c ethereum

# 3. Verify the results
cargo run -p contract-scraper -- verify -a <address> -c 1
```

### Daily Maintenance

```bash
# Update all contracts not refreshed in last week
cargo run -p contract-scraper -- update
```

### Expand Coverage

```bash
# Add Polygon contracts
export POLYGONSCAN_API_KEY="your_key_here"
cargo run -p contract-scraper -- scrape -l 50 -c polygon
```

## Troubleshooting

### "DATABASE_URL not set"

Set the `DATABASE_URL` environment variable:
```bash
export DATABASE_URL="postgresql://user:pass@localhost:5432/chatbot"
```

### "No Etherscan API key configured"

You need to set the `ETHERSCAN_API_KEY` environment variable. See Configuration section.

### Rate limit errors

The scraper will automatically retry with backoff, but you may need to:
- Reduce the `--limit` parameter
- Add delays between runs
- Upgrade to a paid API tier

### "Contract not found"

The contract may not have verified source code on Etherscan, or the address may be incorrect.

## Development

### Running Tests

```bash
cargo test -p contract-scraper
```

### Building for Release

```bash
cargo build --release -p contract-scraper
```

### Project Structure

```
bin/contract-scraper/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── clients/          # API clients
│   │   ├── mod.rs
│   │   ├── defillama.rs  # DeFi Llama client
│   │   ├── coingecko.rs  # CoinGecko client
│   │   └── etherscan.rs  # Etherscan client
│   ├── config.rs         # Configuration management
│   ├── db.rs             # Database operations
│   ├── models.rs         # Data models
│   └── scraper.rs        # Scraping orchestration
├── Cargo.toml
└── README.md
```

## Future Enhancements

- [ ] Web UI for contract management
- [ ] Scheduled automatic updates (cron integration)
- [ ] Support for more data sources (The Graph, etc.)
- [ ] Contract risk scoring
- [ ] Duplicate detection across chains
- [ ] Export to CSV/JSON
- [ ] Parallel scraping for better performance
- [ ] Resume failed scrapes

## License

See the main project LICENSE file.
