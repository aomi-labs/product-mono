Here's each table with example test data:
---
contracts
| chain_id | address | chain | source_code | abi | name | symbol | description | is_proxy | implementation_address | last_activity_at | created_at | updated_at | protocol | contract_type | version |
|----------|---------|-------|-------------|-----|------|--------|-------------|----------|------------------------|------------------|------------|------------|----------|---------------|---------|
| 1 | 0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984 | ethereum | pragma solidity ^0.8.0; contract Token {...} | {"type":"function","name":"transfer"...} | Uniswap | UNI | Uniswap governance token | false | NULL | 1706745600 | 1704067200 | 1706745600 | uniswap | token | 1.0.0 |
| 137 | 0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619 | polygon | pragma solidity ^0.8.0; contract WETH {...} | {"type":"function","name":"deposit"...} | Wrapped Ether | WETH | Wrapped ETH on Polygon | true | 0xABCD1234567890abcdef1234567890abcdef1234 | 1706832000 | 1704153600 | 1706832000 | aave | wrapper | 2.0.0 |
---
transaction_records
| chain_id | address | nonce | last_fetched_at | last_block_number | total_transactions |
|----------|---------|-------|-----------------|-------------------|-------------------|
| 1 | 0x742d35Cc6634C0532925a3b844Bc9e7595f8B321 | 156 | 1706918400 | 19234567 | 342 |
| 137 | 0x8626f6940E2eb28930eFb4CeF49B2d1F2C9C1199 | 89 | 1706914800 | 52876543 | 127 |
---
transactions
| id | chain_id | address | hash | block_number | timestamp | from_address | to_address | value | gas | gas_price | gas_used | is_error | input | contract_address |
|----|----------|---------|------|--------------|-----------|--------------|------------|-------|-----|-----------|----------|----------|-------|------------------|
| 1 | 1 | 0x742d35Cc6634C0532925a3b844Bc9e7595f8B321 | 0xabc123def456789... | 19234567 | 1706918400 | 0x742d35Cc6634C0532925a3b844Bc9e7595f8B321 | 0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984 | 1000000000000000000 | 21000 | 30000000000 | 21000 | 0 | 0xa9059cbb000000... | NULL |
| 2 | 137 | 0x8626f6940E2eb28930eFb4CeF49B2d1F2C9C1199 | 0xdef789abc123456... | 52876543 | 1706914800 | 0x8626f6940E2eb28930eFb4CeF49B2d1F2C9C1199 | 0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619 | 500000000000000000 | 65000 | 50000000000 | 54321 | 0 | 0xd0e30db0... | NULL |
---
users
| public_key | username | created_at | namespaces |
|------------|----------|------------|------------|
| 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty | alice_wallet | 1704067200 | {default,polymarket} |
| 7HqR82PWL6jKsZgxmZF3GNLxmP8FhqXYuCLmY9Y1VHJd | bob_crypto | 1705276800 | {default,polymarket,defi-agent} |
---
sessions
| id | public_key | started_at | last_active_at | title | pending_transaction | messages_persisted |
|----|------------|------------|----------------|-------|---------------------|-------------------|
| sess_a1b2c3d4e5f6 | 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty | 1706832000 | 1706918400 | Swap ETH to USDC | {"to":"0x1234...","value":"1000000000000000000","data":"0x..."} | false |
| sess_x7y8z9w0v1u2 | 7HqR82PWL6jKsZgxmZF3GNLxmP8FhqXYuCLmY9Y1VHJd | 1706745600 | 1706835000 | Check portfolio | NULL | true |
---
messages
| id | session_id | message_type | sender | content | timestamp |
|----|------------|--------------|--------|---------|-----------|
| 1 | sess_a1b2c3d4e5f6 | chat | user | {"text":"I want to swap 1 ETH to USDC"} | 1706832000 |
| 2 | sess_a1b2c3d4e5f6 | chat | assistant | {"text":"I'll help you swap 1 ETH to USDC. Here's the transaction..."} | 1706832005 |
| 3 | sess_a1b2c3d4e5f6 | agent | system | {"action":"prepare_swap","params":{"from":"ETH","to":"USDC","amount":"1"}} | 1706832006 |
---
api_keys
| id | api_key | label | namespace | is_active | created_at |
|----|---------|-------|-----------|-----------|------------|
| 1 | ak_live_abc123xyz789 | Production App | defi-agent | true | 1704067200 |
| 2 | ak_live_abc123xyz789 | Production App | portfolio-tracker | true | 1704067200 |
| 3 | ak_test_def456uvw012 | Development | test-namespace | true | 1705276800 |
| 4 | ak_live_old999888777 | Deprecated App | legacy-bot | false | 1700000000 |
---
wallet binding (via sessions.public_key)
| session_id | public_key |
|------------|------------|
| telegram:dm:123456789 | 0x742d35Cc6634C0532925a3b844Bc9e7595f8B321 |
| telegram:dm:555555555 | 0x8626f6940E2eb28930eFb4CeF49B2d1F2C9C1199 |
---
Notes on Data Types
| Type | Example Value | Description |
|------|---------------|-------------|
| BIGINT (timestamp) | 1706918400 | Unix epoch seconds (Jan 2024) |
| TEXT (address) | 0x742d35Cc... | Ethereum hex address (42 chars) |
| TEXT (hash) | 0xabc123def... | Transaction hash (66 chars) |
| TEXT (value) | 1000000000000000000 | Wei amount as string (1 ETH = 10^18) |
| JSONB | {"text":"hello"} | JSON object/array |
| BOOLEAN | true / false | Boolean flag |
| BIGSERIAL | 1, 2, 3... | Auto-incrementing integer |
| TEXT[] | {default,polymarket} | PostgreSQL text array |
| VARCHAR(n) | tg_123456789 | Variable-length string with max length |
| TIMESTAMP WITH TIME ZONE | 2024-02-01 12:00:00+00 | Timestamp with timezone (ISO 8601) |
