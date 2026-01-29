# Database Schema

## Table: `contracts`

| Column | Type | Constraints | Default |
|--------|------|-------------|---------|
| address | TEXT | NOT NULL | - |
| chain | TEXT | NOT NULL | - |
| chain_id | INTEGER | NOT NULL | - |
| source_code | TEXT | NOT NULL | - |
| abi | TEXT | NOT NULL | - |
| name | TEXT | NOT NULL | 'Unknown' |
| symbol | TEXT | - | NULL |
| description | TEXT | - | NULL |
| is_proxy | BOOLEAN | NOT NULL | false |
| implementation_address | TEXT | - | NULL |
| last_activity_at | BIGINT | - | NULL |
| created_at | BIGINT | NOT NULL | EPOCH timestamp |
| updated_at | BIGINT | NOT NULL | EPOCH timestamp |
| protocol | TEXT | - | NULL |
| contract_type | TEXT | - | NULL |
| version | TEXT | - | NULL |

**Primary Key:** `(chain_id, address)`

### Example Data

| chain_id | address | chain | name | symbol | is_proxy |
|----------|---------|-------|------|--------|----------|
| 1 | 0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984 | ethereum | Uniswap | UNI | false |
| 137 | 0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619 | polygon | Wrapped Ether | WETH | true |

---

## Table: `transaction_records`

| Column | Type | Constraints | Default |
|--------|------|-------------|---------|
| chain_id | INTEGER | NOT NULL | - |
| address | TEXT | NOT NULL | - |
| nonce | BIGINT | - | NULL |
| last_fetched_at | BIGINT | - | NULL |
| last_block_number | BIGINT | - | NULL |
| total_transactions | INTEGER | - | NULL |

**Primary Key:** `(chain_id, address)`

### Example Data

| chain_id | address | nonce | last_fetched_at | last_block_number | total_transactions |
|----------|---------|-------|-----------------|-------------------|-------------------|
| 1 | 0x742d35Cc6634C0532925a3b844Bc9e7595f8B321 | 156 | 1706918400 | 19234567 | 342 |
| 137 | 0x8626f6940E2eb28930eFb4CeF49B2d1F2C9C1199 | 89 | 1706914800 | 52876543 | 127 |

---

## Table: `transactions`

| Column | Type | Constraints | Default |
|--------|------|-------------|---------|
| id | BIGSERIAL | PRIMARY KEY | auto |
| chain_id | INTEGER | NOT NULL | - |
| address | TEXT | NOT NULL | - |
| hash | TEXT | NOT NULL | - |
| block_number | BIGINT | NOT NULL | - |
| timestamp | BIGINT | NOT NULL | - |
| from_address | TEXT | NOT NULL | - |
| to_address | TEXT | NOT NULL | - |
| value | TEXT | NOT NULL | - |
| gas | TEXT | NOT NULL | - |
| gas_price | TEXT | NOT NULL | - |
| gas_used | TEXT | NOT NULL | - |
| is_error | TEXT | NOT NULL | - |
| input | TEXT | NOT NULL | - |
| contract_address | TEXT | - | NULL |

**Primary Key:** `id`  
**Foreign Key:** `(chain_id, address)` -> `transaction_records(chain_id, address)`  
**Unique:** `(chain_id, address, hash)`

### Example Data

| id | chain_id | address | hash | block_number | from_address | to_address | value |
|----|----------|---------|------|--------------|--------------|------------|-------|
| 1 | 1 | 0x742d35Cc... | 0xabc123... | 19234567 | 0x742d35Cc... | 0x1f9840a8... | 1000000000000000000 |
| 2 | 137 | 0x8626f694... | 0xdef789... | 52876543 | 0x8626f694... | 0x7ceB23fD... | 500000000000000000 |

---

## Table: `users`

| Column | Type | Constraints | Default |
|--------|------|-------------|---------|
| public_key | TEXT | PRIMARY KEY | - |
| username | TEXT | UNIQUE | NULL |
| created_at | BIGINT | NOT NULL | EPOCH timestamp |
| namespaces | TEXT[] | NOT NULL | ARRAY['default', 'polymarket'] |

**Primary Key:** `public_key`

### Example Data

| public_key | username | created_at | namespaces |
|------------|----------|------------|------------|
| 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty | alice_wallet | 1704067200 | {default,polymarket} |
| 7HqR82PWL6jKsZgxmZF3GNLxmP8FhqXYuCLmY9Y1VHJd | bob_crypto | 1705276800 | {default,polymarket,l2beat} |

---

## Table: `sessions`

| Column | Type | Constraints | Default |
|--------|------|-------------|---------|
| id | TEXT | PRIMARY KEY | - |
| public_key | TEXT | FK -> users | NULL |
| started_at | BIGINT | NOT NULL | EPOCH timestamp |
| last_active_at | BIGINT | NOT NULL | EPOCH timestamp |
| title | TEXT | - | NULL |
| pending_transaction | JSONB | - | NULL |
| messages_persisted | BOOLEAN | NOT NULL | FALSE |

**Primary Key:** `id`  
**Foreign Key:** `public_key` -> `users(public_key)` ON DELETE SET NULL

### Example Data

| id | public_key | started_at | last_active_at | title | pending_transaction | messages_persisted |
|----|------------|------------|----------------|-------|---------------------|-------------------|
| sess_a1b2c3d4e5f6 | 5FHneW46xGXgs5m... | 1706832000 | 1706918400 | Swap ETH to USDC | {"to":"0x...","value":"1000000000000000000"} | false |
| sess_x7y8z9w0v1u2 | 7HqR82PWL6jKsZg... | 1706745600 | 1706835000 | Check portfolio | NULL | true |

---

## Table: `messages`

| Column | Type | Constraints | Default |
|--------|------|-------------|---------|
| id | BIGSERIAL | PRIMARY KEY | auto |
| session_id | TEXT | NOT NULL, FK -> sessions | - |
| message_type | TEXT | NOT NULL | 'chat' |
| sender | TEXT | NOT NULL | - |
| content | JSONB | NOT NULL | - |
| timestamp | BIGINT | NOT NULL | EPOCH timestamp |

**Primary Key:** `id`  
**Foreign Key:** `session_id` -> `sessions(id)` ON DELETE CASCADE

### Example Data

| id | session_id | message_type | sender | content | timestamp |
|----|------------|--------------|--------|---------|-----------|
| 1 | sess_a1b2c3d4e5f6 | chat | user | {"text":"I want to swap 1 ETH to USDC"} | 1706832000 |
| 2 | sess_a1b2c3d4e5f6 | chat | assistant | {"text":"I'll help you swap 1 ETH to USDC..."} | 1706832005 |
| 3 | sess_a1b2c3d4e5f6 | agent | system | {"action":"prepare_swap","params":{...}} | 1706832006 |

---

## Table: `api_keys`

| Column | Type | Constraints | Default |
|--------|------|-------------|---------|
| id | BIGSERIAL | PRIMARY KEY | auto |
| api_key | TEXT | NOT NULL | - |
| label | TEXT | - | NULL |
| namespace | TEXT | NOT NULL | - |
| is_active | BOOLEAN | NOT NULL | TRUE |
| created_at | BIGINT | NOT NULL | EPOCH timestamp |

**Primary Key:** `id`  
**Unique:** `(api_key, namespace)`

### Example Data

| id | api_key | label | namespace | is_active | created_at |
|----|---------|-------|-----------|-----------|------------|
| 1 | ak_live_abc123xyz789 | Production App | defi-agent | true | 1704067200 |
| 2 | ak_live_abc123xyz789 | Production App | portfolio-tracker | true | 1704067200 |
| 3 | ak_test_def456uvw012 | Development | test-namespace | true | 1705276800 |
| 4 | ak_live_old999888777 | Deprecated App | legacy-bot | false | 1700000000 |

---

## Indexes

| Table | Index Name | Columns | Notes |
|-------|------------|---------|-------|
| contracts | idx_contracts_chain_id | chain_id | |
| contracts | idx_contracts_address | address | |
| contracts | idx_contracts_chain | chain | |
| contracts | idx_contracts_last_activity | last_activity_at DESC | partial, WHERE NOT NULL |
| contracts | idx_contracts_updated | updated_at DESC | |
| contracts | idx_contracts_symbol | symbol | partial, WHERE NOT NULL |
| contracts | idx_contracts_protocol | protocol | partial, WHERE NOT NULL |
| contracts | idx_contracts_type | contract_type | partial, WHERE NOT NULL |
| contracts | idx_contracts_version | version | partial, WHERE NOT NULL |
| transactions | idx_tx_chain_address_block | chain_id, address, block_number DESC | |
| transactions | idx_tx_hash | hash | |
| transactions | idx_tx_timestamp | chain_id, address, timestamp DESC | |
| users | idx_users_namespaces | namespaces | GIN index for array lookups |
| sessions | idx_sessions_public_key | public_key | |
| sessions | idx_sessions_last_active | last_active_at DESC | |
| messages | idx_messages_session_type | session_id, message_type, timestamp ASC | |
| api_keys | idx_api_keys_active | is_active | |
| api_keys | idx_api_keys_api_key | api_key | |
| api_keys | idx_api_keys_namespace | namespace | |

---

## Entity Relationships

```
users
  |
  | 1:N (public_key)
  v
sessions -----> messages
  |               (1:N via session_id)
  |
transaction_records
  |
  | 1:N (chain_id, address)
  v
transactions

contracts (standalone)
api_keys (standalone, one row per api_key + namespace pair)
```

---

## Data Types Reference

| Type | Example | Description |
|------|---------|-------------|
| BIGINT (timestamp) | 1706918400 | Unix epoch seconds |
| TEXT (address) | 0x742d35Cc6634C0532925a3b844Bc9e7595f8B321 | Ethereum hex address (42 chars) |
| TEXT (hash) | 0xabc123def456789... | Transaction hash (66 chars) |
| TEXT (value) | 1000000000000000000 | Wei amount as string (1 ETH = 10^18) |
| JSONB | {"text":"hello"} | JSON object/array |
| TEXT[] | {default,polymarket} | PostgreSQL array |
| BOOLEAN | true / false | Boolean flag |
| BIGSERIAL | 1, 2, 3... | Auto-incrementing integer |
