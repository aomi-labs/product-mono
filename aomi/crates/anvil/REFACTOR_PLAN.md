# ProviderManager Refactoring Plan (v2)

## Overview
Refactor `aomi/crates/anvil` to introduce a `ProviderManager` that:
- Manages all chain forks from a `providers.toml` config file
- Provides UUID-based instance tracking and profiling
- Supports dual-mode access: RootProvider (for RPC) and Backend (for EVM execution)
- Supports multi-fork Backends for cross-chain execution

**Note**: No backward compatibility required - full refactor of all downstream consumers.

## Current Architecture

### Existing Components
1. **AnvilInstance** (`instance.rs`)
   - Manages anvil process lifecycle
   - Stores: endpoint, port, chain_id, block_number
   - Methods: spawn(), kill(), is_running()

2. **ChainProvider** (`provider.rs`)
   - Enum: `Anvil(AnvilInstance)` | `External { url, block_number }`
   - Global state: `CHAIN_PROVIDERS` (Lazy<RwLock<Option<Vec<ChainProvider>>>>)
   - Functions: init_fork_providers(), fork_snapshots(), shutdown_all()

### Downstream Usage Patterns

**Pattern I: External RPC → RootProvider**
```rust
let rpc = std::env::var("LEGACY_RPC_URL")?;
let rpc_url = rpc.parse()?;
let provider = RootProvider::<AnyNetwork>::new_http(rpc_url);
```

**Pattern II: EVM Config → Backend (Single Fork)**
```rust
let mut evm_opts = config.evm_opts.clone();
let env = evm_opts.evm_env().await?;
let fork = evm_opts.get_fork(&config.foundry_config, env.clone());
let backend = Backend::spawn(fork);
```

**Pattern III: Multi-Fork Backend**
```rust
let mut backend = Backend::spawn(Some(first_fork))?;
let fork_id_2 = backend.create_fork(second_fork)?;
let fork_id_3 = backend.create_fork(third_fork)?;
// Can switch between forks with backend.select_fork(fork_id)
```

## Desired Architecture

### New Components

#### 1. providers.toml Configuration
```toml
# Managed Anvil instances - will be auto-spawned
[anvil-instances]
ethereum = { chain_id = 1, fork_url = "https://eth-mainnet.g.alchemy.com/v2/{ALCHEMY_API_KEY}", fork_block_number = 23456, port = 8545 }
optimism = { chain_id = 10, fork_url = "https://mainnet.optimism.io", fork_block_number = 12345 }
arbitrum = { chain_id = 42161, fork_url = "https://arb1.arbitrum.io/rpc" }

# External RPC endpoints - no anvil process, just connect
[external]
base = { chain_id = 8453, rpc_url = "https://mainnet.base.org" }
polygon = { chain_id = 137, rpc_url = "https://polygon-rpc.com" }
```

**Config Structure**:
```rust
#[derive(Deserialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub anvil_instances: HashMap<String, AnvilInstanceConfig>,
    #[serde(default)]
    pub external: HashMap<String, ExternalConfig>,
}

#[derive(Deserialize)]
pub struct AnvilInstanceConfig {
    pub chain_id: u64,
    pub fork_url: String,  // Supports {ENV_VAR} substitution
    pub fork_block_number: Option<u64>,
    #[serde(default)]
    pub port: u16,  // 0 = auto-assign
    #[serde(default)]
    pub block_time: Option<u64>,
    #[serde(default)]
    pub accounts: u32,
    // ... other AnvilParams fields
}

#[derive(Deserialize)]
pub struct ExternalConfig {
    pub chain_id: u64,
    pub rpc_url: String,  // Supports {ENV_VAR} substitution
}
```

#### 2. ManagedInstance
```rust
pub struct ManagedInstance {
    // Identity & Metadata
    id: Uuid,
    name: String,  // from config key: "ethereum", "optimism", etc.
    chain_id: u64,
    block_number: u64,  // Fetched on initialization

    // Source
    source: InstanceSource,

    // Cached provider (lazy-loaded, shared)
    provider: OnceCell<Arc<RootProvider<AnyNetwork>>>,

    // Profiling
    created_at: Instant,
    metrics: InstanceMetrics,
}

enum InstanceSource {
    Anvil(AnvilInstance),
    External(String),  // RPC URL
}

struct InstanceMetrics {
    provider_requests: AtomicU64,
    backend_requests: AtomicU64,
    last_provider_access: RwLock<Option<Instant>>,
    last_backend_access: RwLock<Option<Instant>>,
}
```

#### 3. ProviderManager
```rust
pub struct ProviderManager {
    instances: RwLock<HashMap<Uuid, ManagedInstance>>,
    name_to_id: RwLock<HashMap<String, Uuid>>,  // name -> uuid lookup
}

impl ProviderManager {
    // Initialization from config
    pub fn new() -> Self;
    pub async fn from_config(config: ProvidersConfig) -> Result<Self>;
    pub async fn from_config_file(path: impl AsRef<Path>) -> Result<Self>;

    // Lifecycle Management
    pub async fn spawn_anvil(&self, name: String, config: AnvilInstanceConfig) -> Result<Uuid>;
    pub async fn register_external(&self, name: String, config: ExternalConfig) -> Result<Uuid>;
    pub async fn shutdown_instance(&self, id: Uuid) -> Result<()>;
    pub async fn shutdown_all(&self) -> Result<()>;

    // Provider Access (Pattern I)
    pub async fn get_provider(
        &self,
        chain_id: Option<u64>,
        block_number: Option<u64>
    ) -> Option<Arc<RootProvider<AnyNetwork>>>;

    pub async fn get_provider_by_name(&self, name: &str) -> Option<Arc<RootProvider<AnyNetwork>>>;

    // Backend Access (Pattern II & III - Multi-fork)
    pub async fn get_backend(
        &self,
        forks: Vec<ForkQuery>
    ) -> Result<Backend>;

    // Instance Queries
    pub fn get_instance(&self, id: Uuid) -> Option<&ManagedInstance>;
    pub fn get_instance_by_name(&self, name: &str) -> Option<&ManagedInstance>;
    pub fn list_instances(&self) -> Vec<InstanceInfo>;

    // Profiling
    pub fn get_metrics(&self, id: Uuid) -> Option<InstanceMetrics>;
    pub fn get_all_metrics(&self) -> HashMap<Uuid, InstanceMetrics>;
}

pub struct ForkQuery {
    pub chain_id: Option<u64>,
    pub block_number: Option<u64>,
}

pub struct InstanceInfo {
    pub id: Uuid,
    pub name: String,
    pub chain_id: u64,
    pub block_number: u64,
    pub is_managed: bool,  // anvil vs external
    pub endpoint: String,
}
```

### Query & Backend Creation Logic

**get_provider() Filtering**:
1. If both `chain_id` and `block_number` are None → return first available
2. If only `chain_id` is Some → filter by exact chain_id match
3. If only `block_number` is Some → filter by exact block_number match
4. If both are Some → filter by both (exact match)
5. Return None if no match found

**get_backend() Multi-Fork Creation**:
```rust
pub async fn get_backend(&self, forks: Vec<ForkQuery>) -> Result<Backend> {
    if forks.is_empty() {
        bail!("At least one fork query required");
    }

    // Find all matching instances
    let mut matched_instances = Vec::new();
    for query in forks {
        let instance = self.find_instance(query.chain_id, query.block_number)
            .ok_or_else(|| anyhow!("No instance matching query"))?;
        matched_instances.push(instance);
    }

    // Create Backend with first fork
    let first = &matched_instances[0];
    let first_fork = self.create_fork_config(first).await?;
    let mut backend = Backend::spawn(Some(first_fork))?;

    // Add remaining forks
    for instance in matched_instances.iter().skip(1) {
        let fork_config = self.create_fork_config(instance).await?;
        backend.create_fork(fork_config)?;
    }

    Ok(backend)
}

async fn create_fork_config(&self, instance: &ManagedInstance) -> Result<CreateFork> {
    // Always create fresh EvmOpts from instance metadata
    let evm_opts = EvmOpts::default();
    let env = evm_opts.evm_env().await?;

    Ok(CreateFork {
        enable_caching: true,
        url: instance.endpoint().to_string(),
        env,
        evm_opts: EvmOpts {
            fork_url: Some(instance.endpoint().to_string()),
            fork_block_number: Some(instance.block_number),
            ..Default::default()
        },
    })
}
```

**Key Point**: No BackendConfig storage needed. Backend is created fresh from instance metadata each time.

## Implementation Steps

### Phase 1: Dependencies & Config Infrastructure
1. **Update `Cargo.toml`**:
   ```toml
   [dependencies]
   uuid = { version = "1.0", features = ["v4", "serde"] }
   toml = "0.8"
   alloy-provider = { version = "0.7", features = ["reqwest"] }
   alloy-transport-http = "0.7"
   foundry-evm = { git = "https://github.com/foundry-rs/foundry", features = ["rustls"] }
   foundry-config = { git = "https://github.com/foundry-rs/foundry" }
   # ... existing deps
   ```

2. **Create `config.rs` module** (extend existing):
   - Add `ProvidersConfig`, `AnvilInstanceConfig`, `ExternalConfig`
   - Implement TOML deserialization
   - Implement env var substitution (e.g., `{ALCHEMY_API_KEY}`)
   - Add validation (e.g., unique chain_ids, valid URLs)

3. **Create example `providers.toml`**:
   - Document all available options
   - Include comments explaining each field
   - Provide sensible defaults

### Phase 2: Core Manager Infrastructure
4. **Create `manager.rs` module**:
   - Define `ManagedInstance` struct with UUID, name, chain_id, block_number
   - Define `InstanceSource` enum (Anvil | External)
   - Define `InstanceMetrics` struct with AtomicU64 counters
   - Define `ForkQuery` struct
   - Define `InstanceInfo` struct (read-only view)

5. **Implement `ProviderManager` struct**:
   - Storage: `instances: RwLock<HashMap<Uuid, ManagedInstance>>`
   - Storage: `name_to_id: RwLock<HashMap<String, Uuid>>`
   - Method: `new()` - empty manager
   - Method: `from_config()` - spawn all instances from config
   - Method: `from_config_file()` - load config from path
   - Method: `spawn_anvil()` - add single anvil instance
   - Method: `register_external()` - add single external endpoint
   - Method: `shutdown_instance()` - kill anvil or disconnect external
   - Method: `shutdown_all()` - cleanup all instances

### Phase 3: Provider Access Implementation
6. **Implement `get_provider()`**:
   - Filter instances by chain_id and/or block_number
   - Lazy-load `RootProvider` using `OnceCell`
   - Cache provider in `Arc<RootProvider<AnyNetwork>>`
   - Update metrics (provider_requests, last_provider_access)
   - Return cloned Arc

7. **Implement `get_provider_by_name()`**:
   - Look up UUID by name
   - Delegate to provider loading logic
   - Update metrics

### Phase 4: Backend Access Implementation
8. **Implement `get_backend()` with multi-fork support**:
   - Accept `Vec<ForkQuery>` - each query filters for one fork
   - For each query, find matching instance
   - Create `CreateFork` from first instance → `Backend::spawn(Some(first_fork))`
   - For remaining instances, call `backend.create_fork()`
   - Update metrics for all used instances
   - Return Backend (user owns it, no caching)

9. **Implement `create_fork_config()`** helper:
   - Use `EvmOpts::default()` as base
   - Populate fork_url and fork_block_number from instance
   - Create `CreateFork` with enable_caching=true
   - No persistent storage of this config

### Phase 5: Query & Profiling
10. **Implement query methods**:
    - `find_instance(chain_id, block_number)` - internal filter logic
    - `get_instance(id)` - lookup by UUID
    - `get_instance_by_name(name)` - lookup by name
    - `list_instances()` - return Vec<InstanceInfo>

11. **Implement profiling methods**:
    - `get_metrics(id)` - clone metrics for one instance
    - `get_all_metrics()` - HashMap of all metrics
    - Internal helper to update metrics on access

### Phase 6: Full Refactor (No Backward Compatibility)
12. **Delete old `provider.rs`** - no wrapper needed

13. **Update `lib.rs` exports**:
    ```rust
    mod manager;
    pub use manager::{
        ProviderManager, ForkQuery, InstanceInfo, InstanceMetrics,
    };

    // Keep config, instance as-is
    pub use config::{AnvilParams, ProvidersConfig, AnvilInstanceConfig, ExternalConfig};
    pub use instance::AnvilInstance;
    ```

14. **Refactor all downstream consumers**:
    - `scripts/src/contract/runner.rs` - use ProviderManager for Backend
    - `apps/l2beat/src/runner.rs` - use ProviderManager for RootProvider
    - Remove all `std::env::var("LEGACY_RPC_URL")` calls
    - Use config-driven approach everywhere

### Phase 7: Testing & Documentation
15. **Unit tests** (`manager.rs`):
    - Test config loading and env var substitution
    - Test instance spawning and registration
    - Test provider lazy-loading and caching
    - Test backend creation with single and multiple forks
    - Test filtering logic (by chain_id, block_number, both, neither)
    - Test metrics tracking

16. **Integration tests**:
    - End-to-end: Load providers.toml → spawn anvil → get provider → make RPC call
    - End-to-end: Load providers.toml → get multi-fork backend → execute transaction
    - Test shutdown and cleanup

17. **Documentation**:
    - Document `providers.toml` format with examples
    - Add rustdoc to all public APIs
    - Create migration guide for downstream consumers
    - Add examples in `examples/` directory

## Key Design Decisions

### 1. Config-Driven Architecture
- **All chain forks managed via `providers.toml`** instead of env vars
- Supports env var substitution in config (e.g., `{ALCHEMY_API_KEY}`)
- Clear separation: anvil-instances (managed) vs external (unmanaged)
- Named instances for easier debugging ("ethereum", "optimism", etc.)

### 2. Provider vs Backend Ownership
- **Provider**: Cached in `Arc<RootProvider>` - thread-safe, can be cloned and shared
- **Backend**: Created fresh each time - NOT Send/Sync, represents mutable EVM state
- **No BackendConfig storage**: Always create from fresh `EvmOpts::default()` + instance metadata

### 3. Multi-Fork Backend Support
- `get_backend()` accepts `Vec<ForkQuery>` - each query finds one fork
- First fork used for `Backend::spawn(Some(first))`
- Remaining forks added via `backend.create_fork()`
- User can switch between forks using `backend.select_fork(fork_id)`

### 4. Lazy Loading & Caching
- **Providers**: Lazy-loaded on first request, cached in `OnceCell<Arc<RootProvider>>`
- **Backends**: Created fresh every time (no caching)
- Avoids unnecessary HTTP connections for unused instances

### 5. UUID-Based Instance Identity
- Each instance has a UUID for:
  - Stable identity (future persistence support)
  - Better profiling and debugging
  - Name-based lookup also supported
- Trade-off: 16 bytes vs index-based lookup

### 6. Filter Logic
When filtering by `chain_id` and/or `block_number`:
- Both None → return first available instance
- Only chain_id → exact match on chain_id
- Only block_number → exact match on block_number
- Both → exact match on both
- No match → return None

### 7. No Backward Compatibility
- Complete refactor of all downstream code
- Remove env var-based configuration
- Standardize on config-driven approach
- Cleaner, more maintainable codebase

## Testing Strategy

1. **Config Tests**:
   - TOML parsing with all field variations
   - Env var substitution (e.g., `{API_KEY}` → actual value)
   - Validation (duplicate chain_ids, invalid URLs)

2. **Unit Tests** (`manager.rs`):
   - Instance spawning (anvil + external)
   - Provider lazy-loading and caching
   - Backend creation (single and multi-fork)
   - Filter logic (all combinations)
   - Metrics tracking and updates
   - Name-based and UUID-based lookups

3. **Integration Tests**:
   - Load providers.toml → spawn all instances
   - Get provider by chain_id → make RPC call
   - Get multi-fork backend → execute cross-chain transaction
   - Shutdown and cleanup verification

4. **Example Usage**:
   - Simple: Single provider from config
   - Advanced: Multi-fork backend with 3 chains
   - Migration: Before/after refactor comparison

## Migration Guide for Downstream Consumers

### Before (Old Approach)
```rust
// Pattern I: External RPC
let rpc = std::env::var("LEGACY_RPC_URL")?;
let provider = RootProvider::<AnyNetwork>::new_http(rpc.parse()?);

// Pattern II: Backend
let mut evm_opts = config.evm_opts.clone();
let env = evm_opts.evm_env().await?;
let fork = evm_opts.get_fork(&config.foundry_config, env.clone());
let backend = Backend::spawn(fork);
```

### After (New Approach)
```rust
use aomi_anvil::{ProviderManager, ForkQuery};

// Load from config once (at app startup)
let manager = ProviderManager::from_config_file("providers.toml").await?;

// Pattern I: Get provider by chain_id
let eth_provider = manager.get_provider(Some(1), None).await
    .ok_or_else(|| anyhow!("Ethereum provider not found"))?;

// Pattern II: Get single-fork backend
let backend = manager.get_backend(vec![
    ForkQuery { chain_id: Some(1), block_number: None }
]).await?;

// Pattern III: Get multi-fork backend
let multi_backend = manager.get_backend(vec![
    ForkQuery { chain_id: Some(1), block_number: None },    // Ethereum
    ForkQuery { chain_id: Some(10), block_number: None },   // Optimism
    ForkQuery { chain_id: Some(42161), block_number: None } // Arbitrum
]).await?;

// Or by name
let eth_provider = manager.get_provider_by_name("ethereum").await.unwrap();
```

### Files to Update
1. **Remove**: All `std::env::var("LEGACY_RPC_URL")` calls
2. **Update**: `scripts/src/contract/runner.rs` - use ProviderManager
3. **Update**: `apps/l2beat/src/runner.rs` - use ProviderManager
4. **Update**: All test files using providers
5. **Create**: `providers.toml` in project root

## Open Questions & Resolutions

1. **Backend caching**: Should we cache Backend?
   - ❌ **Decision**: No, Backend is not Send/Sync, users own it

2. **Multi-fork Backend**: How to support multiple forks?
   - ✅ **Decision**: `get_backend(Vec<ForkQuery>)` - create multi-fork Backend directly

3. **BackendConfig**: Should we store config for Backend creation?
   - ❌ **Decision**: No, always create fresh from `EvmOpts::default()` + instance metadata

4. **Backward compatibility**: Maintain old API?
   - ❌ **Decision**: No, full refactor of all downstream code

5. **Provider network type**: Generic `N: Network` or `AnyNetwork`?
   - ✅ **Decision**: Start with `AnyNetwork`, sufficient for current use cases

6. **Config location**: Where should `providers.toml` live?
   - ✅ **Decision**: Project root, load via `from_config_file("providers.toml")`

## Updates (Ownership, Metrics, Determinism)

### ManagedInstance Ownership
- **Change**: Store instances as `Arc<ManagedInstance>` inside the manager.
- **Reason**: Avoid returning references tied to `RwLock` lifetimes; callers get stable handles.
- **Implementation**:
  ```rust
  pub struct ProviderManager {
      instances: RwLock<HashMap<Uuid, Arc<ManagedInstance>>>,
      name_to_id: RwLock<HashMap<String, Uuid>>,
  }
  ```

### Instance Accessor Signatures
- **Change**: Return `Arc<ManagedInstance>` instead of `&ManagedInstance`.
- **Updated API**:
  ```rust
  pub fn get_instance(&self, id: Uuid) -> Option<Arc<ManagedInstance>>;
  pub fn get_instance_by_name(&self, name: &str) -> Option<Arc<ManagedInstance>>;
  ```

### InstanceInfo Snapshot
- **Change**: `list_instances()` returns `Vec<InstanceInfo>` snapshots (not references).
- **Determinism**: Sort snapshots by `name` (or `created_at`) to avoid nondeterministic
  `HashMap` iteration.

### Metrics Snapshot Type
- **Change**: Replace raw `InstanceMetrics` returns with `InstanceMetricsSnapshot`.
- **Reason**: `AtomicU64` and `RwLock<Option<Instant>>` are not clonable.
- **Updated API**:
  ```rust
  pub fn get_metrics(&self, id: Uuid) -> Option<InstanceMetricsSnapshot>;
  pub fn get_all_metrics(&self) -> HashMap<Uuid, InstanceMetricsSnapshot>;
  ```
- **Snapshot**:
  ```rust
  pub struct InstanceMetricsSnapshot {
      pub provider_requests: u64,
      pub backend_requests: u64,
      pub last_provider_access: Option<Instant>,
      pub last_backend_access: Option<Instant>,
  }
  ```

### Provider Selection Semantics
- **Change**: When filters are None or multiple matches occur, choose deterministically.
- **Rule**: Sort by `name` (or `created_at`) and return the first match.

## Success Criteria

- ✅ Can load `providers.toml` with anvil-instances and external endpoints
- ✅ Can spawn anvil instances from config
- ✅ Can retrieve `RootProvider` by chain_id or name
- ✅ Can create single-fork `Backend` from instance
- ✅ Can create multi-fork `Backend` with multiple instances
- ✅ UUID-based instance tracking with metrics
- ✅ All downstream consumers refactored (no env vars)
- ✅ Comprehensive tests for all functionality
- ✅ Documentation and examples
