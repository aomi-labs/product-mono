# AOMI Domain Logic

> **Purpose**: Immutable truths of the system - rules that never change unless a human changes them.
>
> **This is the instruction manual your agents must obey. Never rewrite these accidentally.**

---

## 1. System Identity

### What is AOMI?

AOMI (Aomi Terminal) is an **AI-powered blockchain operations assistant** that enables natural language interactions for Web3 operations. Users can:
- Query blockchain state (balances, contract data, transaction history)
- Execute token swaps via 0x Protocol
- Call smart contract functions
- Send transactions through connected wallets
- Search the web for blockchain information

### Core Value Proposition
Transform complex blockchain operations into conversational interactions while maintaining security through simulation-before-execution and user wallet confirmation.

---

## 2. Architecture Decisions

### 2.1 Multi-Service Architecture
```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│    Frontend     │────▶│     Backend     │────▶│   MCP Server    │
│  (Next.js 15)   │     │  (Rust/Axum)    │     │  (Rust/RMCP)    │
└─────────────────┘     └─────────────────┘     └─────────────────┘
        │                       │                       │
        ▼                       ▼                       ▼
   wagmi/viem             Anthropic Claude        Alloy (Ethereum)
   Wallet Connect           rig-core              Cast CLI tools
```

**Rationale**: Separating MCP from the backend allows:
- Independent scaling of AI tool execution
- Isolated blockchain interaction failures
- Reuse of MCP tools across different clients

### 2.2 Multi-Backend Agent System
- Supports multiple backend types: `Default` (ChatApp) and `L2b` (L2BeatApp)
- Backend switching per-session via `SessionManager.replace_backend()`
- Each backend implements the `AomiBackend` trait

### 2.3 Session-Based State Management
All user interactions are scoped to a **session**. Sessions have two distinct state types:

| State Type | Purpose | Storage |
|------------|---------|---------|
| `SessionState<S>` | Chat-stream state (messages, processing, tool streams) | In-memory (DashMap) |
| `SessionData` | Metadata (title, archive status, history) | Database (PostgreSQL/SQLite) |

### 2.4 Trait-Based Abstractions
All major components use trait-based abstractions for testability and flexibility:

| Trait | Purpose | Implementations |
|-------|---------|-----------------|
| `AomiBackend` | Chat backend interface | `ChatApp`, `L2BeatApp` |
| `HistoryBackend` | Session persistence | `PersistentHistoryBackend`, `NoOpHistoryBackend` |
| `ContractStoreApi` | Contract metadata storage | `ContractStore` |
| `SessionStoreApi` | Session/message persistence | `SessionStore` |
| `TransactionStoreApi` | Transaction history | `TransactionStore` |

### 2.5 Streaming-First Design
- All LLM responses stream via `ChatCommand<S>` enum
- Tool results stream via `ToolResultStream` (BoxStream)
- SSE endpoints for real-time updates (`/api/updates`, `/api/chat/stream`)

---

## 3. Invariants

### 3.1 Session Invariants
| Invariant | Rule |
|-----------|------|
| **Session ID uniqueness** | Generated via `Uuid::new_v4()`, never reused |
| **Title fallback format** | Placeholder titles use `#[session_id_prefix]` format (e.g., `#[abc123]`) |
| **Title detection** | Check `title.starts_with("#[")` to detect placeholder titles - **NEVER use length heuristics** |
| **Archived sessions** | Never auto-generate titles for archived sessions |

### 3.2 Message Invariants
| Invariant | Rule |
|-----------|------|
| **Streaming exclusion** | `is_streaming: true` messages are never persisted |
| **System message filtering** | System messages are filtered out before sending to LLM |
| **Message order** | Messages maintain chronological order by timestamp |
| **Content types** | Messages can be `Text`, `ToolResult`, or structured content |

### 3.3 Tool Execution Invariants
| Invariant | Rule |
|-----------|------|
| **No concurrent overflow** | `ToolScheduler` manages execution queue |
| **Tool results are strings** | All tool outputs serialize to `String` |
| **Interrupt support** | Any tool execution can be interrupted via broadcast channel |
| **Wallet special handling** | `SendTransactionToWallet` queues pending transactions, doesn't execute directly |

### 3.4 Database Invariants
| Invariant | Rule |
|-----------|------|
| **Idempotent migrations** | SQLx migrations in `bin/backend/migrations/` |
| **Session before messages** | Cannot save messages without session record |
| **Cascade deletes** | Deleting a session removes all associated messages |

---

## 4. Constraints

### 4.1 Blockchain Interactions
- **All blockchain calls go through Alloy** - NEVER use ethers-rs
- **Simulate before sending** - Wallet transactions must be simulated before execution
- **RPC endpoints** - Configure via `CHAIN_NETWORK_URLS_JSON` or config.yaml
- **Multi-network support** - Each tool call specifies target network

### 4.2 LLM Interactions
- **Use rig framework** - All LLM calls go through `rig-core`
- **Anthropic Claude** - Primary provider (claude-sonnet-4-20250514)
- **BAML for structured output** - Use BAML client for type-safe AI responses (title generation, summaries)
- **System prompts** - Composed via `PreambleBuilder` from `prompts.rs`

### 4.3 Concurrency
- **Tokio runtime required** - All async code runs on Tokio
- **DashMap for shared state** - Use `DashMap` for concurrent session access
- **Arc<Mutex<T>> for session state** - Fine-grained locking per session
- **Never hold locks across await** - Acquire, read/write, release immediately

### 4.4 Error Handling
- **anyhow for application errors** - Use `anyhow::Result` for fallible operations
- **thiserror for library errors** - Define custom error types with `thiserror`
- **Never panic in production** - Use `tracing::error!` instead of `panic!`
- **Surface tool errors verbatim** - Never imply a failed call worked

---

## 5. Naming Conventions

### 5.1 Crate Naming
```
Binary crates:   aomi-{name}     (e.g., aomi-cli, aomi-tui)
Library crates:  aomi-{name}     (e.g., aomi-backend, aomi-tools)
Package name:    aomi_{name}     (in Cargo.toml: name = "aomi_backend")
```

### 5.2 Type Naming
```rust
Traits:     PascalCase + descriptive suffix    // HistoryBackend, ContractStoreApi
Structs:    PascalCase                         // SessionManager, ChatMessage
Enums:      PascalCase                         // BackendType, MessageSender
Tools:      VerbNoun                           // SendTransactionToWallet, GetAccountInfo
```

### 5.3 Function Naming
```rust
Async operations:    get_or_create_session, flush_history
Background tasks:    start_{name}_task         // start_title_generation_task
Builders:            with_{field}              // with_backend, with_docs
Constructors:        new, default
```

### 5.4 File Naming
```
Rust files:    snake_case.rs                   // session_store.rs, tool_scheduler.rs
Test files:    {module}_tests.rs in tests/     // history_tests.rs
Migrations:    {timestamp}_{description}.sql   // 20250109000000_initial_schema.sql
```

---

## 6. Idioms

### 7.1 Session Access Pattern
```rust
// Always get session through SessionManager
let session = session_manager.get_or_create_session(&session_id, backend, title).await?;

// Lock session state only when needed
let chat_state = {
    let state = session.lock().await;
    state.get_chat_state()
}; // Lock released immediately

// NEVER: session.lock().await.do_async_work().await
```

### 7.2 Tool Registration Pattern
```rust
// Tools implement the Rig Tool trait
impl Tool for MyTool {
    const NAME: &'static str = "my_tool";
    type Args = MyToolParams;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _: String) -> ToolDefinition { ... }
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> { ... }
}

// Register with ToolScheduler for unified execution
scheduler.register_tool::<MyTool>()?;
```

### 7.3 Database Access Pattern
```rust
// Use trait objects for storage abstraction
let db: Arc<dyn SessionStoreApi> = Arc::new(SessionStore::new(pool));

// Async trait methods with anyhow::Result
db.create_session(&session).await?;
db.get_session(&session_id).await?;
```

### 7.4 Streaming Pattern
```rust
// ChatCommand enum for streaming responses
enum ChatCommand<S> {
    TextStreaming(String),
    ToolCall { name: String, args: Value },
    ToolResult { name: String, result: Result<Value, String> },
    WalletTransactionRequest(Transaction),
    Complete,
    Error(String),
}

// Consume via channel receiver
while let Some(cmd) = receiver.recv().await {
    match cmd {
        ChatCommand::TextStreaming(text) => append_to_message(text),
        ChatCommand::Complete => break,
        // ...
    }
}
```

### 7.5 Background Task Pattern
```rust
// Spawn background tasks with Arc<Self>
pub fn start_title_generation_task(self: Arc<Self>) {
    let manager = Arc::clone(&self);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            // Process sessions - check for title generation needs
        }
    });
}
```

### 7.6 Prompt Composition Pattern
```rust
// Use PreambleBuilder for composable system prompts
let preamble = agent_preamble_builder()
    .section(agent_identity_section())
    .section(workflow_section())
    .section(constraints_section())
    .section(tool_instructions_section())
    .build();
```

---

## 7. API Design Rules

### 8.1 REST Endpoints
- Use Axum router with typed extractors
- Session-scoped endpoints: `/api/sessions/:session_id`
- SSE for real-time: `/api/updates`
- All endpoints return JSON

### 8.2 Response Types
- API responses defined in `bin/backend/src/endpoint/types.rs`
- Use `SessionResponse` for chat state + title
- Use `FullSessionState` for admin/debug endpoints

### 8.3 Error Responses
| Condition | Status Code |
|-----------|-------------|
| Invalid input | `400 Bad Request` |
| Missing resource | `404 Not Found` |
| Unexpected failure | `500 Internal Server Error` |

---

## 8. Testing Rules

### 9.1 Test Organization
- Unit tests in `#[cfg(test)]` modules within source files
- Integration tests in `crates/{name}/tests/`
- Use `#[tokio::test]` for async tests

### 9.2 Test Database
- Use SQLite in-memory for tests: `sqlite::memory:`
- Mark PostgreSQL-specific tests with `#[ignore]`
- Set up test fixtures in `setup_test_db()` functions

### 9.3 Mock Implementations
- Implement traits for mock types in tests
- Use `NoOpHistoryBackend` for tests that don't need persistence

---

## 9. Security Rules

### 10.1 Transaction Safety
- **Always simulate before sending** - Call `simulate_transaction` before `send_transaction`
- **Pending transactions require wallet confirmation** - Never auto-execute transactions
- **Display value changes** - Show recipient balances after transfers

### 10.2 API Key Handling
- API keys in environment variables, never in code
- `.env` files are gitignored
- Use `.env.template` for documentation

### 10.3 Input Validation
- Validate wallet addresses (checksum, format)
- Validate chain IDs against supported networks
- Sanitize user input before passing to tools

---

## 10. Agent Behavioral Rules

These rules are embedded in the system prompt and govern LLM behavior:

### 11.1 Role
> "You are an Ethereum ops assistant. Keep replies crisp, ground every claim in real tool output, and say 'I don't know' or 'that failed' whenever that is the truth."

### 11.2 Workflow
1. Briefly name the step you're on
2. Parallelize tool calls as needed
3. Report what actually happened, including any failures
4. Repeat until the request is complete or blocked

### 11.3 Constraints
- Confirm whether each transaction succeeded; show recipient balances that changed
- Surface tool errors verbatim; never imply a failed call worked
- During a single step, run multiple tool calls, but only address user between steps
- When a transaction is rejected, acknowledge it and suggest alternatives

### 11.4 Tool Priority
- Prefer deterministic tools (GetContractABI, CallViewFunction) over web search
- Only search if required data is not available in structured tools
- Pay attention to tool descriptions and argument priority
