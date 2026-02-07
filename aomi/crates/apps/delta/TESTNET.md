# Delta Testnet Setup

## Credentials (from Delta team)

**Cargo Registry:**
- Registry: `repyh` (configured in `.cargo/config.toml`)
- Token: Add to `~/.cargo/credentials.toml`:
  ```toml
  [registries.repyh]
  token = "mp5iIBSPFSUcyEeL0qpmK1bqGXdDVSZH"
  ```

**Kellnr (crate browser):**
- URL: https://crates.repyhlabs.dev
- Username: `aomi`
- Password: `vwgVAr4GpXVH8vg`

## Testnet Config

Create `.env.testnet` with:
```bash
DELTA_RFQ_API_URL=http://164.92.69.96:9000
DELTA_SHARD_ID=9
DELTA_KEYPAIR_PATH=./keypair_9.json
```

## Pre-seeded Keypair

Request `keypair_9.json` from Delta team — this is the pre-funded test account.

## Running

```bash
# Load testnet env
source .env.testnet

# Run with testnet
cargo run --bin cli -- --backend delta
```

## Docs

- Tutorial: https://docs.repyhlabs.dev/docs/build/tutorial
- Mocks: https://docs.delta.network/docs/build/mocks
