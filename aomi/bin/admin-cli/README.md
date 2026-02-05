# Admin CLI

Database administration tool for managing API keys, users, sessions, and contracts.

## Setup

```bash
export DATABASE_URL="postgres://aomi:aomi_dev_db_2024@127.0.0.1:5432/chatbot"
```

Or pass directly via `-d` flag:
```bash
cargo run --bin admin-cli -- -d "postgres://..." <command>
```

## Commands

### API Keys

```bash
# List all API keys
cargo run --bin admin-cli -- api-keys list
cargo run --bin admin-cli -- api-keys list --active-only

# Create API key
cargo run --bin admin-cli -- api-keys create -n <namespace> -l <label>
cargo run --bin admin-cli -- api-keys create -n delta -l delta-rfq
cargo run --bin admin-cli -- api-keys create -n ns1 -n ns2 -l multi-namespace-key  # multiple namespaces

# Update API key
cargo run --bin admin-cli -- api-keys update -k <api_key> --active      # activate
cargo run --bin admin-cli -- api-keys update -k <api_key> --inactive    # deactivate
cargo run --bin admin-cli -- api-keys update -k <api_key> -l new-label  # change label
cargo run --bin admin-cli -- api-keys update -k <api_key> --clear-label # remove label
```

### Users

```bash
# List users
cargo run --bin admin-cli -- users list
cargo run --bin admin-cli -- users list -l 10 -o 0  # with pagination

# Update user
cargo run --bin admin-cli -- users update -p <public_key> -u <username>
cargo run --bin admin-cli -- users update -p <public_key> -n ns1 -n ns2  # update namespaces
cargo run --bin admin-cli -- users update -p <public_key> --clear-username

# Delete user
cargo run --bin admin-cli -- users delete -p <public_key>
```

### Sessions

```bash
# List sessions
cargo run --bin admin-cli -- sessions list
cargo run --bin admin-cli -- sessions list -p <public_key>  # filter by user

# Update session
cargo run --bin admin-cli -- sessions update -i <session_id> -t "New Title"
cargo run --bin admin-cli -- sessions update -i <session_id> --clear-title
cargo run --bin admin-cli -- sessions update -i <session_id> -p <public_key>  # bind to user
cargo run --bin admin-cli -- sessions update -i <session_id> --clear-public-key  # unbind

# Delete session
cargo run --bin admin-cli -- sessions delete -i <session_id>
```

### Contracts

```bash
# List contracts
cargo run --bin admin-cli -- contracts list
cargo run --bin admin-cli -- contracts list -c 1  # filter by chain_id
cargo run --bin admin-cli -- contracts list -s USDC  # filter by symbol
cargo run --bin admin-cli -- contracts list -p uniswap  # filter by protocol

# Update contract
cargo run --bin admin-cli -- contracts update -c <chain_id> -a <address> -n "Token Name"
cargo run --bin admin-cli -- contracts update -c <chain_id> -a <address> -s "SYM" -p "protocol"
cargo run --bin admin-cli -- contracts update -c <chain_id> -a <address> --is-proxy -i <impl_address>

# Delete contract
cargo run --bin admin-cli -- contracts delete -c <chain_id> -a <address>
```

## Output

All commands output JSON for easy parsing:

```json
[
  {
    "id": 1,
    "api_key": "aomi-xxxx...",
    "label": "delta-rfq",
    "namespace": "delta",
    "is_active": true,
    "created_at": 1770311499
  }
]
```

## Database Schema

The CLI operates on these tables:
- `api_keys` - API key access control (one row per key+namespace)
- `users` - Users identified by wallet public key
- `sessions` - Chat sessions with optional wallet binding
- `contracts` - Smart contract metadata
