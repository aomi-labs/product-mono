# Executor v2.0 Refactor Plan

## Overview

Refactor the forge executor from monolithic, single-shot execution to a lightweight, tree-based, concurrent execution model with agentic sub-nodes that can iterate on code generation, compilation, and execution.

### Goals

1. **State separation** - Clear separation of immutable config, shared resources, and per-node mutable state
2. **Concurrent execution** - Multiple groups run in parallel when dependencies allow
3. **Agentic iteration** - Sub-agents can retry compilation/execution, edit generated code
4. **Async results** - Non-blocking tool calls with async result dispatch via channels
5. **Scalability** - Support multiple concurrent plans (multi-user/multi-session)
6. **Fault isolation** - Failures contained to individual nodes, don't corrupt global state

---

## Current State Analysis

### Problems

| Issue | Location | Impact |
|-------|----------|--------|
| `ContractRunner` owns `Executor` | `contract/runner.rs:46` | Can't share fork state across groups |
| `ContractSession` owns `Runner` | `contract/session.rs:114` | Each group creates new fork |
| Monolithic `execute_single_group()` | `forge_executor/executor.rs:99` | No retry/iteration capability |
| Shared session hack | `forge_executor/executor.rs:554-571` | Fragile, doesn't scale |
| Static plan consumption | `forge_executor/plan.rs:42-57` | Can't add groups dynamically |
| Results only return after all ready groups finish | `forge/tools.rs:125-131` | No streaming/early results per group |

### Current Call Stack

```
SetExecutionPlan(groups)
  → ForgeExecutor::new_with_resources()
  → ExecutionPlan::from(groups)  ← static DAG
  → source_fetcher.request_fetch()

NextGroups(plan_id)
  → ForgeExecutor::next_groups()
  → plan.next_ready_batch()  ← topological sort
  → spawn_group_tasks()
      → tokio::spawn(execute_single_group)  ← monolithic, no retry
          → run_baml_extract()
          → run_baml_generate_script()
          → assemble_script()
          → compile_and_deploy_script()  ← if fails, return Failed
          → execute_run()  ← if fails, return Failed
          → build_transactions()
  → collect_group_results()
  → return Vec<GroupResult>  ← blocking
```

---

## Target Architecture

### Component Hierarchy

```
┌────────────────────────────────────────────────────────────────┐
│                      ForgeOrchestrator                         │
│  (manages plans, spawns nodes, dispatches async results)      │
└────────────────────────────────────────────────────────────────┘
                            │
            ┌───────────────┼──────────────────────────────┐
            │               │               │              │
     ┌──────▼──────┐  ┌────▼─────┐   ┌─────▼──────┐  ┌─────▼─────────────────┐
     │ ExecutionPlan│  │GroupNode │  │GroupNode   │  |    ExecutionSink      |
     │ (lightweight)│  │(stateful)│  │(stateful)  │  | (loop to send result  |
     └──────────────┘  └──────────┘  └────────────┘  | to async tool channel)|
                            │                        └───────────────────────┘
                    ┌───────┼────────┐
                    │       │        │
            ┌───────▼──┐ ┌──▼──────┐ ┌──▼─────────┐
            │ScriptApp │ │Artifacts│ │Config(Arc) │
            │(sub-agent│ │(mutable)│ │(shared)    │
            │iterator) │ └─────────┘ └────────────┘
            └──────────┘      │
                    │         │
                    └────┬────┘
                         │
            ┌────────────▼────────────┐
            │   ExecutionBackend      │
            │   (shared, mutex-locked)│
            │   - Backend             │
            │   - per-chain journals  │
            │   - per-chain envs      │
            │   - per-chain inspectors│
            └─────────────────────────┘
```

---

## Ownership & Sharing Mental Model

### Compile-Time (Immutable Setup)

**`GroupConfig`** - OWNED by each node (NOT Arc, NOT shared)
- `foundry_config: Config` - Per-node compiler settings
- `evm_opts: EvmOpts` - COPIED from `ExecutionBackend.evm_opts` per target chain
- **Why owned**: Compiler needs isolated, mutable workspace per node

**`EvmOpts`** - Per-chain immutable config (read-only HashMap in backend)
- Stored in `ExecutionBackend.evm_opts: Arc<HashMap<ChainId, EvmOpts>>`
- Nodes copy the relevant chain's `EvmOpts` into their `GroupConfig`
- **Why immutable**: Fork configuration doesn't change during execution

### Runtime (Mutable Execution State)

**`Env`** - Per-chain runtime EVM environment (mutable, shared via DashMap)
- Stored in `ExecutionBackend.envs: Arc<DashMap<ChainId, Env>>`
- Contains `TxEnv`, `BlockEnv` - runtime transaction/block state
- Nodes request: `backend.envs.get_mut(chain_id)` to access, clone, and write back
- **Why mutable**: Transaction state changes during execution

**`JournaledState`** - Per-chain EVM state journal (mutable, shared via DashMap)
- Stored in `ExecutionBackend.journals: Arc<DashMap<ChainId, JournaledState>>`
- Tracks state changes (storage, balances, nonces)
- **Why mutable**: EVM execution modifies blockchain state

### Orchestration Model

```
1 plan → 1 backend → [node1, node2, ...]
                   → [chain1, chain2, ...]

Multiple plans → [backend1, backend2, ...]
```

**Key insights**:
- Node can execute on multiple chains (just requests chain env when ready)
- Backend manages all chains for one plan
- Orchestrator manages multiple plans (each with its own backend)

### Resource Sharing Table

| Resource | Scope | Owner | Access Pattern |
|----------|-------|-------|----------------|
| `GroupConfig` | Per-node | `GroupNode` | Owned (NOT Arc) |
| `GroupArtifacts` | Per-node | `GroupNode` | Owned (mutable) |
| `EvmOpts` | Per-chain | `ExecutionBackend` | Read-only (Arc<HashMap>) |
| `Env` | Per-chain | `ExecutionBackend` | Mutable (DashMap) |
| `JournaledState` | Per-chain | `ExecutionBackend` | Mutable (DashMap) |
| `Backend` | Per-plan | `ExecutionBackend` | Mutex (single fork switch) |
| `ExecutionBackend` | Per-plan | `ForgeOrchestrator` | Arc (shared across nodes) |
| `SourceFetcher` | Global | `ForgeOrchestrator` | Arc (shared across plans) |
| `BamlClient` | Global | `ForgeOrchestrator` | Arc (shared across plans) |

### Data Flow

1. **Plan creation**:
   ```
   create_plan(groups)
     → collect target_chains
     → create ExecutionBackend with forks for all chains
     → pre-create GroupNodes (each gets owned GroupConfig)
     → store in execution_trees
   ```

2. **Node execution**:
   ```
   next_groups(execution_id)
     → pull nodes from execution_trees (move ownership)
     → spawn node.run()

   node.run()
     → compile with self.config (owned)
     → execute via self.backend.execute_on_chain(chain_id, |evm| ...)
         → backend locks, switches fork, builds EVM with chain's Env
         → closure runs, modifies JournaledState
         → backend writes back updated state
   ```

---

## Data Structures

### 1. GroupConfig (Per-Node, Owned)

```rust
/// Per-node compilation configuration (NOT shared)
pub struct GroupConfig {
    /// Foundry project configuration (NOT Arc - owned by each node)
    pub foundry_config: foundry_config::Config,

    /// EVM runtime options (copied from backend per target chain)
    pub evm_opts: EvmOpts,

    /// Compiler: disable solc auto-detection
    pub no_auto_detect: bool,

    /// Session identifier
    pub id: Option<String>,
}

impl GroupConfig {
    /// Create from ContractConfig
    pub fn from_contract_config(config: &ContractConfig) -> Self {
        Self {
            foundry_config: (*config.foundry_config).clone(),  // unwrap Arc and clone
            evm_opts: config.evm_opts.clone(),
            no_auto_detect: config.no_auto_detect,
            id: config.id.clone(),
        }
    }

    /// Create with target chain's EvmOpts from backend
    pub fn for_chain(
        foundry_config: foundry_config::Config,
        backend: &ExecutionBackend,
        chain_id: ChainId,
    ) -> Result<Self> {
        let evm_opts = backend.get_evm_opts(chain_id)?;
        Ok(Self {
            foundry_config,
            evm_opts,
            no_auto_detect: false,
            id: None,
        })
    }
}
```

**Location**: `aomi/crates/scripts/src/executor_v2/config.rs`

**Usage**: Each `GroupNode` owns its own `GroupConfig` (NOT shared, NOT Arc)

**Rationale**: Compiler needs isolated config per node. EvmOpts is copied from backend's immutable per-chain config.

---

### 2. GroupArtifacts (Mutable, Per-Node)

```rust
/// Per-group compilation cache and deployment registry
#[derive(Default)]
pub struct GroupArtifacts {
    /// Compilation cache: compilation_name -> compiled output
    compilations: HashMap<String, ProjectCompileOutput>,

    /// Deployment registry: "compilation:contract" -> address
    deployments: HashMap<String, Address>,
}

impl GroupArtifacts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_compilation(&mut self, name: String, output: ProjectCompileOutput) {
        self.compilations.insert(name, output);
    }

    pub fn get_compilation(&self, name: &str) -> Option<&ProjectCompileOutput> {
        self.compilations.get(name)
    }

    pub fn insert_deployment(&mut self, key: String, address: Address) {
        self.deployments.insert(key, address);
    }

    pub fn get_deployment(&self, key: &str) -> Option<Address> {
        self.deployments.get(key).copied()
    }

    pub fn all_compilations(&self) -> &HashMap<String, ProjectCompileOutput> {
        &self.compilations
    }

    pub fn all_deployments(&self) -> &HashMap<String, Address> {
        &self.deployments
    }
}
```

**Location**: `aomi/crates/scripts/src/executor_v2/artifacts.rs`

**Usage**: Owned by each `GroupNode`, replaces `ContractSession.compiled_contracts` and `deployed_contracts`

---

### 3. ExecutionBackend (Per-Plan, Shared Across Nodes)

```rust
use foundry_evm::{backend::Backend, revm::{JournaledState, primitives::Env}};
use foundry_evm::inspectors::InspectorStack;
use dashmap::DashMap;

pub type ChainId = u64;

/// Per-plan execution backend managing multi-chain fork state
/// One backend instance per execution plan
pub struct ExecutionBackend {
    /// Shared backend (manages all forks for this plan)
    backend: Arc<Mutex<Backend>>,

    /// Per-chain IMMUTABLE configs (read-only, nodes copy from here)
    evm_opts: Arc<HashMap<ChainId, EvmOpts>>,

    /// Per-chain MUTABLE state (DashMap for high-performance concurrent access)
    journals: Arc<DashMap<ChainId, JournaledState>>,
    envs: Arc<DashMap<ChainId, Env>>,
    inspectors: Arc<DashMap<ChainId, InspectorStack>>,

    /// Fork ID mapping
    fork_ids: Arc<DashMap<ChainId, foundry_evm::backend::LocalForkId>>,
}

impl ExecutionBackend {
    /// Create new execution backend and initialize forks for all target chains
    pub async fn new(
        target_chains: &HashSet<ChainId>,
        base_foundry_config: &foundry_config::Config,
    ) -> Result<Self> {
        let mut backend = Backend::spawn(None);
        let fork_ids = DashMap::new();
        let journals = DashMap::new();
        let envs = DashMap::new();
        let inspectors = DashMap::new();
        let mut evm_opts_map = HashMap::new();

        for &chain_id in target_chains {
            // Create EvmOpts for this chain
            let mut evm_opts = EvmOpts::default();
            evm_opts.fork_url = Some(Self::get_rpc_url_for_chain(chain_id)?);
            evm_opts_map.insert(chain_id, evm_opts.clone());

            // Create fork
            let env = evm_opts.evm_env().await?;
            let fork = evm_opts.get_fork(base_foundry_config, env.clone());
            let fork_id = backend.insert_fork(fork);
            fork_ids.insert(chain_id, fork_id);

            // Initialize per-chain state
            let spec_id = base_foundry_config.evm_spec_id();
            journals.insert(chain_id, JournaledState::new(spec_id, vec![]));
            envs.insert(chain_id, env);
            inspectors.insert(chain_id, InspectorStack::new(/* config */));
        }

        Ok(Self {
            backend: Arc::new(Mutex::new(backend)),
            evm_opts: Arc::new(evm_opts_map),
            journals: Arc::new(journals),
            envs: Arc::new(envs),
            inspectors: Arc::new(inspectors),
            fork_ids: Arc::new(fork_ids),
        })
    }

    /// Get immutable EvmOpts for a chain (nodes copy from this)
    pub fn get_evm_opts(&self, chain_id: ChainId) -> Result<EvmOpts> {
        self.evm_opts
            .get(&chain_id)
            .cloned()
            .ok_or_else(|| anyhow!("No EvmOpts for chain {}", chain_id))
    }

    fn get_rpc_url_for_chain(chain_id: ChainId) -> Result<String> {
        // TODO: map chain_id to RPC URL
        match chain_id {
            1 => Ok("https://eth.llamarpc.com".to_string()),
            10 => Ok("https://optimism.llamarpc.com".to_string()),
            _ => Err(anyhow!("Unknown chain_id: {}", chain_id)),
        }
    }

    /// Execute closure on specific chain, building EVM on-demand
    /// Node calls this to get runtime Env and execute on forked chain
    pub async fn execute_on_chain<F, R>(&self, chain_id: ChainId, f: F) -> Result<R>
    where
        F: FnOnce(&mut Evm<'_, InspectorStack, &mut Backend>) -> Result<R>,
    {
        // Lock backend (only one execution at a time)
        let mut backend = self.backend.lock().await;

        // Get fork ID
        let fork_id = self.fork_ids
            .get(&chain_id)
            .ok_or_else(|| anyhow!("No fork for chain {}", chain_id))?
            .clone();

        // Get chain-specific mutable state from DashMap
        let mut journal_guard = self.journals
            .get_mut(&chain_id)
            .ok_or_else(|| anyhow!("No journal for chain {}", chain_id))?;
        let mut env_guard = self.envs
            .get_mut(&chain_id)
            .ok_or_else(|| anyhow!("No env for chain {}", chain_id))?;
        let inspector_guard = self.inspectors
            .get(&chain_id)
            .ok_or_else(|| anyhow!("No inspector for chain {}", chain_id))?;

        // Switch backend to target fork (MUST pass env and journal)
        backend.select_fork(
            fork_id,
            &mut env_guard.as_env_mut(),
            journal_guard.value_mut()
        )?;

        // Clone env and inspector for EVM building
        let env = env_guard.clone();
        let inspector = inspector_guard.clone();

        // Build EVM manually (no Executor wrapper)
        let mut evm = Evm::builder()
            .with_db(&mut *backend)
            .with_env(Box::new(env))
            .with_external_context(inspector)
            .append_handler_register(inspector::inspector_handle_register)
            .build();

        // Execute closure
        let result = f(&mut evm)?;

        // State changes are committed during transact()
        // No manual writeback needed

        Ok(result)
    }

    /// Deploy contract on specific chain
    pub async fn deploy(
        &self,
        chain_id: ChainId,
        sender: Address,
        bytecode: Bytes,
        value: U256,
    ) -> Result<(Address, ExecutionResult)> {
        self.execute_on_chain(chain_id, |evm| {
            // Set transaction parameters
            evm.env_mut().tx.caller = sender;
            evm.env_mut().tx.transact_to = TxKind::Create;
            evm.env_mut().tx.data = bytecode;
            evm.env_mut().tx.value = value;
            evm.env_mut().tx.gas_limit = 30_000_000;

            // Execute deployment
            let ResultAndState { result, state } = evm.transact()?;

            // Extract deployed address
            let address = match result {
                ExecutionResult::Success { output, .. } => {
                    match output {
                        Output::Create(_, Some(addr)) => addr,
                        _ => anyhow::bail!("No address in deploy result"),
                    }
                }
                ExecutionResult::Revert { output, .. } => {
                    anyhow::bail!("Deploy reverted: {:?}", output)
                }
                ExecutionResult::Halt { reason, .. } => {
                    anyhow::bail!("Deploy halted: {:?}", reason)
                }
            };

            Ok((address, result))
        }).await
    }

    /// Call contract on specific chain
    pub async fn call(
        &self,
        chain_id: ChainId,
        sender: Address,
        to: Address,
        calldata: Bytes,
        value: U256,
    ) -> Result<ExecutionResult> {
        self.execute_on_chain(chain_id, |evm| {
            // Set transaction parameters
            evm.env_mut().tx.caller = sender;
            evm.env_mut().tx.transact_to = TxKind::Call(to);
            evm.env_mut().tx.data = calldata;
            evm.env_mut().tx.value = value;
            evm.env_mut().tx.gas_limit = 30_000_000;

            // Execute call
            let ResultAndState { result, state } = evm.transact()?;

            Ok(result)
        }).await
    }
}
```

**Location**: `aomi/crates/scripts/src/executor_v2/backend.rs`

**Usage**: Single instance shared across all `GroupNode` instances via `Arc`

---

### 4. GroupNode (Stateful, Per-Group)

```rust
use super::{GroupArtifacts, GroupConfig, ExecutionBackend};
use crate::forge_executor::{OperationGroup, GroupResult, GroupResultInner};

pub struct GroupNode {
    /// Group identifier
    pub group_id: String,

    /// The operation group being executed (JSON strings of to-do list)
    pub group: OperationGroup,

    /// This group's compiled contracts and deployments
    pub artifacts: GroupArtifacts,

    /// OWNED config (NOT shared, NOT Arc)
    config: GroupConfig,

    /// SHARED backend reference (per-plan)
    backend: Arc<ExecutionBackend>,

    /// SHARED global resources
    source_fetcher: Arc<SourceFetcher>,
    baml_client: Arc<aomi_baml::BamlClient>,
}

impl GroupNode {
    pub fn new(
        group_id: String,
        group: OperationGroup,
        config: GroupConfig,  // OWNED, not Arc
        backend: Arc<ExecutionBackend>,
        source_fetcher: Arc<SourceFetcher>,
        baml_client: Arc<aomi_baml::BamlClient>,
    ) -> Self {
        Self {
            group_id,
            group,
            artifacts: GroupArtifacts::new(),
            config,
            backend,
            source_fetcher,
            baml_client,
        }
    }

    /// Main execution loop - runs to completion or failure
    pub async fn run(mut self) -> GroupResult {
        let group_idx = self.group_id.parse::<usize>().unwrap_or(0);

        // 1. Fetch contract sources
        let sources = match self.fetch_sources().await {
            Ok(s) => s,
            Err(e) => return self.build_failed_result(e.to_string(), vec![]),
        };

        // 2. BAML extract
        let extracted_infos = match self.run_baml_extract(&sources).await {
            Ok(infos) => infos,
            Err(e) => return self.build_failed_result(e.to_string(), vec![]),
        };

        // 3. BAML generate script
        let script_block = match self.run_baml_generate_script(&extracted_infos).await {
            Ok(block) => block,
            Err(e) => return self.build_failed_result(e.to_string(), vec![]),
        };

        // 4. Assemble Solidity script
        let generated_code = match self.assemble_script(&script_block) {
            Ok(code) => code,
            Err(e) => return self.build_failed_result(e.to_string(), vec![]),
        };

        // 5. Compile (with retry capability for agentic iteration)
        let script_address = match self.compile_and_deploy(&generated_code).await {
            Ok(addr) => addr,
            Err(e) => return self.build_failed_result(e.to_string(), generated_code, vec![]),
        };

        // 6. Execute run()
        let execution_result = match self.execute_run(script_address).await {
            Ok(result) => result,
            Err(e) => return self.build_failed_result(e.to_string(), generated_code, vec![]),
        };

        // 7. Build transactions
        let transactions = self.build_transactions(&execution_result);

        if !execution_result.success {
            return self.build_failed_result(
                "Script execution failed".to_string(),
                generated_code,
                transactions,
            );
        }

        GroupResult {
            group_index: group_idx,
            description: self.group.description.clone(),
            operations: self.group.operations.clone(),
            inner: GroupResultInner::Done {
                transactions,
                generated_code,
            },
        }
    }

    async fn fetch_sources(&self) -> Result<Vec<aomi_baml::ContractSource>> {
        self.source_fetcher.get_contracts_for_group(&self.group).await
    }

    async fn run_baml_extract(
        &self,
        sources: &[aomi_baml::ContractSource],
    ) -> Result<Vec<aomi_baml::ExtractedContractInfo>> {
        self.baml_client
            .extract_contract_info(&self.group.operations, sources)
            .await
    }

    async fn run_baml_generate_script(
        &self,
        extracted_infos: &[aomi_baml::ExtractedContractInfo],
    ) -> Result<aomi_baml::ScriptBlock> {
        self.baml_client
            .generate_script(&self.group.operations, extracted_infos)
            .await
    }

    fn assemble_script(&self, script_block: &aomi_baml::ScriptBlock) -> Result<String> {
        use crate::forge_executor::assembler::{AssemblyConfig, ScriptAssembler};
        let config = AssemblyConfig::default();
        ScriptAssembler::assemble(vec![], script_block, config)
    }

    async fn compile_and_deploy(&mut self, generated_code: &str) -> Result<Address> {
        // Compile using local compiler (per-node, not shared)
        let compiler = ContractCompiler::new(&self.config_as_contract_config())?;
        let output = compiler.compile_source(
            PathBuf::from(format!("script_{}.sol", self.group_id)),
            generated_code.to_string(),
        )?;

        self.artifacts.insert_compilation("script".to_string(), output.clone());

        let bytecode = compiler.get_contract_bytecode(&output, "AomiScript")?;

        // Deploy using shared backend
        let chain_id = self.get_primary_chain_id()?;
        let sender = Address::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266")?;

        let (address, _result) = self.backend
            .deploy(chain_id, sender, Bytes::from(bytecode), U256::ZERO)
            .await?;

        self.artifacts.insert_deployment("script:AomiScript".to_string(), address);

        Ok(address)
    }

    async fn execute_run(&self, script_address: Address) -> Result<ExecutionResult> {
        let chain_id = self.get_primary_chain_id()?;
        let sender = Address::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266")?;
        let run_selector = keccak256("run()")[0..4].to_vec();

        self.backend
            .call(chain_id, sender, script_address, Bytes::from(run_selector), U256::ZERO)
            .await
    }

    fn build_transactions(&self, execution_result: &ExecutionResult) -> Vec<TransactionData> {
        // Extract from execution_result.broadcastable_transactions
        vec![]
    }

    fn build_failed_result(
        &self,
        error: String,
        generated_code: String,
        transactions: Vec<TransactionData>,
    ) -> GroupResult {
        let group_idx = self.group_id.parse::<usize>().unwrap_or(0);
        GroupResult {
            group_index: group_idx,
            description: self.group.description.clone(),
            operations: self.group.operations.clone(),
            inner: GroupResultInner::Failed {
                error,
                generated_code,
                transactions,
            },
        }
    }

    fn get_primary_chain_id(&self) -> Result<ChainId> {
        self.group
            .contracts
            .first()
            .map(|(chain_id, _, _)| chain_id.parse::<ChainId>())
            .unwrap_or(Ok(1))
            .map_err(|e| anyhow!("Invalid chain_id: {}", e))
    }

    fn config_as_contract_config(&self) -> ContractConfig {
        ContractConfig {
            foundry_config: self.config.foundry_config.clone(),
            no_auto_detect: self.config.no_auto_detect,
            evm_opts: self.config.evm_opts.clone(),
            traces: false,
            initial_balance: None,
            id: self.config.id.clone(),
        }
    }
}
```

**Location**: `aomi/crates/scripts/src/executor_v2/node.rs`

**Usage**: Spawned by `ForgeOrchestrator` for each ready group

---

### 5. ForgeOrchestrator (Manages Concurrency)

```rust
use dashmap::DashMap;
use tokio::sync::mpsc;
use std::sync::Arc;

pub type ExecutionId = String;
pub type NodeHandle = tokio::task::JoinHandle<GroupResult>;
pub type GroupNodes = Vec<Option<GroupNode>>;

struct ResultState {
    sender: mpsc::UnboundedSender<GroupResult>,
    results: Vec<GroupResult>,
}

/// 1 plan → 1 backend → [node1, node2, ...]
///                    → [chain1, chain2, ...]
/// Multiple plans → multiple backends
pub struct ForgeOrchestrator {
    /// Lightweight plans (structure + status only)
    plans: DashMap<ExecutionId, ExecutionPlan>,

    /// In-memory heavy tree states (pre-created nodes before spawning)
    execution_trees: DashMap<ExecutionId, GroupNodes>,

    /// Active group nodes (tracking only)
    active_nodes: Arc<DashMap<(ExecutionId, usize), ()>>,

    /// Result dispatch
    results: DashMap<ExecutionId, ResultState>,

    /// Per-plan backends (one backend per execution plan)
    backends: Arc<DashMap<ExecutionId, ExecutionBackend>>,

    /// Shared global resources
    source_fetcher: Arc<SourceFetcher>,
    baml_client: Arc<aomi_baml::BamlClient>,
}

impl ForgeOrchestrator {
    pub async fn new() -> Result<Self> {
        let source_fetcher = Arc::new(SourceFetcher::new());

        let clients = external_clients().await;
        let baml_client = clients.baml_client()?;

        Ok(Self {
            plans: DashMap::new(),
            execution_trees: DashMap::new(),
            active_nodes: Arc::new(DashMap::new()),
            results: DashMap::new(),
            backends: Arc::new(DashMap::new()),
            source_fetcher,
            baml_client,
        })
    }

    /// Create a new execution plan
    /// 1 plan → 1 backend → [nodes...]
    pub async fn create_plan(
        &self,
        execution_id: ExecutionId,
        groups: Vec<OperationGroup>,
    ) -> Result<usize> {
        let total_groups = groups.len();

        // Collect target chains
        let target_chains = Self::collect_target_chain_ids(&groups);

        // Create per-plan backend
        let base_foundry_config = foundry_config::Config::default();
        let backend = Arc::new(
            ExecutionBackend::new(&target_chains, &base_foundry_config).await?
        );
        self.backends.insert(execution_id.clone(), backend.clone());

        // Start background source fetching
        let all_contracts = Self::collect_unique_contracts(&groups);
        self.source_fetcher.request_fetch(all_contracts);

        // Create lightweight plan
        let plan = ExecutionPlan::from(groups.clone());
        self.plans.insert(execution_id.clone(), plan);

        // Pre-create GroupNodes (in-memory tree state)
        let mut nodes = Vec::new();
        for (idx, group) in groups.into_iter().enumerate() {
            let primary_chain = Self::get_primary_chain_id(&group)?;
            let config = GroupConfig::for_chain(
                base_foundry_config.clone(),
                &backend,
                primary_chain,
            )?;

            let node = GroupNode::new(
                idx.to_string(),
                group,
                config,  // owned by node
                backend.clone(),
                self.source_fetcher.clone(),
                self.baml_client.clone(),
            );
            nodes.push(Some(node));
        }
        self.execution_trees.insert(execution_id.clone(), nodes);

        // Initialize result state
        let (sender, _receiver) = mpsc::unbounded_channel();
        self.results.insert(
            execution_id.clone(),
            ResultState {
                sender,
                results: Vec::new(),
            },
        );

        Ok(total_groups)
    }

    fn get_primary_chain_id(group: &OperationGroup) -> Result<ChainId> {
        group
            .contracts
            .first()
            .map(|(chain_id, _, _)| chain_id.parse::<ChainId>())
            .unwrap_or(Ok(1))
            .map_err(|e| anyhow!("Invalid chain_id: {}", e))
    }

    /// Execute next batch of ready groups
    /// Pull pre-created nodes from execution_trees and spawn them
    pub async fn next_groups(&self, execution_id: &str) -> Result<Vec<GroupReceipt>> {
        let mut plan = self.plans.get_mut(execution_id)
            .ok_or_else(|| anyhow!("No plan found for {}", execution_id))?;

        // Find ready groups
        let ready_indices = plan.next_ready_batch();
        if ready_indices.is_empty() {
            return Ok(vec![]);
        }

        // Mark in progress
        plan.mark_in_progress(&ready_indices);

        // Wait for sources
        let ready_groups: Vec<&OperationGroup> = ready_indices
            .iter()
            .map(|&idx| &plan.groups[idx])
            .collect();
        Self::wait_for_sources(&self.source_fetcher, &ready_groups).await?;

        // Get pre-created nodes from execution_tree
        let mut execution_tree = self.execution_trees
            .get_mut(execution_id)
            .ok_or_else(|| anyhow!("No execution tree for {}", execution_id))?;

        // Spawn nodes
        let mut receipts = Vec::new();

        for &group_idx in &ready_indices {
            // Take ownership of node from tree (nodes are moved, not cloned)
            let node = execution_tree[group_idx]
                .take()
                .ok_or_else(|| anyhow!("GroupNode already moved for {}", group_idx))?;

            let description = node.group.description.clone();
            let ops_count = node.group.operations.len();

            let handle = tokio::spawn(async move { node.run().await });

            self.active_nodes
                .insert((execution_id.to_string(), group_idx), ());
            self.spawn_result_handler(execution_id.to_string(), group_idx, handle);

            receipts.push(GroupReceipt {
                execution_id: execution_id.to_string(),
                node_id: group_idx,
                description,
                ops_count,
            });
        }

        Ok(receipts)
    }

    /// Spawn per-node result handler (avoids global polling)
    fn spawn_result_handler(
        &self,
        exec_id: ExecutionId,
        node_id: usize,
        handle: NodeHandle,
    ) {
        let active_nodes = self.active_nodes.clone();
        let plans = self.plans.clone();
        let results = self.results.clone();

        tokio::spawn(async move {
            let result = handle.await;
            active_nodes.remove(&(exec_id.clone(), node_id));

            match result {
                Ok(result) => {
                    // Update plan status
                    if let Some(mut plan) = plans.get_mut(&exec_id) {
                        match &result.inner {
                            GroupResultInner::Done { transactions, generated_code } => {
                                plan.mark_done(node_id, transactions.clone(), generated_code.clone());
                            }
                            GroupResultInner::Failed { error, .. } => {
                                plan.mark_failed(node_id, error.clone());
                            }
                        }
                    }

                    // Append result and dispatch
                    if let Some(mut state) = results.get_mut(&exec_id) {
                        state.results.push(result.clone());
                        let _ = state.sender.send(result);
                    }
                }
                Err(e) => {
                    tracing::error!("Node task panicked: {}", e);
                    if let Some(mut plan) = plans.get_mut(&exec_id) {
                        plan.mark_failed(node_id, e.to_string());
                    }
                }
            }
        });
    }

    /// Get results for an execution
    pub fn get_results(&self, execution_id: &str) -> Option<Vec<GroupResult>> {
        self.results.get(execution_id).map(|state| state.results.clone())
    }

    /// Subscribe to result stream
    pub fn subscribe_results(&self, execution_id: &str) -> Option<mpsc::UnboundedReceiver<GroupResult>> {
        // TODO: implement proper subscription
        None
    }

    fn collect_unique_contracts(groups: &[OperationGroup]) -> Vec<(String, String, String)> {
        groups
            .iter()
            .flat_map(|g| g.contracts.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    fn collect_target_chain_ids(groups: &[OperationGroup]) -> HashSet<ChainId> {
        groups
            .iter()
            .flat_map(|g| g.contracts.iter().map(|(chain_id, _, _)| chain_id.parse::<ChainId>().unwrap_or(1)))
            .collect()
    }

    async fn wait_for_sources(
        source_fetcher: &SourceFetcher,
        groups: &[&OperationGroup],
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(60);

        while !source_fetcher.are_contracts_ready(groups).await {
            if Instant::now() > deadline {
                anyhow::bail!("Timeout waiting for contract sources");
            }

            let missing = source_fetcher.missing_contracts(groups).await;
            if !missing.is_empty() {
                source_fetcher.request_fetch(missing);
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupReceipt {
    pub execution_id: String,
    pub node_id: usize,
    pub description: String,
    pub ops_count: usize,
}
```

**Location**: `aomi/crates/scripts/src/executor_v2/orchestrator.rs`

**Usage**: Single instance created by tools, manages all execution

---

## Implementation Phases

### Phase 1: Foundation (Data Structures)

**Goal**: Extract data structures without breaking existing code

**Steps**:

1. Create `executor_v2/` directory structure
   ```bash
   mkdir -p aomi/crates/scripts/src/executor_v2
   touch aomi/crates/scripts/src/executor_v2/mod.rs
   ```

2. Create `config.rs`
   - Extract `GroupConfig` from `ContractConfig`
   - Add conversion method `from_contract_config()`
   - **Test**: Compile, no functional changes

3. Create `artifacts.rs`
   - Define `GroupArtifacts` with `compilations` and `deployments`
   - Add accessor methods
   - **Test**: Unit tests for insert/get operations

4. Create `backend.rs` (stub)
   - Define `ExecutionBackend` struct
   - Implement `new()` with single fork
   - Implement `execute_on_chain()` with basic EVM building
   - **Test**: Deploy and call simple contract

5. Update `executor_v2/mod.rs`
   ```rust
   pub mod config;
   pub mod artifacts;
   pub mod backend;

   pub use config::GroupConfig;
   pub use artifacts::GroupArtifacts;
   pub use backend::ExecutionBackend;
   ```

**Deliverables**:
- [ ] `executor_v2/config.rs` compiles
- [ ] `executor_v2/artifacts.rs` passes unit tests
- [ ] `executor_v2/backend.rs` can deploy/call contracts
- [ ] No changes to existing `forge_executor/` or `contract/`

**Validation**:
```bash
cargo test --package aomi-scripts executor_v2
```

---

### Phase 2: GroupNode (Execution Logic)

**Goal**: Move `execute_single_group` logic into `GroupNode::run()`

**Steps**:

1. Create `node.rs`
   - Define `GroupNode` struct with all fields
   - Implement `new()` constructor
   - Copy `execute_single_group` logic into `run()` method
   - Update to use `self.artifacts` instead of session state
   - Update to use `self.backend.deploy()` and `self.backend.call()`

2. Refactor helper methods
   - `fetch_sources()` - uses `self.source_fetcher`
   - `run_baml_extract()` - uses `self.baml_client`
   - `run_baml_generate_script()` - uses `self.baml_client`
   - `assemble_script()` - uses `ScriptAssembler`
   - `compile_and_deploy()` - uses local `ContractCompiler` + shared `backend`
   - `execute_run()` - uses shared `backend`
   - `build_transactions()` - extracts from `ExecutionResult`

3. Add error handling
   - `build_failed_result()` helper
   - Proper error propagation with context

4. **Test**: Run `GroupNode::run()` in isolation
   - Mock shared resources with test fixtures
   - Verify it returns `GroupResult` correctly

**Deliverables**:
- [ ] `executor_v2/node.rs` compiles
- [ ] `GroupNode::run()` executes single group end-to-end
- [ ] Integration test passes with test fixtures

**Validation**:
```bash
cargo test --package aomi-scripts executor_v2::node
```

---

### Phase 2.5: ScriptApp Loop Scaffolding

**Goal**: Restructure `GroupNode` to expose an agentic loop interface (fetch/review/edit/compile/deploy/execute/audit)

**Steps**:

1. Read and document current app lifecycle
   - Review `aomi/crates/apps/forge/src/app.rs` (`ForgeApp` construction, tools, docs)
   - Review `aomi/bin/cli/src/main.rs` (CLI initialization, backend selection, run loop)
   - Note how `AomiApp`-style agents are constructed and invoked

2. Break `GroupNode::run()` into elementary operations
   - `fetch_sources()` (already exists)
   - `review_script()` (new: script + dependencies review, returns verdict)
   - `edit_script()` (new: apply edits if review fails)
   - `compile_script()` (new: compile and return errors)
   - `deploy_script()` (new: deploy compiled bytecode)
   - `execute_script()` (new: execute `run()` and return `ExecutionResult`)
   - `audit_results()` (new: wallet end-state checks, money-in==money-out)

3. Define the optimistic path (one-shot BAML)
   - Keep current BAML extract/generate/assemble flow as initial attempt
   - Compile once optimistically
   - If compile or execute fails, enter ScriptApp loop for edit/retry

4. Introduce ScriptApp tool surface (API only, no implementation yet)
   - `compile_session`, `execute_contract`, `edit_script`, `fetch_contract`, `search_docs`
   - Stubs should map to the new GroupNode elementary operations

5. Add loop control contract
   - Max iterations and exit conditions
   - Restart-from-scratch path if audit fails

**Deliverables**:
- [ ] `GroupNode` exposes elementary methods used by the ScriptApp loop
- [ ] Optimistic BAML path retained; loop only triggered on review/compile/execute failure
- [ ] ScriptApp tool surface defined (stubs are acceptable)
- [ ] Notes in spec capture how `AomiApp` is initialized and invoked

**Validation**:
```bash
cargo test --package aomi-scripts executor_v2::node
```

---

### Phase 3: ForgeOrchestrator (Concurrency Manager)

**Goal**: Replace `ForgeManager` with `ForgeOrchestrator`

**Steps**:

1. Create `orchestrator.rs`
   - Define `ForgeOrchestrator` struct
   - Implement `new()` - initialize shared resources
   - Implement `create_plan()` - lightweight plan creation
   - Implement `next_groups()` - spawn `GroupNode` tasks
   - Implement `spawn_result_handler()` - per-node async result collection

2. Add result dispatch
   - `results: DashMap<ExecutionId, ResultState>`
   - `ResultState { sender, results }`
   - Push to channel on completion

3. Handle active node lifecycle
   - `active_nodes: DashMap<(ExecutionId, NodeId), ()>`
   - Await each node handle in a dedicated task
   - Remove completed, update plan status, dispatch result

4. **Test**: Multi-group concurrent execution
   - Create plan with 3 groups (no dependencies)
   - Call `next_groups()` once
   - Verify all 3 run in parallel
   - Verify results dispatched correctly

**Deliverables**:
- [ ] `executor_v2/orchestrator.rs` compiles
- [ ] Concurrent execution test passes
- [ ] Async result dispatch works

**Validation**:
```bash
cargo test --package aomi-scripts executor_v2::orchestrator
```

---

### Phase 4: Tool Integration

**Goal**: Wire `ForgeOrchestrator` into `forge/tools.rs`

**Steps**:

1. Update `SetExecutionPlan` tool
   - Replace `ForgeManager::new()` with `ForgeOrchestrator::new()`
   - Call `orchestrator.create_plan(execution_id, groups)`
   - Return ACK with `execution_id`

2. Update `NextGroups` tool
   - Call `orchestrator.next_groups(execution_id)`
   - Return ACK with `Vec<GroupReceipt>`
   - Results arrive async via channel (future work: wire to SystemEvent)

3. Add global orchestrator
   ```rust
   static ORCHESTRATOR: OnceCell<Arc<ForgeOrchestrator>> = OnceCell::const_new();

   async fn get_orchestrator() -> Result<Arc<ForgeOrchestrator>> {
       ORCHESTRATOR.get_or_try_init(|| async {
           ForgeOrchestrator::new().await.map(Arc::new)
       }).await
   }
   ```

4. **Test**: End-to-end tool flow
   - Call `SetExecutionPlan` with test groups
   - Call `NextGroups` with returned `execution_id`
   - Verify ACK received immediately
   - Verify results complete asynchronously

**Deliverables**:
- [ ] `tools.rs` uses `ForgeOrchestrator`
- [ ] Tests pass: `test_set_execution_plan_success_with_serialization`
- [ ] Tests pass: `test_next_groups_json_serialization`

**Validation**:
```bash
cargo test --package aomi-forge tools::
```

---

### Phase 5: Cleanup & Deprecation

**Goal**: Remove old `ForgeExecutor` and `ForgeManager`

**Steps**:

1. Mark deprecated
   ```rust
   #[deprecated(since = "2.0.0", note = "Use ForgeOrchestrator instead")]
   pub struct ForgeExecutor { ... }
   ```

2. Search for usages
   ```bash
   rg "ForgeExecutor\|ForgeManager" --type rust
   ```

3. Remove files (after confirming no usages)
   - `forge_executor/executor.rs` (keep types, move to `types.rs`)
   - `forge_executor/manager.rs`

4. Update `forge_executor/mod.rs`
   ```rust
   // Re-export v2 as main API
   pub use crate::executor_v2::{
       ForgeOrchestrator,
       GroupNode,
       GroupConfig,
       ExecutionBackend,
   };
   ```

5. Update documentation
   - Update README with new architecture
   - Add migration guide

**Deliverables**:
- [ ] Old code removed or deprecated
- [ ] No compilation errors
- [ ] All tests pass

**Validation**:
```bash
cargo test --package aomi-scripts
cargo test --package aomi-forge
```

---

## Migration Strategy

### Backward Compatibility

During transition:
1. Keep both `ForgeExecutor` and `ForgeOrchestrator` active
2. Add feature flag: `use_orchestrator_v2` in config
3. Tools check flag and route to appropriate implementation

### Rollout Plan

1. **Week 1**: Phase 1-2 (data structures + node)
2. **Week 2**: Phase 3 (orchestrator)
3. **Week 3**: Phase 4 (tool integration)
4. **Week 4**: Phase 5 (cleanup) + monitoring

### Validation Criteria

- [ ] All existing tests pass
- [ ] New integration tests pass
- [ ] Performance: concurrent groups execute faster than sequential
- [ ] Memory: no leaks in long-running orchestrator
- [ ] Correctness: same results as v1 on test fixtures

---

## Future Work (Post-v2.0)

### Agentic Sub-Agents (Executor 2.5)

Replace `GroupNode::run()` with `ScriptApp` agentic loop:

```rust
pub struct ScriptApp {
    node: GroupNode,
    app: Arc<AomiApp>,  // sub-agent with tools
}

impl ScriptApp {
    async fn run_agentic_loop(mut self) -> GroupResult {
        // Agent has tools:
        // - read_script / edit_script
        // - compile_session (returns Success/CompileError)
        // - execute_contract (returns Success/Revert)
        // - fetch_contract(address)
        // - read_blockchain_state(address, slot)
        // - log_analyzer

        loop {
            // Agent iterates: review → edit → compile → retry
            let agent_result = self.app.chat("...").await?;

            if agent_result.is_execution_success() {
                break;
            }
        }

        // Return final GroupResult
    }
}
```

**Steps**:
1. Define `ScriptApp` tools schema
2. Create `AomiApp` instance per group
3. Replace `GroupNode::run()` with `ScriptApp::run_agentic_loop()`
4. Add iteration limit + timeout

### Dynamic Plan Extension

```rust
impl ForgeOrchestrator {
    pub async fn extend_plan(
        &self,
        execution_id: &str,
        new_groups: Vec<OperationGroup>,
    ) -> Result<()> {
        let mut plan = self.plans.get_mut(execution_id)?;
        let base_idx = plan.groups.len();

        for (i, group) in new_groups.into_iter().enumerate() {
            plan.groups.push(group);
            plan.statuses.push(GroupStatus::Todo);
        }

        Ok(())
    }
}
```

### Multi-Chain Coordination

Enhance `ExecutionBackend` to support cross-chain operations:
- Bridge transactions
- Atomic cross-chain execution
- Cross-chain state verification

---

## Testing Strategy

### Unit Tests

- `GroupConfig::from_contract_config()` conversion
- `GroupArtifacts` insert/get operations
- `ExecutionBackend::execute_on_chain()` with mock EVM

### Integration Tests

- `GroupNode::run()` end-to-end with test fixtures
- `ForgeOrchestrator` concurrent execution (3 parallel groups)
- Tool flow: `SetExecutionPlan` → `NextGroups` → results

### Performance Tests

- Benchmark: 10 groups sequential (v1) vs parallel (v2)
- Memory leak test: 100 groups over 1 hour
- Latency test: time from `next_groups()` call to first result

### Regression Tests

- Run all existing `forge_executor/tests/fixtures/*.json` through v2
- Compare results with v1 (should be identical)

---

## Metrics & Monitoring

### Key Metrics

- `active_nodes_count` - number of running group nodes
- `plan_completion_time` - time from `create_plan()` to last result
- `group_execution_time` - per-group execution duration
- `compilation_cache_hit_rate` - artifact reuse
- `fork_contention_time` - time waiting for backend lock

### Logging

- `INFO`: Plan created, group started, group completed
- `DEBUG`: Sources fetched, BAML extract/generate, compilation output
- `WARN`: Compilation errors, execution reverts, timeout warnings
- `ERROR`: Node panics, backend errors, unhandled failures

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Backend lock contention | Use RwLock for read-heavy operations, profile lock time |
| Memory leaks in orchestrator | Add Drop impl, test long-running scenarios |
| Breaking existing tools | Feature flag, gradual rollout, comprehensive tests |
| Agent infinite loops (future) | Iteration limit, timeout per group |
| Cross-chain state corruption | Per-chain journals, atomic commits |

---

## Appendix: File Structure

```
aomi/crates/scripts/src/
├── executor_v2/
│   ├── mod.rs
│   ├── config.rs          # GroupConfig
│   ├── artifacts.rs       # GroupArtifacts
│   ├── backend.rs         # ExecutionBackend
│   ├── node.rs            # GroupNode
│   ├── orchestrator.rs    # ForgeOrchestrator
│   └── types.rs           # GroupReceipt, etc.
│
├── forge_executor/
│   ├── mod.rs             # Re-exports from executor_v2
│   ├── plan.rs            # ExecutionPlan (keep)
│   ├── types.rs           # GroupResult (keep)
│   ├── assembler.rs       # ScriptAssembler (keep)
│   ├── source_fetcher.rs  # SourceFetcher (keep)
│   ├── resources.rs       # SharedForgeResources (refactor)
│   ├── executor.rs        # [DEPRECATED]
│   └── manager.rs         # [DEPRECATED]
│
└── contract/
    ├── mod.rs
    ├── compiler.rs        # ContractCompiler (keep)
    ├── runner.rs          # [REFACTOR - extract Backend wrapper]
    └── session.rs         # [REFACTOR - becomes thin wrapper]
```

---

## Sign-off Checklist

Before merging to main:

- [ ] All phases completed
- [ ] All tests pass (unit, integration, regression)
- [ ] Performance benchmarks meet targets (>30% improvement)
- [ ] Documentation updated (README, API docs, migration guide)
- [ ] Code review completed
- [ ] Manual QA: run 5 real-world intents through v2
- [ ] Monitoring dashboards created
- [ ] Rollback plan documented
