# ForkProvider Integration Plan

> Replace hardcoded Anvil URLs and shell script management with the `aomi-anvil` crate's `ForkProvider` abstraction.

## Overview

The `aomi-anvil` crate provides:
- `AnvilInstance`: RAII wrapper for spawning/killing Anvil processes (with explicit shutdown)
- `ForkProvider`: Enum over managed Anvil or external RPC
- `ForksProviderConfig`: Configure multiple fork instances (optional snapshot load/dump)
- Global static API via a shared handle (Arc + RwLock<Vec<ForkProvider>>), not a bare `OnceCell`, so tests can reset/reseed between cases.

This eliminates:
- Shell scripts for starting/stopping Anvil
- Hardcoded `http://127.0.0.1:8545` URLs
- Manual process lifecycle management
- Proxy environment interference issues
- CI stalls from refetching mainnet state when a snapshot is available

## API Summary

```rust
// Single fork (common case). Can optionally load a snapshot and expose a reset for tests.
init_fork_provider(ForksProviderConfig::new().with_snapshot("block-23946169.json")).await?;
let endpoint = fork_endpoint();  // &'static str

// Reset/reseed for tests (clears the Arc<RwLock> and re-inits with the same config)
reset_fork_providers().await?;

// Multiple forks
init_fork_providers(ForksProviderConfig::multiple(vec![
    ForkConfig::mainnet_fork("https://..."),
    ForkConfig::new().with_chain_id(42161),
])).await?;
fork_endpoint_at(0)  // Ethereum
fork_endpoint_at(1)  // Arbitrum

// Accessors
fork_provider()           // &'static ForkProvider
fork_providers()          // &'static Vec<ForkProvider>
try_fork_provider()       // Option<&'static ForkProvider>
num_fork_providers()      // usize
```

## Current Hardcoded Locations

### High Priority (Core)

| File | Current Code | Impact |
|------|--------------|--------|
| `crates/tools/src/contract/session.rs:94-98` | `ETH_RPC_URL` env → `fork_url` | Core EVM backend |
| `crates/tools/src/contract/runner.rs:334` | `set_var("ETH_RPC_URL", "http://127.0.0.1:8545/")` | Test setup (must set env from provider endpoint) |
| `crates/tools/src/clients.rs:1` | `const DEFAULT_RPC_URL = "http://127.0.0.1:8545"` | RPC client |
| `crates/eval/src/harness.rs` | `const ANVIL_RPC_URL = "http://127.0.0.1:8545"` | Eval harness |
| `crates/eval/src/eval_state.rs` | `const ANVIL_RPC_URL = "http://127.0.0.1:8545"` | Eval state |

### Medium Priority (Features)

| File | Current Code |
|------|--------------|
| `crates/tools/src/forge_executor/executor.rs` | `rpc_url: "http://localhost:8545"` |
| `crates/l2beat/src/runner.rs` | `"http://localhost:8545"` (2 places) |
| `crates/l2beat/src/handlers/array.rs` | `"http://localhost:8545"` |
| `crates/l2beat/src/handlers/call.rs` | `"http://localhost:8545"` (2 places) |
| `crates/l2beat/src/l2b_tools.rs` | `ETH_RPC_URL` env var |
| `crates/l2beat/src/handlers/event.rs` | `ETH_RPC_URL` env var |

### Low Priority (MCP)

| File | Current Code |
|------|--------------|
| `crates/mcp/src/cast.rs` | `anvil_url = "http://127.0.0.1:8545"` |
| `crates/mcp/src/combined_tool.rs` | `"http://127.0.0.1:8545"` fallback |

### Shell Scripts to Replace

| Script | Purpose | Replacement |
|--------|---------|-------------|
| `scripts/run-eval-tests.sh` | Start Anvil for eval tests | Use `aomi-anvil` init in test harness (no shell Anvil) |
| `scripts/kill-all.sh` | Kill Anvil on port 8545 | RAII shutdown in `AnvilInstance` |
| `crates/tools/src/forge_executor/tests/test-executor.sh` | Anvil + BAML + fixtures | Rust harness init with snapshot load; BAML start remains but should read provider endpoint |

## Integration Phases

### Phase 1: Core Infrastructure

**Goal:** Establish pattern in `ContractSession` with snapshot-aware provider and resettable handle

**Files:**
- `crates/tools/Cargo.toml`
- `crates/tools/src/contract/session.rs`
- `crates/tools/src/contract/runner.rs`

```rust
// session.rs
use aomi_anvil::try_fork_provider;

impl Default for ContractConfig {
    fn default() -> Self {
        let fork_url = if let Some(provider) = try_fork_provider() {
            Some(provider.endpoint().to_string())
        } else {
            std::env::var("ETH_RPC_URL").ok()
        };
        // ...
    }
}

// runner.rs tests
use aomi_anvil::{init_fork_provider, reset_fork_providers, ForksProviderConfig};

async fn build_runner() -> ContractRunner {
    let _ = reset_fork_providers().await; // ensure clean state per test
    let _ = init_fork_provider(
        ForksProviderConfig::new().with_snapshot("block-23946169.json")
    ).await;
    // expose endpoint via env for legacy code:
    std::env::set_var("ETH_RPC_URL", aomi_anvil::fork_endpoint());
    // ...
}
```

### Phase 2: Eval Harness

**Goal:** Replace `run-eval-tests.sh` Anvil management

**Files:**
- `crates/eval/Cargo.toml`
- `crates/eval/src/harness.rs`
- `crates/eval/src/eval_state.rs`

```rust
use aomi_anvil::fork_endpoint;

fn default_networks() -> String {
    format!(r#"{{"ethereum":"{}"}}"#, fork_endpoint())
}
```

### Phase 3: ForgeExecutor

**Goal:** Replace `test-executor.sh`; load optional snapshot and provide test reset

**Files:**
- `crates/tools/src/forge_executor/executor.rs`
- `crates/tools/src/forge_executor/tests/`

```rust
use aomi_anvil::{fork_endpoint, reset_fork_providers, init_fork_provider, ForksProviderConfig};

async fn setup_fork_for_tests() {
    let _ = reset_fork_providers().await;
    let _ = init_fork_provider(
        ForksProviderConfig::new()
            .with_snapshot(std::env::var("ANVIL_SNAPSHOT").ok())
    ).await;
}

TransactionData {
    rpc_url: fork_endpoint().to_string(),
}
```

### Phase 4: L2Beat

**Goal:** Replace hardcoded URLs in handlers

**Files:**
- `crates/l2beat/Cargo.toml`
- `crates/l2beat/src/runner.rs`
- `crates/l2beat/src/handlers/*.rs`

### Phase 5: MCP

**Goal:** Replace hardcoded URLs

**Files:**
- `crates/mcp/Cargo.toml`
- `crates/mcp/src/cast.rs`
- `crates/mcp/src/combined_tool.rs`

### Phase 6: Cleanup

- Simplify `scripts/run-eval-tests.sh` (remove Anvil, keep BAML)
- Update `scripts/kill-all.sh` (remove Anvil killing)
- Deprecate `test-executor.sh`

## Usage Patterns

### Application Startup

```rust
use aomi_anvil::{init_fork_provider, ForksProviderConfig, ForkConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_fork_provider(
        ForksProviderConfig::single(
            ForkConfig::new()
                .with_fork_url(std::env::var("ANVIL_FORK_URL").ok())
                .with_snapshot(std::env::var("ANVIL_SNAPSHOT").ok())
        )
    ).await?;

    std::env::set_var("ETH_RPC_URL", aomi_anvil::fork_endpoint());
    run_app().await
}
```

### Test Setup

```rust
#[tokio::test]
async fn test_something() {
    let _ = aomi_anvil::reset_fork_providers().await;
    let _ = aomi_anvil::init_fork_provider(
        ForksProviderConfig::new().with_snapshot("block-23946169.json")
    ).await;
    std::env::set_var("ETH_RPC_URL", aomi_anvil::fork_endpoint());
    // ...
}
```

### Multiple Forks

```rust
init_fork_providers(
    ForksProviderConfig::multiple(vec![
        ForkConfig::mainnet_fork("https://eth-mainnet.g.alchemy.com/v2/KEY"),
        ForkConfig::new().with_chain_id(42161).with_fork_url("https://arb-mainnet..."),
    ])
).await?;

let eth = fork_endpoint_at(0).unwrap();
let arb = fork_endpoint_at(1).unwrap();
```

## Migration Checklist

### Phase 1: Core
- [ ] Add `aomi-anvil` to `crates/tools/Cargo.toml`
- [ ] Update `contract/session.rs`
- [ ] Update `contract/runner.rs` tests
- [ ] Update `clients.rs`
- [ ] Verify `cargo test -p aomi-tools`

### Phase 2: Eval
- [ ] Add `aomi-anvil` to `crates/eval/Cargo.toml`
- [ ] Update `harness.rs`
- [ ] Update `eval_state.rs`
- [ ] Verify `cargo test -p eval`

### Phase 3: ForgeExecutor
- [ ] Update `forge_executor/executor.rs`
- [ ] Create Rust test harness
- [ ] Verify tests pass

### Phase 4: L2Beat
- [ ] Add `aomi-anvil` to `crates/l2beat/Cargo.toml`
- [ ] Update handlers
- [ ] Verify tests pass

### Phase 5: MCP
- [ ] Add `aomi-anvil` to `crates/mcp/Cargo.toml`
- [ ] Update `cast.rs` and `combined_tool.rs`

### Phase 6: Cleanup
- [ ] Simplify shell scripts
- [ ] Update docs

## Benefits

1. **RAII cleanup** - Anvil dies when process exits
2. **Random port** - No conflicts (auto-allocated)
3. **Proxy bypass** - Removes env vars that cause deadlock
4. **Lazy init** - Anvil only starts when needed
5. **Shared instance** - Multiple modules use same Anvil
6. **Multiple forks** - Support for multi-chain testing

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Breaking workflows | Phase-by-phase, keep env fallback |
| Anvil not installed | Clear error with install URL |
| Port conflicts | Random port allocation |
| Tests holding Anvil | Arc<RwLock> + `reset_fork_providers` to reseed per test; RAII shutdown on drop |
| Slow forks / hangs | Prefer snapshot load (config + env), avoid refetching mainnet during CI |
