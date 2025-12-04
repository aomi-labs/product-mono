# Project Progress: ForgeExecutor + BAML Integration

**Branch:** `mono-be-foundry`
**Last Updated:** 2025-12-04

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
e7d08d3 consolidated baml src and refactored forge assembler
7881e39 refined fixure json data
ef0c0da fixtures
1bdc98b unit tests
a05c2a7 new forge_executor
7749463 baml and executor module
c98600a eval forge
6da6fd5 plans + generated .md
4e8cdca Merge feat/eval-part2 into mono-be-foundry
0e1945a fix clippy errors
```

---

## Recently Completed Work

### ✅ BAML Client Consolidation & Code Simplification (Claude)
- **Date:** 2025-12-04
- **Changes:**
  - **BAML Consolidation:** Merged two separate BAML clients (l2beat + tools) into single unified client in tools crate
    - Copied all BAML schema files to `tools/src/baml/baml_src/`
    - Merged `clients.baml` configurations
    - Updated workspace Cargo.toml to point baml-client to tools location
    - Fixed ContractInfo type conflicts and import paths across crates
    - Updated generator version from 0.212.0 to 0.214.0
  - **Assembler Simplification:** Refactored `forge_executor/assembler.rs` using `alloy_primitives::utils::parse_units`
    - Reduced `format_erc20_amount` from 61 lines to 4 lines
    - Reduced `sanitize_eth_amount` from 15 lines to 5 lines
    - Removed `pow10` helper (8 lines)
    - Simplified `add_funding_setup` from 38 lines to 24 lines
    - Total reduction: ~87 lines of manual parsing → 11 lines using library
  - **Code Quality:** Fixed all clippy warnings
    - Added clippy allows for generated BAML client code
    - Fixed redundant closure in baml/client.rs
    - Added Default impl for SourceFetcher
    - Removed needless Ok(...)? wrappers in tools.rs
  - **Test Fixes:** Added conditional skip logic for tests requiring ANTHROPIC_API_KEY
- **Test Results:** All tests pass (132 passed, 0 failed, 17 ignored in CI mode)
- **Files:**
  - BAML: `tools/src/baml/baml_src/*.baml`, `tools/src/baml/baml_client/`
  - Workspace: `Cargo.toml`, `crates/{tools,l2beat,backend}/Cargo.toml`
  - Simplification: `forge_executor/assembler.rs`, `contract/script_assembler.rs`
  - Quality: `baml/client.rs`, `forge_executor/{source_fetcher.rs,tools.rs}`
  - Tests: `forge_executor/executor.rs`, `contract/tests.rs`

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
- `aomi/crates/tools/src/forge_executor/source_fetcher.rs` - Background fetcher (added Default impl)
- `aomi/crates/tools/src/forge_executor/assembler.rs` - Script assembly (simplified)
- `aomi/crates/tools/src/forge_executor/tools.rs` - Rig tool interface (cleaned up)
- `aomi/crates/tools/src/forge_executor/mod.rs` - Module exports

### BAML Client
- `aomi/crates/tools/src/baml/baml_src/*.baml` - Consolidated BAML schemas
- `aomi/crates/tools/src/baml/baml_client/` - Generated Rust client
- `aomi/crates/tools/src/baml/client.rs` - BAML client wrapper (fixed closure)
- `aomi/crates/tools/src/baml/mod.rs` - Module re-exports

### Contract Tools
- `aomi/crates/tools/src/contract/script_assembler.rs` - Script assembler (import format fix)
- `aomi/crates/tools/src/contract/tests.rs` - Contract session tests (removed dead code)

### Tests
- `aomi/crates/tools/src/forge_executor/test.rs` - Unit/integration tests (added skip logic)

### Workspace Configuration
- `aomi/Cargo.toml` - Workspace deps (consolidated baml-client)
- `aomi/crates/tools/Cargo.toml` - Tools crate deps
- `aomi/crates/l2beat/Cargo.toml` - L2beat crate deps (uses workspace baml-client)
- `aomi/crates/backend/Cargo.toml` - Backend crate deps (uses workspace baml-client)

---

## Pending Tasks

### High Priority
- [ ] Add timeout to `ForgeExecutor::next_groups()` contract fetching wait loop (line 81 in executor.rs)
- [ ] Implement remaining unit tests identified in session:
  - `plan.rs`: dependency validation, circular dependency detection
  - `source_fetcher.rs`: timeout handling, error propagation
  - `assembler.rs`: funding requirement edge cases (already have basic tests)
  - `executor.rs`: concurrent execution, error handling

### Medium Priority
- [ ] Document BAML schema files and consolidation architecture
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
- ✅ Duplicate BAML clients in l2beat and tools → consolidated to tools crate
- ✅ Manual amount parsing in assembler → using alloy_primitives::utils::parse_units
- ✅ Clippy warnings in generated and hand-written code → all fixed
- ✅ Tests failing without API keys in CI → added skip logic

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
- [x] Test skip logic for CI compatibility
- [ ] Integration tests with fixtures
- [ ] Full workflow test with real contracts
- [ ] Error handling tests
- [ ] Dependency graph edge case tests

### Phase 5: Code Quality & Consolidation ✅
- [x] Consolidate duplicate BAML clients
- [x] Simplify amount parsing with library functions
- [x] Fix all clippy warnings
- [x] Add Default implementations where appropriate
- [x] Remove dead code

### Phase 6: Production Readiness (Not Started) ⚪
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

1. **BAML Consolidation**: Single unified BAML client in `tools/src/baml/`. All crates (tools, l2beat, backend) use workspace dependency. Never create separate BAML clients.

2. **BAML Two-Phase Flow**: Always maintain the extract → generate separation. Phase 1 extracts contract structure, Phase 2 generates script code.

3. **Dependency Ordering**: The ExecutionPlan ensures groups are executed in dependency order. Never execute a group before its dependencies complete.

4. **Background Fetching**: SourceFetcher runs in a separate task and uses mpsc::channel for async communication. It must be properly shut down.

5. **Global Executor Storage**: The EXECUTOR static uses once_cell::Lazy + Arc<Mutex<Option<ForgeExecutor>>>. Only one executor can be active at a time.

6. **Transaction Format**: Backend doesn't broadcast transactions - it formats them for wallet signing. The `rpc_url` field tells the wallet which network to use.

7. **Amount Parsing**: Always use `alloy_primitives::utils::parse_units` for ETH/ERC20 amounts. Never implement manual parsing.

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

4. **Amount Parsing**: Use `alloy_primitives::utils::parse_units(amount, decimals)` for all ETH/ERC20 amounts. Returns `U256` in base units.

5. **BAML Generation**: Run `npx @boundaryml/baml generate` from `tools/src/baml` after schema changes. Generated code goes in `baml_client/`.

6. **Clippy Allows**: Generated BAML client has `#![allow(clippy::needless_return)]` and `#![allow(clippy::empty_docs)]` in `lib.rs`. Don't remove these.

7. **CI Test Compatibility**: Tests requiring `ANTHROPIC_API_KEY` must have skip logic using `skip_without_anthropic_api_key()` helper.

### Quick Commands

```bash
# Run all workspace tests (CI mode, no env vars)
cargo test --workspace

# Run all workspace tests with API keys
ANTHROPIC_API_KEY=sk-... cargo test --workspace

# Run all forge_executor tests
cargo test --lib -- forge_executor::test

# Run specific test
cargo test --lib test_set_execution_plan_success_with_serialization

# Run ignored integration tests
cargo test --lib -- --ignored --nocapture

# Check clippy for all crates
cargo clippy --workspace --all-targets -- -D warnings

# Generate BAML client after schema changes
cd crates/tools/src/baml && npx @boundaryml/baml generate

# Check compilation without running tests
cargo check --lib
```

---

## Design Decisions

### Why Consolidate BAML Clients?
- **Decision:** Single unified BAML client in tools crate, shared via workspace dependency
- **Reasoning:** Eliminates duplication, ensures consistency, simplifies maintenance
- **Alternative considered:** Keep separate clients per crate - leads to drift and version mismatches
- **Date:** 2025-12-04

### Why Use alloy_primitives for Amount Parsing?
- **Decision:** Use `parse_units(amount, decimals)` from alloy_primitives for all token amounts
- **Reasoning:** Battle-tested library, handles edge cases, reduces ~87 lines of manual parsing
- **Alternative considered:** Manual parsing with U256 - error-prone, hard to maintain
- **Date:** 2025-12-04

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
