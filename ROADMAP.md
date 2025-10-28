# Aomi Product Roadmap

## 1. DevOps & Testing

### CI/CD Hardening
**Files**: `.github/workflows/`, `docker/docker-compose-*.yml`, `scripts/dev.sh`, `scripts/compose-backend-prod.sh`

- [ ] Strengthen GitHub Actions workflows with proper test gates
- [ ] Add automated security scanning and dependency updates
- [ ] Implement proper staging/production deployment pipelines
- [ ] Add Docker image vulnerability scanning

### Robust Eval Scripts & Benchmarking
**Files**: `chatbot/bin/backend/src/tests.rs` (expand), `scripts/benchmark/` (new), `chatbot/bin/backend/src/manager.rs`

- [ ] Create systematic chat benchmarking harness
- [ ] Test conversation length performance (1-turn vs 50-turn conversations)
- [ ] Test concurrent session scalability (1 vs 100 simultaneous users)
- [ ] Add memory usage and response time metrics
- [ ] Automated regression testing for agent quality

## 2. Chat UX

### Frontend Public Key Support
**Files**: `frontend/src/lib/chat-manager.ts`, `chatbot/bin/backend/src/history.rs`

- [ ] Add public_key field to user history data structure
- [ ] Update frontend to display and manage wallet addresses in chat
- [ ] Persist wallet connection state across sessions

### Tool Output Stream Cleaning
**Files**: `chatbot/crates/agent/src/tool_scheduler.rs`, `chatbot/bin/backend/src/session.rs:140`, `frontend/src/components/ui/message.tsx`

**Issue**: Async tool scheduler doesn't output tool results anymore, just shows "waiting xx tool..."
- [ ] Fix `process_tool_call()` in `completion.rs:170-178` to stream intermediate results
- [ ] Clean and format tool outputs (e.g., `brave_search: {...}` → nicely formatted display)
- [ ] Stream real-time tool execution progress to frontend

### SSE Timing & Network Switching Reactivity
**Files**: `frontend/src/lib/chat-manager.ts`, `chatbot/bin/backend/src/session.rs`, `chatbot/bin/backend/src/endpoint.rs`

**Issue**: When switching networks, bot doesn't react to user's immediate next message
- [ ] Fine-tune SSE message timing and display logic
- [ ] Fix frontend/backend coordination for network state changes
- [ ] Ensure chat responsiveness after network switches

### Error Propagation
**Issue**: When backend has errors, frontend stalls in multi-threading

- [ ] Improve error handling between backend and frontend
- [ ] Display error messages in chat instead of stalling
- [ ] Let LLM naturally handle and communicate errors to users

## 3. Backend Components

### Persistent Database for User History
**Files**: `chatbot/bin/backend/src/history.rs` (currently in-memory)

- [ ] Replace in-memory history with persistent database (PostgreSQL/SQLite)
- [ ] Ensure proper retrieval and search functionality
- [ ] Consider separate MCP instance for history management
- [ ] Support multiple backend instances for different apps (L2b, etc.)

### Native Tools Migration
**Files**: `chatbot/crates/mcp/src/cast.rs` → native, `chatbot/crates/mcp/src/brave_search.rs` → native

- [ ] Move cast tool to native layer using `#[rig_tool]` and direct Rust SDK
- [ ] Move brave_search to native layer with direct API integration
- [ ] Eliminate MCP process overhead for core tools

### Dynamic Network Switching for Native Cast
**Files**: `chatbot/crates/mcp/src/combined_tool.rs:40-41` (current `HashMap<String, CastTool>`)

**Analysis**: Cast/Alloy provider re-initialization is lightweight enough for dynamic switching
- [ ] Implement `switch_network()` method on `CastTool` 
- [ ] Use provider re-initialization approach (`ProviderBuilder::connect(&new_rpc_url)`)
- [ ] Replace per-network instances with single dynamic instance

### Persistent Contract Database
**Files**: `chatbot/crates/mcp/src/etherscan.rs` (current ABI retrieval)

- [ ] Create native `get_contract(address: String)` tool
- [ ] Keep simple interface with Etherscan fallback
- [ ] Cache frequently used contract ABIs and metadata

### Centralized Database Architecture
- [ ] Host all databases in separate instance/service
- [ ] Create unified table for agent-valuable information
- [ ] Design for multi-app architecture (different apps, different data needs)

### Transaction Simulation
**Files**: `chatbot/crates/agent/src/abi_encoder.rs` (integration point)

- [ ] Add transaction simulation endpoint before user signing
- [ ] Design carefully to avoid performance degradation
- [ ] Integrate with existing transaction encoding workflow

## 4. Abstractions

### trait AomiApp + trait ChatBackend
**Files**: `chatbot/bin/backend/src/session.rs` (TODO: "eventually AomiApp")

**Architecture**:
```
[FE1, FE2, FE3] ←→ [ChatBackend] ←→ [App1, App2, App3]
```

- [ ] **ChatBackend**: Stateless interface for frontend communication
  - Handle channels and history only
  - No agent logic exposed to SessionState
  - Support different ChatCommand variants per app type
- [ ] **AomiApp**: Encapsulate agentic logic and tools
  - Different loop implementations (linear, tree search, batch execution)
  - Custom tool orchestration per app
  - Hide complexity from ChatBackend

### Chat Stream Loop Abstraction
- [ ] Abstract chat stream loops into reusable interface
- [ ] Support different streaming patterns per app type
- [ ] Maintain clean separation between interface and implementation

### AomiApp + ToolScheduler Integration
**Files**: `chatbot/crates/agent/src/tool_scheduler.rs`

- [ ] Design AomiApp to work seamlessly with ToolScheduler
- [ ] Support different tool execution patterns per app
- [ ] Maintain tool registration and orchestration flexibility

### Native BAML Rust Support
**Files**: `chatbot/crates/agent/src/agent.rs` (current Rig client)

**Integration Strategy**: BAML alongside Rig, not replacement
- [ ] Wrap BamlClient alongside RigClient in scheduler
- [ ] Use BAML for one-shot data processing subroutines
- [ ] Enable local small models for cost optimization
- [ ] Implement `#[rig_tool]` wrappers around `baml_client.my_parse_logic(input)`
- [ ] Support model switching (Claude for conversation, local models for data processing)

---

## Priority Order

1. **Chat UX** (direct user impact)
   - Tool output streaming fix
   - Error propagation improvements
   
2. **Backend Components** (foundation)
   - Persistent database migration
   - Native tools migration
   - Dynamic cast switching

3. **Abstractions** (scalability)
   - ChatBackend/AomiApp separation
   - BAML integration

4. **DevOps & Testing** (long-term stability)
   - Benchmarking harness
   - CI/CD improvements