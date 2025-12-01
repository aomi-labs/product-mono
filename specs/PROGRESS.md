# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**Foundry Integration & Multi-Step Intents** — Add Forge script building capability for smart contract deployment and interaction via LLM agent.

---

## Branch Status

Current branch: `mono-be-foundry` (base: `main`)

**Recent Commits** (last 10):
```
0e1945a fix clippy errors
a640d7e removed old tool and added implemented new design
733de73 registered tool and updated forge script
58511ff added tool and tests for multi-step intents
c8367aa Ignored clippy warning for generated baml client
7446110 fixed forge script and added unit test
02323db refactored and added tests
078b16e added foundry libs to repo and contract module
c55ab26 Merge pull request #74 from aomi-labs/api-key-in-ci
a754dca Merge pull request #78 from aomi-labs/system-response-redo
```

---

## Recently Completed Work

### Forge Crate (`aomi/crates/forge`)
| Change | Description |
|--------|-------------|
| **ForgeApp** | New chat app specialized for Foundry/Forge operations |
| **Forge preamble** | Custom system prompt for smart contract deployment workflows |
| **Tool integration** | Added `ForgeScriptBuilder` tool for script generation |

### ForgeScriptBuilder Tool (`aomi/crates/tools/src/forge_script_builder.rs`)
| Change | Description |
|--------|-------------|
| **Script assembly** | Generate complete Forge scripts from structured operations |
| **Funding requirements** | Support for ETH and ERC20 funding configuration |
| **Interface handling** | Inline interface definitions and imports |
| **BAML integration** | Uses BAML client for transaction call generation |

### Contract Module (`aomi/crates/tools/src/contract/`)
| Change | Description |
|--------|-------------|
| **ContractSession** | Session management for contract interactions |
| **Compiler** | Solidity compilation support |
| **Runner** | Script execution and simulation |
| **Tests** | Unit tests for multi-step intents |

### Multi-Step Intent Support
- Added tool and tests for multi-step intents
- New design for operation structuring
- Registered tool in forge script builder

---

## Files Modified This Sprint

### New Crate: Forge
| File | Description |
|------|-------------|
| `crates/forge/Cargo.toml` | New crate dependencies |
| `crates/forge/src/lib.rs` | Library exports |
| `crates/forge/src/app.rs` | ForgeApp implementation |

### Tools Crate
| File | Changes |
|------|---------|
| `crates/tools/src/forge_script_builder.rs` | New - Forge script generation tool |
| `crates/tools/src/contract/mod.rs` | New - Contract module |
| `crates/tools/src/contract/compiler.rs` | New - Solidity compiler integration |
| `crates/tools/src/contract/runner.rs` | New - Script runner |
| `crates/tools/src/contract/session.rs` | New - Contract session management |
| `crates/tools/src/contract/tests.rs` | New - Multi-step intent tests |
| `crates/tools/src/lib.rs` | Updated exports |

### Backend Integration
| File | Changes |
|------|---------|
| `bin/backend/src/endpoint.rs` | Updated for forge integration |
| `bin/backend/src/main.rs` | Updated app initialization |
| `bin/cli/src/main.rs` | CLI updates |
| `crates/backend/src/manager.rs` | Session manager updates |
| `crates/backend/src/session.rs` | Session handling |

### BAML Client
| File | Changes |
|------|---------|
| `crates/l2beat/baml_client/` | Generated client for transaction calls |

---

## Pending Tasks

### Immediate Priority

1. **End-to-end Forge script test**:
   - Create script from user intent → verify generation
   - Simulate script execution
   - Verify transaction output

2. **PR preparation**:
   - Ensure all tests pass
   - Run clippy and fmt
   - Update documentation if needed

### Short-Term

3. **Frontend integration**
   - Add Forge mode to UI
   - Handle script preview and approval flow

4. **Additional operation types**
   - Support more complex DeFi operations
   - Multi-contract deployment flows

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Clippy warnings in generated BAML client | Resolved | Added `#[ignore]` attribute |
| Foundry libs as git submodule | Working | Added to .gitmodules |

---

## Multi-Step Flow State

Current Position: In Progress

| Step | Description | Status |
|------|-------------|--------|
| 1 | Add foundry libs to repo | ✓ Done |
| 2 | Create contract module | ✓ Done |
| 3 | Implement ForgeScriptBuilder tool | ✓ Done |
| 4 | Add multi-step intent support | ✓ Done |
| 5 | Register tool and update forge script | ✓ Done |
| 6 | Fix clippy errors | ✓ Done |
| 7 | Integration testing | Pending |
| 8 | PR and merge | Pending |

---

## Notes for Next Agent

### Critical Context

1. **Foundry dependency**
   - Requires Foundry installed locally for script execution
   - Uses `forge-std` for Script base class
   - Foundry libs added as submodule

2. **BAML server dependency**
   - Transaction call generation requires BAML server
   - Default URL: `http://localhost:2024`
   - Configure via `BAML_SERVER_URL` env var

3. **Tool architecture**
   - `ForgeScriptBuilder` is a rig tool that wraps script assembly
   - Uses `ContractSession` for state management
   - Scripts are assembled from `GeneratedScript` responses

### Key Files
```
aomi/crates/forge/src/app.rs          # ForgeApp entry point
aomi/crates/tools/src/forge_script_builder.rs  # Main tool implementation
aomi/crates/tools/src/contract/       # Contract interaction module
```

### Quick Start Commands
```bash
# Start backend (from aomi/)
cargo run --bin backend

# Run tests
cargo test --package aomi-tools

# Check compilation
cargo check

# Run clippy
cargo clippy
```
