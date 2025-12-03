# Project Progress: ForgeExecutor + BAML Integration

**Branch:** `mono-be-foundry`
**Last Updated:** 2025-12-03

---

## Sprint Goal

Build a stateful, dependency-aware ForgeExecutor that:
1. Accepts natural language operation groups from LLM agents
2. Uses BAML (two-phase LLM code generation) to generate Solidity scripts
3. Executes operations with proper dependency ordering
4. Returns formatted transactions ready for wallet signing

**Status:** 🟡 In Progress

---

## Architecture Overview

### Two-Phase BAML Flow

**Phase 1: Extract Contract Info**
```
ContractInfo (from DB) + Operations → ExtractedContractInfo
- Functions with signatures, ABI, body
- Storage declarations and slots
- Events with signatures and ABI
```

**Phase 2: Generate Script**
```
ExtractedContractInfo + Operations → ScriptBlock
- CodeLine[] with imports/interfaces
- Ready to assemble into Forge script
```

### ForgeExecutor Workflow

```
Agent → set_execution_plan(OperationGroup[])
    ↓
ForgeExecutor::new()
    - Creates ExecutionPlan with dependency graph
    - Spawns background SourceFetcher
    - Returns immediately
    ↓
Agent → next_groups()
    ↓
ForgeExecutor::next_groups()
    - Gets ready groups (dependencies satisfied)
    - Waits for sources from SourceFetcher
    - Calls BAML Phase 1 (extract_contract_info)
    - Calls BAML Phase 2 (generate_script)
    - Assembles and compiles Forge script
    - Executes with local fork
    - Returns formatted transactions
    ↓
Agent → Sends transactions to wallet
Agent → Calls next_groups() again...
```

---

## Branch Status

**Current Branch:** `mono-be-foundry`

**Recent Commits:**
```
a05c2a7 new forge_executor
7749463 baml and executor module
c98600a eval forge
6da6fd5 plans + generated .md
4e8cdca Merge feat/eval-part2 into mono-be-foundry
0e1945a fix clippy errors
a640d7e removed old tool and added implemented new design
```

---

## Recently Completed Work

### ✅ ForgeExecutor Consolidation (Codex)
- **Date:** 2025-12-03
- **Changes:**
  - Consolidated `forge_executor2` → `forge_executor`
  - Added `DashMap` for thread-safe contract session storage
  - Implemented `Drop` trait for `SourceFetcher` cleanup
  - Improved type definitions with consistent serialization
- **Files:** `src/forge_executor/{executor.rs, source_fetcher.rs, types.rs}`, `src/lib.rs`

### ✅ Unit Tests for forge_executor/tools.rs (Claude)
- **Date:** 2025-12-03
- **Changes:**
  - Created `src/forge_executor/test.rs` with 6 tests
  - `test_set_execution_plan_success_with_serialization` - validates tool JSON output
  - `test_next_groups_no_plan_error` - validates error handling
  - `test_next_groups_json_serialization` - validates NextGroups JSON structure
  - `test_group_result_serialization` - validates Done/Failed variant serialization
  - `test_full_workflow_set_and_execute` - integration test (ignored, requires full stack)
  - `test_group_result_serialization` - bonus test for result types
- **Test Results:** 5 passed, 1 ignored
- **Files:** `src/forge_executor/test.rs` (created), `src/forge_executor/mod.rs` (modified)

### ✅ BAML Schema Design
- **Date:** Earlier in sprint
- **Changes:**
  - Defined `ContractInfo`, `ExtractedContractInfo` schemas
  - Defined `ScriptBlock`, `CodeLine`, `Import`, `Interface` schemas
  - Implemented two-phase extraction → generation flow
- **Files:** BAML schema files (referenced in `tmp.txt`)

---

## Module Structure

### Core Modules

| Module | Description | Status |
|--------|-------------|--------|
| `executor.rs` | ForgeExecutor with dependency-aware execution | ✅ Complete |
| `plan.rs` | ExecutionPlan with dependency graph | ✅ Complete |
| `types.rs` | GroupResult, TransactionData, etc. | ✅ Complete |
| `source_fetcher.rs` | Background contract fetching service | ✅ Complete |
| `assembler.rs` | ScriptAssembler for Forge script generation | ✅ Complete |
| `tools.rs` | SetExecutionPlan & NextGroups Rig tools | ✅ Complete |
| `test.rs` | Unit/integration tests | ✅ Initial tests complete |

### Key Types

```rust
// Operation groups with dependencies
pub struct OperationGroup {
    pub description: String,
    pub operations: Vec<String>,
    pub dependencies: Vec<usize>,
    pub contracts: Vec<(String, String, String)>, // (chain_id, address, name)
}

// Execution results
pub enum GroupResultInner {
    Done {
        transactions: Vec<TransactionData>,
        generated_code: String,
    },
    Failed {
        error: String,
    },
}

// Transaction data for wallet
pub struct TransactionData {
    pub from: Option<String>,
    pub to: Option<String>,
    pub value: String,
    pub data: String,
    pub rpc_url: String,
}
```

---

## Files Modified This Sprint

### Core Implementation
- `aomi/crates/tools/src/forge_executor/executor.rs` - Main executor logic
- `aomi/crates/tools/src/forge_executor/plan.rs` - Dependency graph management
- `aomi/crates/tools/src/forge_executor/types.rs` - Result types
- `aomi/crates/tools/src/forge_executor/source_fetcher.rs` - Background fetcher
- `aomi/crates/tools/src/forge_executor/assembler.rs` - Script assembly
- `aomi/crates/tools/src/forge_executor/tools.rs` - Rig tool interface
- `aomi/crates/tools/src/forge_executor/mod.rs` - Module exports

### Tests
- `aomi/crates/tools/src/forge_executor/test.rs` - Unit/integration tests

### Configuration
- `aomi/crates/tools/src/lib.rs` - Removed forge_executor2, kept forge_executor

---

## Pending Tasks

### High Priority
- [ ] Add fixture files for integration test (`src/forge_executor/tests/fixtures/*.json`)
- [ ] Implement remaining unit tests identified in session:
  - `plan.rs`: dependency validation, circular dependency detection
  - `source_fetcher.rs`: timeout handling, error propagation
  - `assembler.rs`: funding requirement tests, import deduplication
  - `executor.rs`: concurrent execution, error handling
- [ ] Add timeout to `ForgeExecutor::next_groups()` contract fetching wait loop (line 81 in executor.rs)

### Medium Priority
- [ ] Document BAML schema files
- [ ] Add error recovery mechanisms for failed groups
- [ ] Implement retry logic for transient failures
- [ ] Add metrics/observability for execution flow

### Low Priority
- [ ] Optimize contract session reuse strategy
- [ ] Consider pooling for BAML client calls
- [ ] Performance benchmarks for large dependency graphs

---

## Known Issues

### Resolved
- ✅ forge_executor2 naming inconsistency → consolidated to forge_executor
- ✅ No Drop trait for SourceFetcher → added Drop implementation
- ✅ Contract sessions not thread-safe → using DashMap

### Active
- None currently

---

## Multi-Step Flow State

### Phase 1: Core Architecture ✅
- [x] Design BAML two-phase flow
- [x] Implement OperationGroup and ExecutionPlan
- [x] Build SourceFetcher background service
- [x] Create ScriptAssembler

### Phase 2: Executor Implementation ✅
- [x] Implement ForgeExecutor with dependency resolution
- [x] Integrate BAML client (Phase 1 & 2)
- [x] Add contract compilation and execution
- [x] Format transaction output for wallet

### Phase 3: Tool Integration ✅
- [x] Implement SetExecutionPlan Rig tool
- [x] Implement NextGroups Rig tool
- [x] Add global executor storage
- [x] Test JSON serialization

### Phase 4: Testing (In Progress) 🟡
- [x] Basic unit tests for tools.rs
- [x] Serialization tests for types.rs
- [ ] Integration tests with fixtures
- [ ] Full workflow test with real contracts
- [ ] Error handling tests
- [ ] Dependency graph edge case tests

### Phase 5: Production Readiness (Not Started) ⚪
- [ ] Add comprehensive error messages
- [ ] Implement logging/tracing
- [ ] Add metrics collection
- [ ] Performance optimization
- [ ] Documentation for agent integration

---

## Test Coverage

### Implemented Tests (src/forge_executor/test.rs)

| Test | Type | Status | Description |
|------|------|--------|-------------|
| `test_set_execution_plan_success_with_serialization` | Unit | ✅ Pass | Validates SetExecutionPlan JSON output |
| `test_next_groups_no_plan_error` | Unit | ✅ Pass | Validates error when no plan set |
| `test_next_groups_json_serialization` | Unit | ✅ Pass | Validates NextGroups JSON structure |
| `test_group_result_serialization` | Unit | ✅ Pass | Validates Done/Failed serialization |
| `test_full_workflow_set_and_execute` | Integration | 🟡 Ignored | Full workflow test (requires BAML/Etherscan) |

### Identified Tests (Not Yet Implemented)

**types.rs** (7 tests)
- Default values, serialization edge cases, deserialization

**plan.rs** (11 tests)
- Dependency validation, circular deps, ready batch selection, concurrent execution

**source_fetcher.rs** (9 tests)
- Concurrent fetching, caching, error handling, timeout

**assembler.rs** (20 tests)
- Import/interface handling, funding setup, pragma, indentation, edge cases

**executor.rs** (11 tests)
- Concurrent group execution, error handling, session reuse, shutdown

**tools.rs** (9 tests)
- Parameter validation, error serialization, concurrent calls, global state

---

## Notes for Next Agent

### Critical Context

1. **BAML Two-Phase Flow**: Always maintain the extract → generate separation. Phase 1 extracts contract structure, Phase 2 generates script code.

2. **Dependency Ordering**: The ExecutionPlan ensures groups are executed in dependency order. Never execute a group before its dependencies complete.

3. **Background Fetching**: SourceFetcher runs in a separate task and uses mpsc::channel for async communication. It must be properly shut down.

4. **Global Executor Storage**: The EXECUTOR static uses once_cell::Lazy + Arc<Mutex<Option<ForgeExecutor>>>. Only one executor can be active at a time.

5. **Transaction Format**: Backend doesn't broadcast transactions - it formats them for wallet signing. The `rpc_url` field tells the wallet which network to use.

### Integration Test Prerequisites

To run `test_full_workflow_set_and_execute`:
```bash
# Set environment variables
export DATABASE_URL="postgresql://..."
export ANTHROPIC_API_KEY="sk-..."
export AOMI_FORK_RPC="http://localhost:8545"  # Optional

# Run test
cargo test --lib test_full_workflow_set_and_execute -- --ignored --nocapture
```

Requirements:
- WETH contract in database or fetchable from Etherscan
- Local Anvil fork running (if using AOMI_FORK_RPC)

### Code Navigation

| Component | File | Key Lines |
|-----------|------|-----------|
| Executor creation | `executor.rs` | 29-63 |
| Next groups execution | `executor.rs` | 66-156 |
| Single group execution | `executor.rs` | 165-262 |
| SetExecutionPlan tool | `tools.rs` | 36-120 |
| NextGroups tool | `tools.rs` | 134-188 |
| Script assembly | `assembler.rs` | 26-131 |
| Dependency graph | `plan.rs` | Full file |

### Common Gotchas

1. **Test Isolation**: Tests that use the global EXECUTOR must be run sequentially or reset state between runs.

2. **Async Spawning**: ForgeExecutor spawns tokio tasks for concurrent group execution. Make sure the runtime is available.

3. **Contract Session Keys**: Sessions are keyed by `group_{idx}`. Don't reuse sessions across different groups without careful coordination.

4. **ERC20 Amount Formatting**: The `format_erc20_amount` function in assembler.rs handles decimal precision. Test thoroughly with different decimal values.

### Quick Commands

```bash
# Run all forge_executor tests
cargo test --lib -- forge_executor::test

# Run specific test
cargo test --lib test_set_execution_plan_success_with_serialization

# Run ignored integration test
cargo test --lib test_full_workflow_set_and_execute -- --ignored --nocapture

# Check compilation without running tests
cargo check --lib
```

---

## Design Decisions

### Why DashMap for Contract Sessions?
- Thread-safe without explicit locking on every access
- Allows concurrent group execution to share compiled contracts
- Alternative considered: `Arc<Mutex<HashMap>>` - too coarse-grained locking

### Why Background SourceFetcher?
- Contract fetching is I/O bound and can take seconds
- Starting fetches immediately (in `new()`) reduces wait time in `next_groups()`
- Alternative considered: Fetch on-demand - would block execution

### Why Global Executor Storage?
- Rig Tool trait requires stateless `call()` methods
- Alternative considered: Pass executor through context - not supported by Rig
- Trade-off: Only one executor per process, but acceptable for single-agent use

### Why Two BAML Phases?
- Phase 1 extracts structured contract info (functions, storage, events)
- Phase 2 generates script using extracted info
- Separation improves LLM accuracy and allows caching of extracted info
- Alternative considered: Single-phase generation - less accurate, harder to debug

---

## Reference: Original Design (tmp.txt)

See `aomi/crates/tools/tmp.txt` for the original architecture design and BAML schema definitions.
