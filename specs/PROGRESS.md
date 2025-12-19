# Project Progress: Aomi Anvil Integration

**Branch:** `aomi-anvil`
**Last Updated:** 2025-12-11

---

## Sprint Goal

Integrate `aomi-anvil` crate for programmable fork management and replace all hardcoded RPC URLs with dynamic fork provider endpoints.

**Status:** ✅ Complete

---

## Branch Status

**Current Branch:** `aomi-anvil`

**Recent Commits:**
```
812d406 pick more
cfee501 picked a259650
1b5df14 fix
8c8694a removed irrelavent cherry pick
01b3bb7 fork_endpoint
da23d31 renames
9f681f2 replace script anvil with programable forks
4395aad anvil crates
4c7e6c0 Merge pull request #85 from aomi-labs/feat/eval-part2
2b22223 Exclude eval tests in ci
```

---

## Recently Completed Work

### ✅ Cherry-pick from mono-be-foundry (2025-12-09)
- **Commits cherry-picked:** 34628e9, b32f05c, ed33d83, 6508dc7
- **Key features brought over:**
  - `fork_endpoint` - RPC endpoint for fork management
  - Module renames and reorganization
  - Programmable forks replacing script anvil
  - `aomi-anvil` crate for fork management

### ✅ Compilation Fixes (2025-12-09)
- **Problem:** After cherry-pick, foundry dependency version mismatch caused type errors
- **Root cause:** Foundry deps missing `tag = "v1.5.0"` causing API incompatibilities
- **Fixes applied:**
  - Added `tag = "v1.5.0"` to all foundry dependencies in workspace Cargo.toml
  - Added `crates/forge` to workspace members and copied directory
  - Updated solar patches to `rev = "1f28069"`
  - Added missing workspace dependencies (alloy-primitives, foundry-evm, etc.)
  - Copied missing modules from source: `baml/`, `forge_executor/`, `forge_script_builder.rs`, `contract/`
  - Fixed package naming (`baml-client` → `l2b-baml-client`)
  - Synced divergent source files (clients.rs, tools.rs, db_tools.rs, etc.)

### ✅ Hardcoded RPC URL Replacement (2025-12-11)
Replaced all hardcoded `localhost:8545` / `127.0.0.1:8545` URLs with `aomi_anvil::fork_endpoint()`:

| File | Change |
|------|--------|
| `crates/l2beat/src/runner.rs` | Added `get_rpc_url()` helper, updated 2 test providers |
| `crates/l2beat/src/handlers/call.rs` | Added `get_rpc_url()` helper, updated 2 test providers |
| `crates/l2beat/src/handlers/array.rs` | Added `get_rpc_url()` helper, updated 1 test provider |
| `crates/mcp/src/cast.rs` | Updated `CastTool::new()` to use fork_endpoint() |
| `crates/mcp/src/combined_tool.rs` | Updated fallback testnet URL |
| `crates/l2beat/Cargo.toml` | Added `aomi-anvil.workspace = true` |
| `crates/mcp/Cargo.toml` | Added `aomi-anvil.workspace = true` |

**Pattern used:**
```rust
aomi_anvil::fork_endpoint().unwrap_or_else(|| "http://localhost:8545".to_string())
```

---

## Module Structure

### Core Modules (from cherry-pick)

| Module | Description | Status |
|--------|-------------|--------|
| `aomi-anvil` | Programmable fork management | ✅ Integrated |
| `forge_executor` | ForgeExecutor with dependency-aware execution | ✅ Integrated |
| `baml` | BAML client for LLM code generation | ✅ Integrated |
| `forge_script_builder` | Forge script building utilities | ✅ Integrated |
| `contract` | Contract compilation and session management | ✅ Integrated |

---

## Files Modified This Sprint

### Workspace Configuration
- `aomi/Cargo.toml` - Added workspace members, pinned foundry v1.5.0, updated solar patches
- `aomi/Cargo.lock` - Updated dependency resolution

### Crate Dependencies
- `crates/tools/Cargo.toml` - Added foundry deps, alloy-primitives, baml clients
- `crates/anvil/Cargo.toml` - New crate configuration
- `crates/l2beat/Cargo.toml` - Added aomi-anvil dependency
- `crates/mcp/Cargo.toml` - Added aomi-anvil dependency
- `crates/l2beat/baml_client/Cargo.toml` - Fixed package name to `l2b-baml-client`
- `crates/backend/Cargo.toml` - Updated baml-client reference

### New Directories (copied from source)
- `crates/forge/` - Forge crate
- `crates/tools/src/baml/` - BAML client module
- `crates/tools/src/forge_executor/` - Executor implementation
- `crates/tools/src/contract/` - Contract session management

### Anvil Module
- `crates/anvil/src/instance.rs` - Fork instance management (spawn, kill, RAII)
- `crates/anvil/src/config.rs` - AnvilParams and ForksConfig
- `crates/anvil/src/lib.rs` - Module exports
- `crates/anvil/src/provider.rs` - ForkProvider with global static storage

---

## Build Status

**Compilation:** ✅ Success

**Warnings (non-blocking):**
```
warning: unused import: `tokio::task::block_in_place`
 --> crates/anvil/src/provider.rs:5:5

warning: unused import: `alloy::serde`
 --> crates/tools/src/forge_executor/assembler.rs:2:5
```

---

## Anvil Integration Status

### Shell Scripts
| Script | Anvil Usage | Status |
|--------|-------------|--------|
| `scripts/run-eval-tests.sh` | Sets `ANVIL_FORK_URL`, relies on Rust ForkProvider | ✅ Migrated |
| `scripts/dev.sh` | No anvil start | ✅ Clean |
| `scripts/kill-all.sh` | Kills port 8545 | Still needed for cleanup |

### Rust Code - Hardcoded URLs
| Location | Status |
|----------|--------|
| `crates/tools/src/contract/session.rs` | ✅ Uses `aomi_anvil::fork_snapshot()` |
| `crates/tools/src/clients.rs` | ✅ Uses `aomi_anvil::fork_snapshot()` |
| `crates/eval/src/harness.rs` | ✅ Uses `aomi_anvil::fork_endpoint()` |
| `crates/eval/src/eval_state.rs` | ✅ Uses `aomi_anvil::fork_endpoint()` |
| `crates/l2beat/src/runner.rs` | ✅ Uses `get_rpc_url()` helper |
| `crates/l2beat/src/handlers/call.rs` | ✅ Uses `get_rpc_url()` helper |
| `crates/l2beat/src/handlers/array.rs` | ✅ Uses `get_rpc_url()` helper |
| `crates/mcp/src/cast.rs` | ✅ Uses `aomi_anvil::fork_endpoint()` |
| `crates/mcp/src/combined_tool.rs` | ✅ Uses `aomi_anvil::fork_endpoint()` |

---

## Pending Tasks

### Ready for Next Steps
- [ ] Clean up unused import warnings
- [ ] Run full test suite to verify functionality
- [ ] Test ForgeExecutor with real contract operations

### Medium Priority
- [ ] Review and test fork endpoint functionality
- [ ] Verify BAML integration works end-to-end
- [ ] Consider removing `scripts/kill-all.sh` anvil cleanup (RAII handles it now)

---

## Known Issues

### Resolved
- ✅ Foundry version mismatch causing `MIN_SOLIDITY_VERSION` type error
- ✅ Missing `crates/forge` workspace member
- ✅ Missing baml, forge_executor, forge_script_builder modules
- ✅ Package name mismatch (baml-client vs l2b-baml-client)
- ✅ Missing alloy-primitives workspace dependency
- ✅ Private `get_or_fetch_contract` function
- ✅ Missing `GetErc20Balance` tool definition
- ✅ Hardcoded RPC URLs in l2beat and mcp crates

### Active
- None currently

---

## Notes for Next Agent

### Critical Context

1. **Foundry v1.5.0**: All foundry dependencies MUST use `tag = "v1.5.0"` for API compatibility.

2. **Solar Patches**: Pinned to `rev = "1f28069"`. Don't change without testing.

3. **Package Naming**: `l2b-baml-client` is the l2beat BAML client. `forge-baml-client` is separate.

4. **aomi-anvil API**:
   - `fork_endpoint()` → `Option<String>` - Get current fork RPC URL
   - `init_fork_provider(ForksConfig)` → Auto-spawn anvil if needed
   - `fork_snapshot()` → `Option<ForkSnapshot>` - Get full snapshot with metadata

5. **Fallback Pattern**: Always use fallback for graceful degradation:
   ```rust
   aomi_anvil::fork_endpoint().unwrap_or_else(|| "http://localhost:8545".to_string())
   ```

### Quick Commands

```bash
# Check compilation
cargo check --workspace

# Run tests
cargo test --workspace

# Build release
cargo build --release --workspace
```

---

## Architecture Reference

### aomi-anvil Crate

**AnvilInstance** - RAII wrapper for spawning/killing Anvil processes:
```rust
AnvilInstance::spawn(AnvilParams::default()).await?
// Auto-kills on drop
```

**ForkProvider** - Enum over managed Anvil or external RPC:
```rust
pub enum ForkProvider {
    Anvil(AnvilInstance),
    External { url: String, block_number: u64 },
}
```

**Global API**:
```rust
init_fork_provider(ForksConfig::new()).await?;  // Initialize
fork_endpoint()  // Get endpoint URL
fork_snapshot()  // Get full snapshot
shutdown_all().await?  // Cleanup
```

---

## Previous Sprint Summary (from main branch)

The previous sprint on `main` focused on **Title Generation System Enhancement**:

| Feature | Status |
|---------|--------|
| `is_user_title` flag for user vs auto-generated titles | ✅ Complete |
| Title generation filter (skip user titles) | ✅ Complete |
| Race condition protection | ✅ Complete |
| Anonymous session privacy (no DB writes) | ✅ Complete |
| Integration test suite | ✅ Complete |
| Clippy fixes | ✅ Complete |
| Frontend `/api/updates` SSE integration | ⏳ Pending |

**Key files from that sprint**:
- `crates/backend/src/manager.rs` - Title protection logic
- `crates/backend/tests/title_generation_integration_test.rs` - E2E tests
- `bin/backend/src/endpoint/types.rs` - `is_user_title` in API

**Remaining from that sprint**: Frontend needs to listen to `/api/updates` SSE endpoint for `TitleChanged` events.
