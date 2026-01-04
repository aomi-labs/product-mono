# AOMI Environment Metadata

> **Purpose**: Machine-critical facts about the environment, dependencies, and file structure.
>
> **Rule**: This file contains only factual, external, pure information. No semantic rules or prescriptions.
>
> **Important**: Metadata must be factual and external. Domain logic is semantic and prescriptive. Never mix them.

---

## 1. Tooling Versions

- Rust/Cargo nightly (edition 2024) — see `aomi/rust-toolchain.toml` for the pinned channel.
- Node.js 20.x — align with `frontend/.nvmrc` if present; check `package.json` engines.
- Databases: PostgreSQL 17 (prod), SQLite 3.x (dev/tests).
- Docker 24+ for container builds.

---

## 2. Repository File Tree

```
forge-mcp-backend/
├── aomi/                               # Rust workspace root
│   ├── Cargo.toml                      # Workspace configuration (edition 2024)
│   ├── Cargo.lock                      # Dependency lock file
│   │
│   ├── bin/                            # Binary crates (executables)
│   │   ├── backend/                    # HTTP REST API server
│   │   │   ├── Cargo.toml
│   │   │   ├── src/
│   │   │   │   ├── main.rs             # Entry point, migrations, session init
│   │   │   │   └── endpoint/           # API route handlers
│   │   │   │       ├── mod.rs          # Router, chat endpoints
│   │   │   │       ├── sessions.rs     # Session CRUD
│   │   │   │       ├── system.rs       # SSE updates
│   │   │   │       ├── db.rs           # Debug endpoints
│   │   │   │       └── types.rs        # API response types
│   │   │   └── migrations/             # SQLx database migrations
│   │   │       ├── 20250109000000_initial_schema.sql
│   │   │       ├── 20250110000000_add_session_persistence.sql
│   │   │       └── 20250112000000_update_contracts_schema.sql
│   │   │
│   │   ├── cli/                        # Command-line interface
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── main.rs             # CLI entry point
│   │   │       └── session.rs          # Session management
│   │   │
│   │   └── tui/                        # Terminal UI (Ratatui)
│   │       ├── Cargo.toml
│   │       └── src/
│   │           ├── main.rs             # TUI entry point
│   │           └── app.rs              # Application state
│   │
│   ├── crates/                         # Library crates
│   │   ├── backend/                    # Session & history management
│   │   │   ├── Cargo.toml
│   │   │   ├── src/
│   │   │   │   ├── lib.rs              # Module exports
│   │   │   │   ├── session.rs          # SessionState, ChatMessage (~700 lines)
│   │   │   │   ├── manager.rs          # SessionManager (~700 lines)
│   │   │   │   └── history.rs          # HistoryBackend trait (~400 lines)
│   │   │   └── tests/
│   │   │       ├── history_tests.rs
│   │   │       └── session_tests.rs
│   │   │
│   │   ├── chat/                       # LLM agent orchestration
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── app.rs              # ChatApp, ChatAppBuilder (~14KB)
│   │   │       ├── completion.rs       # LLM completion logic (~18KB)
│   │   │       ├── accounts.rs         # Account context generation
│   │   │       ├── connections.rs      # MCP connection management
│   │   │       └── prompts.rs          # System prompts, PreambleBuilder
│   │   │
│   │   ├── tools/                      # Tool implementations
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── tools.rs            # Tool trait impls (~31KB)
│   │   │       ├── scheduler.rs        # ToolScheduler (~19KB)
│   │   │       ├── account.rs          # GetAccountInfo, GetAccountTransactionHistory (~21KB)
│   │   │       ├── cast.rs             # CallViewFunction, SimulateContractCall (~27KB)
│   │   │       ├── etherscan.rs        # GetContractFromEtherscan (~21KB)
│   │   │       ├── wallet.rs           # SendTransactionToWallet
│   │   │       ├── abi_encoder.rs      # EncodeFunctionCall
│   │   │       ├── brave_search.rs     # BraveSearch tool
│   │   │       ├── db_tools.rs         # GetContractABI, GetContractSourceCode
│   │   │       ├── clients.rs          # External API clients
│   │   │       └── db/                 # Database layer
│   │   │           ├── mod.rs
│   │   │           ├── traits.rs       # Storage API traits
│   │   │           ├── contract_store.rs
│   │   │           ├── session_store.rs
│   │   │           └── transaction_store.rs
│   │   │
│   │   ├── mcp/                        # Model Context Protocol server
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── main.rs             # MCP server entry point
│   │   │       ├── combined_tool.rs    # CombinedTool aggregation
│   │   │       ├── cast.rs             # Cast tool implementation
│   │   │       ├── brave_search.rs     # Web search tool
│   │   │       ├── etherscan.rs        # Contract fetching
│   │   │       └── zerox.rs            # 0x swap quotes
│   │   │
│   │   ├── rag/                        # RAG system
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── embeddings.rs       # FastEmbed integration
│   │   │       ├── documents.rs        # Document parsing, chunking
│   │   │       └── vector_store.rs     # In-memory vector search
│   │   │
│   │   ├── eval/                       # Evaluation framework
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── eval_app.rs
│   │   │       ├── eval_state.rs
│   │   │       └── harness.rs
│   │   │
│   │   └── l2beat/                     # L2 protocol analysis
│   │       ├── Cargo.toml
│   │       ├── baml_src/               # BAML function definitions
│   │       │   ├── generate_conversation_summary.baml
│   │       │   ├── analyze_*.baml
│   │       │   └── clients.baml
│   │       ├── baml_client/            # Auto-generated BAML client
│   │       │   └── src/
│   │       │       ├── apis/default_api.rs
│   │       │       └── models/generate_title_request.rs
│   │       ├── baml.Dockerfile         # BAML server container
│   │       └── src/
│   │           ├── lib.rs
│   │           ├── app.rs              # L2BeatApp
│   │           └── runner.rs           # Discovery runner
│   │
│   └── documents/                      # RAG source documents
│       └── uniswap/                    # Uniswap protocol documentation
│
├── specs/                              # Architecture documentation (at repo root)
│   ├── DOMAIN.md                       # Invariants and rules
│   ├── PROGRESS.md                     # Current progress/sprint state
│   └── METADATA.md                     # This file
│
├── frontend/                           # Next.js 15 web application
│   ├── package.json
│   ├── tsconfig.json
│   ├── next.config.js
│   └── src/
│       ├── app/                        # Next.js app router
│       ├── components/                 # React components
│       └── lib/                        # Utilities
│           ├── chat-manager.ts         # API communication
│           ├── wallet-manager.ts       # Wallet connection
│           └── anvil-manager.ts        # Local testnet
│
├── docker/                             # Docker configurations
│   ├── docker-compose-backend.yml      # Production backend stack
│   ├── docker-compose-monolithic.yml   # All-in-one deployment
│   ├── entrypoints/                    # Container startup scripts
│   │   ├── backend-entrypoint.sh
│   │   └── mcp-entrypoint.sh
│   ├── nginx/                          # Reverse proxy configuration
│   └── DEPLOYMENT.md                   # Deployment documentation
│
├── scripts/                            # Automation scripts
│   ├── dev.sh                          # Development startup
│   ├── compose-backend-prod.sh         # Production Docker setup
│   ├── configure.py                    # YAML config loader
│   └── fetch_contracts.sh              # Contract data fetching
│
├── config.yaml                         # Environment configurations
├── .env.template                       # Environment variables template
├── Dockerfile                          # Multi-stage build
└── README.md                           # Project documentation
```

---

## 3. Key Dependencies

- Rust workspace: Alloy stack for blockchain, Axum/Tower for HTTP, SQLx for DB, Tokio async, serde/thiserror/anyhow for data + errors, rig-core/rmcp for AI. Source of truth is `aomi/Cargo.toml`.
- Frontend: Next.js 15 + React 18, wagmi/viem, Tailwind, React Query. Source of truth is `frontend/package.json`.

---

## 4. Environment Variables

### Required Variables
| Variable | Purpose | Example |
|----------|---------|---------|
| `ANTHROPIC_API_KEY` | Claude API access | `sk-ant-...` |
| `DATABASE_URL` | PostgreSQL connection | `postgres://user@host:5432/db` |

### Optional Variables
| Variable | Purpose | Default |
|----------|---------|---------|
| `BRAVE_SEARCH_API_KEY` | Web search | - |
| `ETHERSCAN_API_KEY` | Contract data | - |
| `ALCHEMY_API_KEY` | RPC endpoints | - |
| `ZEROX_API_KEY` | Token swaps | - |

### Service Configuration
| Variable | Purpose | Default |
|----------|---------|---------|
| `BACKEND_HOST` | Backend bind address | `127.0.0.1` (dev), `0.0.0.0` (prod) |
| `BACKEND_PORT` | Backend port | `8080` (dev), `8081` (prod) |
| `MCP_SERVER_HOST` | MCP bind address | `127.0.0.1` (dev), `0.0.0.0` (prod) |
| `MCP_SERVER_PORT` | MCP port | `5000` (dev), `5001` (prod) |
| `BACKEND_SKIP_DOCS` | Skip RAG loading | `false` |
| `BACKEND_SKIP_MCP` | Skip MCP connection | `false` |
| `RUST_LOG` | Logging level | `info` |

### Frontend Configuration
| Variable | Purpose | Default |
|----------|---------|---------|
| `NEXT_PUBLIC_BACKEND_URL` | Backend API URL | `http://localhost:8081` |
| `NEXT_PUBLIC_ANVIL_URL` | Anvil RPC URL | `http://localhost:8545` |

---

## 5. Port Mapping

### Development Environment
| Service | Host | Port | Protocol |
|---------|------|------|----------|
| Backend REST API | 127.0.0.1 | 8080 | HTTP |
| MCP Server | 127.0.0.1 | 5000 | HTTP |
| Frontend | 127.0.0.1 | 3000 | HTTP |
| Anvil (local testnet) | 127.0.0.1 | 8545 | JSON-RPC |

### Production Environment
| Service | Host | Port | Protocol |
|---------|------|------|----------|
| Backend REST API | 0.0.0.0 | 8081 | HTTP |
| MCP Server | 0.0.0.0 | 5001 | HTTP |
| Frontend | 0.0.0.0 | 3000 | HTTP |
| PostgreSQL | postgres | 5432 | PostgreSQL |

---

## 6. Supported Networks

Configured in `config.yaml`:

| Network | Chain ID | RPC Template |
|---------|----------|--------------|
| `testnet` | local | `http://127.0.0.1:8545` (dev) / `http://anvil:8545` (prod) |
| `ethereum` | 1 | `https://eth-mainnet.g.alchemy.com/v2/{ALCHEMY_API_KEY}` |
| `base` | 8453 | `https://base-mainnet.g.alchemy.com/v2/{ALCHEMY_API_KEY}` |
| `arbitrum` | 42161 | `https://arb-mainnet.g.alchemy.com/v2/{ALCHEMY_API_KEY}` |
| `optimism` | 10 | `https://opt-mainnet.g.alchemy.com/v2/{ALCHEMY_API_KEY}` |
| `polygon` | 137 | `https://polygon-mainnet.g.alchemy.com/v2/{ALCHEMY_API_KEY}` |
| `sepolia` | 11155111 | `https://eth-sepolia.g.alchemy.com/v2/{ALCHEMY_API_KEY}` |

---

## 7. Database Schema

### Tables

**users**
```sql
CREATE TABLE users (
    public_key TEXT PRIMARY KEY,
    username TEXT UNIQUE,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
```

**sessions**
```sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    public_key TEXT REFERENCES users(public_key) ON DELETE SET NULL,
    started_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    last_active_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    title TEXT,
    pending_transaction TEXT  -- JSONB in PostgreSQL
);
```

**messages**
```sql
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    message_type TEXT NOT NULL DEFAULT 'chat',
    sender TEXT NOT NULL,
    content TEXT NOT NULL,  -- JSONB in PostgreSQL
    timestamp INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
```

**contracts**
```sql
CREATE TABLE contracts (
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    name TEXT,
    symbol TEXT,
    protocol TEXT,
    contract_type TEXT,
    version TEXT,
    is_proxy BOOLEAN,
    implementation_address TEXT,
    abi TEXT,
    source_code TEXT,
    fetched_at INTEGER,
    PRIMARY KEY (chain_id, address)
);
```

**transactions**
```sql
CREATE TABLE transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    hash TEXT NOT NULL,
    -- Additional transaction metadata...
);
```

---

## 8. API Endpoints

### Session Management
| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/sessions` | List sessions for user |
| `POST` | `/api/sessions` | Create new session |
| `GET` | `/api/sessions/:id` | Get session state |
| `DELETE` | `/api/sessions/:id` | Delete session |
| `PATCH` | `/api/sessions/:id` | Rename session |
| `POST` | `/api/sessions/:id/archive` | Archive session |
| `POST` | `/api/sessions/:id/unarchive` | Unarchive session |

### Chat
| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/chat` | Send message |
| `GET` | `/api/state` | Get chat state |
| `GET` | `/api/chat/stream` | SSE stream (**deprecated**) |
| `POST` | `/api/interrupt` | Interrupt processing |

### System
| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/updates` | SSE for system updates |
| `POST` | `/api/system` | System message |
| `POST` | `/api/memory-mode` | Toggle memory mode |
| `GET` | `/health` | Health check |

---

## 9. Module Dependency Graph

```
                    ┌─────────────────────────────────────┐
                    │          BINARY ENTRY POINTS         │
                    │   backend    cli    tui              │
                    └──────────────┬──────────────────────┘
                                   │
                    ┌──────────────▼──────────────────────┐
                    │            aomi-chat                 │
                    │  (ChatApp, ChatAppBuilder, prompts)  │
                    └──────────────┬──────────────────────┘
                                   │
         ┌────────────┬────────────┼────────────┬─────────────┐
         ▼            ▼            ▼            ▼             ▼
   aomi-backend   aomi-tools   aomi-l2beat   aomi-rag    aomi-mcp
   (sessions)     (tools, db)  (discovery)  (embeddings) (protocol)
         │            │            │            │             │
         └────────────┴────────────┴────────────┴─────────────┘
                                   │
                    ┌──────────────▼──────────────────────┐
                    │         External Services            │
                    │  PostgreSQL, Anthropic, Etherscan    │
                    │  BAML Server, RPC Nodes, 0x API      │
                    └─────────────────────────────────────┘
```

---

## 10. Build & Run Commands

### Development
```bash
# Start backend (from aomi/)
cargo run --bin backend

# Start MCP server
cargo run --bin aomi-mcp-server

# Start frontend (from frontend/)
npm run dev

# Run all tests
cargo test

# Run specific package tests
cargo test --package aomi-backend

# Check compilation
cargo check

# Format code
cargo fmt

# Lint
cargo clippy
```

### Production (Docker)
```bash
# Build all images
docker build --target backend-runtime -t aomi-backend .
docker build --target mcp-runtime -t aomi-mcp .
docker build --target frontend-runtime -t aomi-frontend .

# Start with docker-compose
cd docker && docker-compose -f docker-compose-backend.yml up -d
```

---

## 11. Docker Services

### docker-compose-backend.yml

| Service | Image | Port | Depends On |
|---------|-------|------|------------|
| `backend` | `ghcr.io/aomi-labs/product-mono/backend` | 8081 | baml, postgres |
| `baml` | Built from `baml.Dockerfile` | 2024 | - |
| `postgres` | `postgres:17` | 5432 | - |
| `db-init` | `postgres:17` | - | postgres |

### Volumes
- `postgres_data` - PostgreSQL persistent storage

---

## 12. External Service URLs

| Service | Base URL | Purpose |
|---------|----------|---------|
| Anthropic Claude | `https://api.anthropic.com` | LLM inference |
| Brave Search | `https://api.search.brave.com` | Web search |
| Etherscan | `https://api.etherscan.io` | Contract data |
| 0x Protocol | `https://api.0x.org` | Token swaps |
| Alchemy | `https://*.g.alchemy.com/v2/` | RPC endpoints |

---

## 13. Connection Settings (from config.yaml)

```yaml
connection:
  max_reconnect_attempts: 5
  reconnect_delay_ms: 3000
  sse_interval_ms: 500
  keep_alive_interval_s: 1
  health_check_timeout_s: 30
  mcp_connection_timeout_s: 20

chat:
  max_message_length: 2000
  scroll_threshold: 50
```
