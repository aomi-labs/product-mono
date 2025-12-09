# Project Progress: Aomi Anvil Integration

**Branch:** `aomi-anvil`
**Last Updated:** 2025-12-09

---

## Sprint Goal

Cherry-pick foundry/anvil integration commits from `mono-be-foundry` branch and fix compilation issues to enable:
1. Programmable fork support via aomi-anvil crate
2. ForgeExecutor and BAML integration for LLM-driven Solidity script generation
3. Foundry v1.5.0 toolchain integration

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
- `crates/l2beat/baml_client/Cargo.toml` - Fixed package name to `l2b-baml-client`
- `crates/l2beat/Cargo.toml` - Updated baml-client reference
- `crates/backend/Cargo.toml` - Updated baml-client reference

### New Directories (copied from source)
- `crates/forge/` - Forge crate
- `crates/tools/src/baml/` - BAML client module
- `crates/tools/src/forge_executor/` - Executor implementation
- `crates/tools/src/contract/` - Contract session management

### Source Files Synced
- `crates/tools/src/lib.rs` - Module declarations
- `crates/tools/src/clients.rs` - External clients including baml_client()
- `crates/tools/src/tools.rs` - Tool definitions including GetErc20Balance
- `crates/tools/src/db_tools.rs` - Public get_or_fetch_contract
- `crates/tools/src/etherscan.rs` - Etherscan API client
- `crates/tools/src/forge_script_builder.rs` - Script builder
- `crates/l2beat/src/adapter.rs`, `runner.rs`, `l2b_tools.rs`
- `crates/backend/src/history.rs`
- `crates/chat/src/app.rs`

### Anvil Module
- `crates/anvil/src/instance.rs` - Fork instance management
- `crates/anvil/src/lib.rs` - Module exports
- `crates/anvil/src/provider.rs` - Provider implementation

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

## Pending Tasks

### Ready for Next Steps
- [ ] Commit all changes made during fix process
- [ ] Clean up unused import warnings
- [ ] Run full test suite to verify functionality
- [ ] Update any documentation affected by the integration

### Medium Priority
- [ ] Review and test fork endpoint functionality
- [ ] Verify BAML integration works end-to-end
- [ ] Test ForgeExecutor with real contract operations

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

### Active
- None currently

---

## Notes for Next Agent

### Critical Context

1. **Source Worktree Reference**: The source repository at `/Users/ceciliazhang/Code/aomi-product/aomi` was used to sync files. If issues arise, compare with that directory.

2. **Foundry v1.5.0**: All foundry dependencies MUST use `tag = "v1.5.0"` for API compatibility. The version constraint matters for type compatibility.

3. **Solar Patches**: The solar compiler patches are pinned to `rev = "1f28069"`. Don't change this without testing.

4. **Package Naming**: `l2b-baml-client` is the correct name for the l2beat BAML client package. There's also `forge-baml-client` which is separate.

5. **Cherry-pick Context**: This branch contains cherry-picked commits from `mono-be-foundry`. Some files were manually synced because they had diverged between branches.

### Quick Commands

```bash
# Check compilation
cargo check --workspace

# Run tests
cargo test --workspace

# Build release
cargo build --release --workspace
```

### Dependency Resolution

If you see dependency resolution errors:
1. Check `aomi/Cargo.toml` for workspace dependencies
2. Verify all foundry deps have `tag = "v1.5.0"`
3. Check solar patches have consistent `rev` values
4. Run `cargo update` if needed

---

## Architecture Reference

The integrated code follows a two-phase BAML flow:

**Phase 1: Extract Contract Info**
```
ContractInfo (from DB) + Operations → ExtractedContractInfo
```

**Phase 2: Generate Script**
```
ExtractedContractInfo + Operations → ScriptBlock → Forge Script
```

See the previous `mono-be-foundry` progress notes for detailed architecture documentation.
