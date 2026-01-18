# Chain Manager Specification

## Overview

This spec defines `ChainManager`, a pooled base-chain manager that wraps `foundry_evm::Backend` and handles:

1. **Base chain lifecycle** - spawn and sync base backends per chain
2. **Fork creation** - create forks on a backend (multi-fork supported by Foundry)
3. **Resource pooling** - limit concurrent backends and reuse them
4. **Test-friendly** - auto-spawn local Anvil or cached state for tests

Fork resolution (which forks to use for a node, multi-chain bundles, dependency rules) is owned by the executor layer. `ChainManager` provides the primitives: base backend + fork creation + base syncing.

---

## Background: Foundry Backend Supports Multi-Fork

Foundry's `Backend` already supports multi-fork and can persist across one execution. That means:

- We do **not** need a fork-resolution layer here.
- We should focus on creating and synchronizing base backends and providing fork creation APIs.
- The executor owns graph resolution (depends_on, neighbors) and decides when to reuse a fork.

Reference: `foundry_evm::Backend` multi-fork support in `crates/evm/core/src/backend/mod.rs`.

---

## Current State Analysis

### Existing Architecture (`aomi-anvil`)

```
crates/anvil/src/
├── config.rs      # AnvilParams, ForksConfig
├── instance.rs    # AnvilInstance (spawns anvil process)
├── provider.rs    # ForkProvider, ForkSnapshot, global FORK_PROVIDERS
└── lib.rs
```

**Problems:**
1. **Global singleton** - shared across executions
2. **No base-chain pooling** - backends are not reused or capped
3. **External RPC required** - tests need API keys or local fork
4. **Fork resolution is mixed in** - concerns belong in executor

---

## Phase 1: Chain Data Model

### New Types

```rust
// aomi/crates/anvil/src/chain_manager.rs

pub type ChainId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BackendId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForkId(pub u64);

#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub chain_id: ChainId,
    pub rpc_url: String,
    pub fork_block: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct BackendSnapshot {
    pub backend_id: BackendId,
    pub chain_id: ChainId,
    pub is_spawned: bool,
}

#[derive(Debug, Clone)]
pub struct ForkSnapshot {
    pub backend_id: BackendId,
    pub fork_id: ForkId,
    pub chain_id: ChainId,
}
```

### Core Manager

```rust
pub struct ChainManager {
    /// Backends by ID
    backends: DashMap<BackendId, Arc<RwLock<Backend>>>,

    /// Backends by chain
    backends_by_chain: DashMap<ChainId, Vec<BackendId>>,

    /// Chain RPC config
    chain_configs: DashMap<ChainId, ChainConfig>,

    /// Configuration
    max_backends: usize,
    next_backend_id: AtomicU64,
}

impl ChainManager {
    pub fn new(max_backends: usize) -> Self;

    /// Register chain RPC and fork settings
    pub fn register_chain(&self, config: ChainConfig);

    /// Get or spawn a base backend for a chain
    pub async fn get_backend(&self, chain_id: ChainId) -> Result<BackendSnapshot>;

    /// Sync base backend from upstream (if configured)
    pub async fn sync_backend(&self, backend_id: BackendId) -> Result<()>;

    /// Create a fork on the given backend
    pub async fn create_fork(&self, backend_id: BackendId) -> Result<ForkSnapshot>;

    /// Shutdown all backends
    pub async fn shutdown_all(&self) -> Result<()>;
}
```

### Notes

- `ChainManager` does **not** decide fork reuse. The executor passes a specific `backend_id` (and decides whether to reuse the same fork).
- `foundry_evm::Backend` handles internal multi-fork support; this is exposed via `create_fork`.

---

## Phase 2: Manager Behavior

### Backend Selection

- `get_backend(chain_id)` returns an existing backend for the chain if one is available.
- If none exists and capacity allows, it spawns a new backend.
- If capacity is full, it returns an error (or blocks later via a wait queue if needed).

### Fork Creation

- `create_fork(backend_id)` delegates to `foundry_evm::Backend::create_fork`.
- The fork ID is tracked only for observability/cleanup; the executor is responsible for selecting the fork during execution.

### Base Sync

- `sync_backend` performs a base chain sync using the configured RPC URL or cached state.
- The executor can request explicit sync points (e.g., before a cross-chain bundle).

---

## Phase 3: Test Infrastructure

### Goal: No More Ignored Tests Due to Anvil

Tests should auto-spawn local Anvil instances or reuse cached state for each chain.

```rust
// aomi/crates/anvil/src/test_support.rs

static TEST_CHAIN_MANAGER: OnceCell<Arc<ChainManager>> = OnceCell::const_new();

pub async fn init_test_chain_manager() -> Result<Arc<ChainManager>> {
    TEST_CHAIN_MANAGER
        .get_or_try_init(|| async {
            let manager = ChainManager::new(10);

            for chain in TEST_CHAINS {
                let anvil = spawn_test_anvil(chain).await?;
                manager.register_chain(ChainConfig {
                    chain_id: chain.chain_id,
                    rpc_url: anvil.endpoint().to_string(),
                    fork_block: chain.fork_block,
                });
            }

            Ok(Arc::new(manager))
        })
        .await
        .map(Arc::clone)
}
```

---

## Phase 4: Integration & Migration

### Update ForgeExecutor

```rust
// scripts/src/forge_executor/executor.rs

let backend = chain_manager.get_backend(chain_id).await?;
let fork = chain_manager.create_fork(backend.backend_id).await?;
let contract_config = Self::build_contract_config_from_fork(&fork);
```

### Update ForgeOrchestrator

```rust
// apps/forge/src/orchestrator.rs

let backend = self.chain_manager.get_backend(chain_id).await?;
let fork = self.chain_manager.create_fork(backend.backend_id).await?;

// Resolution (reuse, neighbors, depends_on) stays in executor layer.
```

---

## Phase 5: Codebase Review - Fork Usage Audit

### Files Requiring Updates

| File | Current Usage | New Usage |
|------|---------------|-----------|
| `scripts/src/forge_executor/executor.rs:294-334` | `init_fork_provider()`, global `fork_snapshot()` | `ChainManager::get_backend()` + `create_fork()` |
| `scripts/src/contract/session.rs` | `ContractConfig.evm_opts.fork_url` | `ForkSnapshot` from `ChainManager` |
| `eval/src/harness.rs:141-156` | `init_fork_provider()` + API keys | `ChainManager` with cached state |
| `apps/l2beat/src/handlers/event.rs` | Fork for event discovery | `ChainManager::create_fork()` |

---

## Summary

### Implementation Order

1. **Phase 1**: Chain data model (`ChainManager`, `BackendSnapshot`, `ForkSnapshot`)
2. **Phase 2**: Base backend lifecycle + `create_fork` APIs
3. **Phase 3**: Test infrastructure (auto-spawn Anvil or cached state)
4. **Phase 4**: Migrate executor/orchestrator to use `ChainManager`

### Success Criteria

- `cargo test` runs fork-related tests without manual Anvil startup
- Executor layer handles fork resolution using Foundry multi-fork
- Base backend reuse works correctly under `MAX_BACKENDS`
- Multi-chain operations work (ETH + ARB + OP)
