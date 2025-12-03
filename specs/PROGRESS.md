# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**Foundry Integration & Eval Testing** — Merge improved eval harness and add integration tests for Forge script generation.

---

## Branch Status

Current branch: `mono-be-foundry` (base: `main`)

**Recent Commits** (last 10):
```
c98600a eval forge
6da6fd5 plans + generated .md
4e8cdca Merge feat/eval-part2 into mono-be-foundry
0e1945a fix clippy errors
a640d7e removed old tool and added implemented new design
733de73 registered tool and updated forge script
58511ff added tool and tests for multi-step intents
c8367aa Ignored clippy warning for generated baml client
7446110 fixed forge script and added unit test
02323db refactored and added tests
```

---

## Recently Completed Work

### Eval Framework Merge (4e8cdca)
| Change | Description |
|--------|-------------|
| **Merged feat/eval-part2** | Clean merge with no conflicts |
| **EvalCase** | Builder pattern for test cases with expectations and assertions |
| **assertions.rs** | Deterministic on-chain assertion framework (772 lines) |
| **Updated Harness** | Now accepts any `Arc<BackendwithTool>`, not just ChatApp |
| **USDC prefunding** | Whale impersonation for test account funding |
| **Improved test suite** | Basic, DeFi, Lido, Aave test categories |

### Scripter Integration Testing
| Change | Description |
|--------|-------------|
| **Harness::for_scripter()** | Helper to create harness with ForgeApp backend (harness.rs:347-367) |
| **ForgeApp::into_chat_app()** | Extracts inner ChatApp for use as BackendwithTool (app.rs:100-103) |
| **TestResult helpers** | `has_tool_call()` and `get_tool_calls()` for verification (lib.rs:75-94) |
| **test_scripter.rs** | Two test cases for ETH transfer and ERC20 approval scripts |

### Forge Crate (`aomi/crates/forge`)
| Change | Description |
|--------|-------------|
| **ForgeApp** | Chat app specialized for Foundry/Forge operations |
| **Forge preamble** | Custom system prompt for smart contract deployment workflows |
| **Tool integration** | `ForgeScriptBuilder` tool registered |

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

---

## Files Modified This Sprint

### Eval Crate
| File | Description |
|------|-------------|
| `crates/eval/Cargo.toml` | Added `aomi-forge` dependency |
| `crates/eval/src/assertions.rs` | New - Deterministic assertion framework |
| `crates/eval/src/harness.rs` | Added `for_scripter()` helper function |
| `crates/eval/src/lib.rs` | Added `has_tool_call()` and `get_tool_calls()`, registered test_scripter module |
| `crates/eval/src/test_scripter.rs` | New - Integration tests for ForgeScriptBuilder |
| `crates/eval/src/eval_app.rs` | Updated from merge |
| `crates/eval/src/eval_state.rs` | Updated from merge |
| `crates/eval/src/test_entry.rs` | Updated from merge |

### Forge Crate
| File | Description |
|------|-------------|
| `crates/forge/Cargo.toml` | Crate dependencies |
| `crates/forge/src/lib.rs` | Library exports |
| `crates/forge/src/app.rs` | Added `into_chat_app()` method |

### Tools Crate
| File | Changes |
|------|---------|
| `crates/tools/src/forge_script_builder.rs` | Forge script generation tool |
| `crates/tools/src/contract/mod.rs` | Contract module |
| `crates/tools/src/contract/compiler.rs` | Solidity compiler integration |
| `crates/tools/src/contract/runner.rs` | Script runner |
| `crates/tools/src/contract/session.rs` | Contract session management |
| `crates/tools/src/contract/tests.rs` | Multi-step intent tests |
| `crates/tools/src/scheduler.rs` | Updated from merge |
| `crates/tools/src/lib.rs` | Updated exports |

### Specs
| File | Description |
|------|-------------|
| `specs/PLAN-SCRIPTER-INTEGRATION.md` | Detailed integration testing plan |
| `specs/PROGRESS-mono-be-foundry.md` | Archived previous progress |

### Backend & Chat Crates
| File | Changes |
|------|---------|
| `crates/chat/src/app.rs` | Updated from merge |
| Various backend files | Updated from merge |

---

## Pending Tasks

### Immediate Priority

1. **Run scripter integration tests**:
   - Start BAML server: `cd aomi/crates/l2beat && docker-compose up -d baml`
   - Run tests: `ANTHROPIC_API_KEY=<key> cargo test --package eval --features eval-test test_scripter -- --ignored --nocapture`
   - Verify both ETH transfer and ERC20 approval tests pass

2. **Validate compilation across all crates**:
   - Run `cargo check` on all packages
   - Verify no clippy warnings

3. **PR preparation**:
   - Test all changes locally
   - Update ROADMAP.md if needed
   - Prepare PR description summarizing foundry integration + eval improvements

### Short-Term

4. **Additional scripter test cases**:
   - Contract deployment test
   - Multi-step operations (deploy + interact)
   - Error handling scenarios

5. **Frontend integration**:
   - Add Forge mode to UI
   - Handle script preview and approval flow

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Scripter tests require BAML server | By design | Tests skip gracefully if unavailable |
| ANTHROPIC_API_KEY required for eval tests | By design | Tests marked with `#[ignore]` |
| Foundry must be installed locally | Documented | Required for script compilation |

---

## Multi-Step Flow State

Current Position: Near Completion

| Step | Description | Status |
|------|-------------|--------|
| 1 | Add foundry libs to repo | ✓ Done |
| 2 | Create contract module | ✓ Done |
| 3 | Implement ForgeScriptBuilder tool | ✓ Done |
| 4 | Add multi-step intent support | ✓ Done |
| 5 | Register tool and update forge script | ✓ Done |
| 6 | Fix clippy errors | ✓ Done |
| 7 | Merge feat/eval-part2 | ✓ Done |
| 8 | Add eval integration tests | ✓ Done |
| 9 | Run integration tests | Pending |
| 10 | PR and merge | Pending |

---

## Notes for Next Agent

### Critical Context

1. **Eval framework improvements**
   - `feat/eval-part2` merged cleanly with no conflicts
   - `Harness` now accepts any backend via `Arc<BackendwithTool>`
   - `EvalCase` provides builder pattern for test cases with assertions
   - USDC prefunding happens automatically via whale impersonation

2. **Scripter testing architecture**
   - `Harness::for_scripter()` creates harness with ForgeApp backend
   - Unlike default harness, skips USDC prefunding (scripts are simulated)
   - `TestResult::has_tool_call()` verifies tool was invoked
   - Tests in `test_scripter.rs` validate script generation

3. **Foundry dependency**
   - Requires Foundry installed locally for script execution
   - Uses `forge-std` for Script base class
   - Foundry libs added as submodule in `tools/src/contract/lib/forge-std/`

4. **BAML server dependency**
   - Transaction call generation requires BAML server
   - Default URL: `http://localhost:2024`
   - Configure via `BAML_API_URL` env var
   - Start with: `cd aomi/crates/l2beat && docker-compose up -d baml`

5. **Test execution**
   - Scripter tests: `cargo test --package eval --features eval-test test_scripter -- --ignored --nocapture`
   - Requires: `ANTHROPIC_API_KEY` env var
   - Tests skip gracefully if BAML server unavailable

### Key Files
```
aomi/crates/forge/src/app.rs                    # ForgeApp entry point
aomi/crates/tools/src/forge_script_builder.rs   # Main tool implementation
aomi/crates/tools/src/contract/                 # Contract interaction module
aomi/crates/eval/src/harness.rs                 # Test harness with for_scripter()
aomi/crates/eval/src/test_scripter.rs           # Integration tests
specs/PLAN-SCRIPTER-INTEGRATION.md              # Detailed testing plan
```

### Quick Start Commands
```bash
# Start BAML server (in separate terminal)
cd aomi/crates/l2beat && docker-compose up -d baml

# Run forge unit tests
cargo test --package aomi-tools

# Run scripter integration tests
ANTHROPIC_API_KEY=<key> cargo test --package eval --features eval-test test_scripter -- --ignored --nocapture

# Check compilation
cargo check --all

# Run clippy
cargo clippy --all
```

### Integration Test Details

**Test 1: ETH Transfer Script**
- Intent: "Create a script to send 1 ETH to 0x1234..."
- Verifies: ForgeScriptBuilder called, script generated correctly

**Test 2: ERC20 Approval Script**
- Intent: "Create a script to approve 1000 USDC for Uniswap V2 router"
- Verifies: ForgeScriptBuilder called with approve operation, IERC20 interface included

Both tests use `Harness::for_scripter()` which:
1. Creates ForgeApp with `skip_docs=true, skip_mcp=true`
2. Extracts inner ChatApp via `into_chat_app()`
3. Passes it as `Arc<BackendwithTool>` to Harness
4. Runs eval loop with max 3-4 rounds
5. Verifies expectations via LLM judge
