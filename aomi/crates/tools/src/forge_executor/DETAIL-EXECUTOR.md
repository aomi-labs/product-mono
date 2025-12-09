# Forge Executor 2 - Detailed Implementation Plan

## Overview

ForgeExecutor2 is a stateful, dependency-aware executor that generates and executes Forge scripts from natural language operation groups. It uses a two-phase BAML approach and supports concurrent execution of independent operation groups.

## Architecture

```
Agent → set_execution_plan(groups[])
     ↓
ForgeExecutor {
    plan: ExecutionPlan
    source_fetcher: Arc<SourceFetcher>  // Long-running service with mpsc::channel
    baml_client: BamlClient
    contract_session: ContractSession   // Reused from existing
}
     ↓
     source_fetcher.request_fetch(all_contracts)  // Non-blocking, starts caching
     ↓
Agent → next_groups()
     ↓
     await until contracts ready in cache
     ↓
[Group 1, Group 2, ...] → Concurrent Execution
     ↓                    ↓
  BAML Phase 1      BAML Phase 1
  BAML Phase 2      BAML Phase 2
  Script Assembly   Script Assembly
  Execute Script    Execute Script
     ↓                    ↓
[GroupResult 1, GroupResult 2, ...]
  (transactions + generated_code, NOT tx_hash - backend doesn't land txs!)
```

## Core Data Structures

### 1. OperationGroup (from tmp.txt)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationGroup {
    pub description: String,              // "Wrap ETH and quote swap"
    pub operations: Vec<String>,          // ["wrap 0.75 ETH...", "quote..."]
    pub dependencies: Vec<usize>,         // [1, 3, ...] indices of groups this depends on
    pub contracts: Vec<(String, String, String)>  // (chain_id, address, name)
}
```

### 2. ExecutionPlan

```rust
pub struct ExecutionPlan {
    pub groups: Vec<OperationGroup>,
    pub statuses: Vec<GroupStatus>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GroupStatus {
    Todo,
    InProgress,
    Done {
        transactions: Vec<TransactionData>,
        generated_code: String,
    },
    Failed { error: String },
}

impl ExecutionPlan {
    /// Create from operation groups
    pub fn from(groups: Vec<OperationGroup>) -> Self {
        Self {
            groups,
            statuses: vec![GroupStatus::Todo; groups.len()],
        }
    }

    /// Get next batch of ready groups (dependencies satisfied, not in progress/done)
    pub fn next_ready_batch(&self) -> Vec<usize> {
        self.groups
            .iter()
            .enumerate()
            .filter(|(idx, group)| {
                // Must be Todo
                matches!(self.statuses[*idx], GroupStatus::Todo) &&
                // All dependencies must be Done
                group.dependencies.iter().all(|dep_idx| {
                    matches!(self.statuses[*dep_idx], GroupStatus::Done { .. })
                })
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Mark groups as in progress
    pub fn mark_in_progress(&mut self, indices: &[usize]) {
        for &idx in indices {
            self.statuses[idx] = GroupStatus::InProgress;
        }
    }

    /// Mark group as done
    pub fn mark_done(&mut self, idx: usize, transactions: Vec<TransactionData>, generated_code: String) {
        self.statuses[idx] = GroupStatus::Done { transactions, generated_code };
    }

    /// Mark group as failed
    pub fn mark_failed(&mut self, idx: usize, error: String) {
        self.statuses[idx] = GroupStatus::Failed { error };
    }
}
```

### 3. SourceFetcher (Long-Running Service)

```rust
use aomi_baml::ContractSource;  // Import from baml/types.rs
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Request to fetch a contract
#[derive(Clone, Debug)]
pub struct FetchRequest {
    pub chain_id: String,
    pub address: String,
    pub name: String,
}

/// Long-running source fetcher service (lives as long as ForgeExecutor)
pub struct SourceFetcher {
    cache: Arc<Mutex<HashMap<String, ContractSource>>>,  // key: "{chain_id}:{address}"
    fetch_tx: mpsc::UnboundedSender<FetchRequest>,
    _task_handle: JoinHandle<()>,
}

impl SourceFetcher {
    /// Initialize new source fetcher with empty cache
    /// Starts long-running service that continuously processes fetch requests
    pub fn new() -> Self {
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<FetchRequest>();

        let cache_clone = cache.clone();

        // Long-running service task
        let _task_handle = tokio::spawn(async move {
            while let Some(req) = fetch_rx.recv().await {
                let key = format!("{}:{}", req.chain_id, req.address);

                // Skip if already cached
                if cache_clone.lock().await.contains_key(&key) {
                    continue;
                }

                // Fetch using get_or_fetch_contract from db_tools.rs
                match Self::fetch_contract_data(&req).await {
                    Ok(source) => {
                        tracing::info!("Fetched and cached contract: {}", key);
                        cache_clone.lock().await.insert(key, source);
                    }
                    Err(e) => {
                        tracing::error!("Failed to fetch {}: {}", key, e);
                    }
                }
            }
        });

        Self { cache, fetch_tx, _task_handle }
    }

    /// Submit fetch requests for contracts (non-blocking)
    pub fn request_fetch(&self, contracts: Vec<(String, String, String)>) {
        for (chain_id, address, name) in contracts {
            let _ = self.fetch_tx.send(FetchRequest {
                chain_id,
                address,
                name,
            });
        }
    }

    /// Get contracts for a group (checks cache only, returns immediately)
    pub async fn get_contracts_for_group(
        &self,
        group: &OperationGroup,
    ) -> Result<Vec<ContractSource>> {
        let cache = self.cache.lock().await;
        let mut result = Vec::new();

        for (chain_id, address, _) in &group.contracts {
            let key = format!("{}:{}", chain_id, address);

            if let Some(source) = cache.get(&key) {
                result.push(source.clone());
            } else {
                anyhow::bail!("Contract {} not yet cached", key);
            }
        }

        Ok(result)
    }

    /// Check if all contracts for groups are cached and ready
    pub async fn are_contracts_ready(&self, groups: &[&OperationGroup]) -> bool {
        let cache = self.cache.lock().await;

        for group in groups {
            for (chain_id, address, _) in &group.contracts {
                let key = format!("{}:{}", chain_id, address);
                if !cache.contains_key(&key) {
                    return false;
                }
            }
        }
        true
    }

    /// Helper to fetch contract data using db_tools::get_or_fetch_contract
    async fn fetch_contract_data(req: &FetchRequest) -> Result<ContractSource> {
        use crate::db_tools::get_or_fetch_contract;

        let chain_id_u32 = req.chain_id.parse::<u32>()
            .map_err(|e| anyhow!("Invalid chain_id: {}", e))?;

        // Use the existing get_or_fetch_contract function from db_tools.rs
        let contract_data = get_or_fetch_contract(chain_id_u32, req.address.clone()).await
            .map_err(|e| anyhow!("Failed to fetch contract: {}", e))?;

        Ok(ContractSource {
            chain_id: req.chain_id.clone(),
            address: contract_data.address,
            name: req.name.clone(),
            abi: serde_json::to_string(&contract_data.abi)?,
            source_code: if contract_data.source_code.is_empty() {
                None
            } else {
                Some(contract_data.source_code)
            },
        })
    }
}
```

### 4. ScriptAssembler (Copy from aomi/crates/tools/src/forge_executor/assembler.rs)

**NOTE: Use the exact same implementation from `aomi/crates/tools/src/forge_executor/assembler.rs`**

The assembler is already implemented and handles:
- Import deduplication (lines 54-74)
- Inline interface deduplication (lines 76-90)
- Funding setup with ETH and ERC20 (lines 144-182)
- Proper code line formatting (lines 133-142)

Key API:
```rust
use aomi_baml::{CodeLine, Import, Interface, ScriptBlock};
use crate::forge_script_builder::{AssemblyConfig, FundingRequirement};

pub struct ScriptAssembler;

impl ScriptAssembler {
    pub fn assemble(
        contract_definitions: Vec<String>,
        block: &ScriptBlock,
        config: AssemblyConfig,
    ) -> Result<String> {
        // Full implementation in assembler.rs
    }
}
```

**File location**: Copy `aomi/crates/tools/src/forge_executor/assembler.rs` to `aomi/crates/tools/src/forge_executor2/assembler.rs`

### 5. GroupResult (Return Type)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupResult {
    pub group_index: usize,
    pub description: String,
    pub operations: Vec<String>,
    pub generated_code: String,           // The assembled Forge script
    pub transactions: Vec<TransactionData>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionData {
    pub from: Option<String>,
    pub to: Option<String>,
    pub value: String,
    pub data: String,
    pub rpc_url: String,
}
```

## ForgeExecutor Implementation

```rust
pub struct ForgeExecutor {
    plan: ExecutionPlan,
    source_fetcher: Arc<SourceFetcher>,
    baml_client: Arc<BamlClient>,
    contract_session: ContractSession,
}

impl ForgeExecutor {
    /// Create new executor and start background source fetching
    pub async fn new(groups: Vec<OperationGroup>) -> Result<Self> {
        let plan = ExecutionPlan::from(groups.clone());

        // Extract all unique contracts
        let all_contracts: Vec<(String, String, String)> = groups
            .iter()
            .flat_map(|g| g.contracts.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        // Create long-running source fetcher service
        let source_fetcher = Arc::new(SourceFetcher::new());

        // Submit fetch requests immediately (non-blocking)
        source_fetcher.request_fetch(all_contracts);

        // Create BAML client
        let baml_client = Arc::new(BamlClient::new()?);

        // Create contract session (reuse from existing code)
        let contract_config = ContractConfig {
            foundry_config: FoundryConfig::default(),
            evm_opts: EvmOpts::default(),
        };
        let contract_session = ContractSession::new(contract_config)?;

        Ok(Self {
            plan,
            source_fetcher,
            baml_client,
            contract_session,
        })
    }

    /// Execute next batch of ready groups concurrently
    pub async fn next_groups(&mut self) -> Result<Vec<GroupResult>> {
        // 1. Get indices of ready groups
        let ready_indices = self.plan.next_ready_batch();

        if ready_indices.is_empty() {
            return Ok(vec![]);  // No more groups to execute
        }

        // 2. Get the actual groups
        let ready_groups: Vec<&OperationGroup> = ready_indices
            .iter()
            .map(|&idx| &self.plan.groups[idx])
            .collect();

        // 3. Wait for all contracts to be fetched
        while !self.source_fetcher.are_contracts_ready(&ready_groups).await {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // 4. Mark groups as in progress
        self.plan.mark_in_progress(&ready_indices);

        // 5. Execute all groups concurrently
        let mut tasks = Vec::new();

        for &group_idx in &ready_indices {
            let group = self.plan.groups[group_idx].clone();
            let source_fetcher = self.source_fetcher.clone();
            let baml_client = self.baml_client.clone();
            let session = self.contract_session.clone();  // Needs Clone impl

            let task = tokio::spawn(async move {
                Self::execute_single_group(
                    group_idx,
                    group,
                    source_fetcher,
                    baml_client,
                    session,
                ).await
            });

            tasks.push(task);
        }

        // 6. Wait for all tasks to complete
        let mut results = Vec::new();
        for (i, task) in tasks.into_iter().enumerate() {
            let group_idx = ready_indices[i];
            match task.await {
                Ok(Ok(result)) => {
                    self.plan.mark_done(
                        group_idx,
                        result.transactions.clone(),
                        result.generated_code.clone()
                    );
                    results.push(result);
                }
                Ok(Err(e)) => {
                    self.plan.mark_failed(group_idx, e.to_string());
                    eprintln!("Group {} failed: {}", group_idx, e);
                }
                Err(e) => {
                    self.plan.mark_failed(group_idx, e.to_string());
                    eprintln!("Task {} panicked: {}", group_idx, e);
                }
            }
        }

        Ok(results)
    }

    /// Execute a single group (called concurrently)
    async fn execute_single_group(
        group_idx: usize,
        group: OperationGroup,
        source_fetcher: Arc<SourceFetcher>,
        baml_client: Arc<BamlClient>,
        contract_session: ContractSession,
    ) -> Result<GroupResult> {
        // 1. Get contract sources
        let sources = source_fetcher.get_contracts_for_group(&group).await?;

        // 2. BAML Phase 1: Extract contract info
        let extracted_infos = baml_client
            .extract_contract_info(&group.operations, &sources)
            .await?;

        // 3. BAML Phase 2: Generate script
        let script_block = baml_client
            .generate_script(&group.operations, &extracted_infos)
            .await?;

        // 4. Assemble complete Forge script
        let config = AssemblyConfig::default();
        let generated_code = ScriptAssembler::assemble_from_codelines(
            script_block,
            config,
        )?;

        // 5. Compile the script
        let script_path = PathBuf::from(format!("script_group_{}.sol", group_idx));
        contract_session.compile_source(
            format!("group_{}", group_idx),
            script_path.clone(),
            generated_code.clone(),
        )?;

        // 6. Deploy the script contract
        let script_address = contract_session
            .deploy_contract(&format!("group_{}", group_idx), "forge_script")
            .await?;

        // 7. Call the run() function
        let run_selector = keccak256("run()")[0..4].to_vec();
        let execution_result = contract_session
            .call_contract(script_address, Bytes::from(run_selector), None)
            .await?;

        // 8. Extract broadcastable transactions (similar to line 588)
        let transactions = execution_result
            .broadcastable_transactions
            .iter()
            .map(|btx| TransactionData {
                from: btx.transaction.from().map(|addr| format!("{:?}", addr)),
                to: btx.transaction.to().and_then(|kind| match kind {
                    alloy_primitives::TxKind::Call(addr) => Some(format!("{:?}", addr)),
                    alloy_primitives::TxKind::Create => None,
                }),
                value: btx.transaction.value().unwrap_or(U256::ZERO).to_string(),
                data: alloy_primitives::hex::encode(
                    btx.transaction.input().unwrap_or(&Default::default()),
                ),
                rpc_url: btx.rpc.clone(),
            })
            .collect();

        Ok(GroupResult {
            group_index: group_idx,
            description: group.description,
            operations: group.operations,
            generated_code,
            transactions,
        })
    }
}
```

## Tool Definitions

### set_execution_plan

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetExecutionPlanParams {
    pub groups: Vec<OperationGroup>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetExecutionPlanResult {
    pub success: bool,
    pub message: String,
    pub total_groups: usize,
}

// Store executor in ForgeApp context
// The tool creates ForgeExecutor and stores it globally
```

### next_groups

```rust
// No parameters - operates on stored executor

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NextGroupsResult {
    pub results: Vec<GroupResult>,
    pub remaining_groups: usize,
}

// Retrieves executor from ForgeApp context and calls next_groups()
```

## Implementation Checklist

### Phase 1: Core Data Structures
- [ ] Create `forge_executor2/plan.rs` with `OperationGroup`, `ExecutionPlan`, `GroupStatus`
- [ ] Implement `next_ready_batch()` logic with dependency resolution
- [ ] Add tests for dependency graph resolution

### Phase 2: Source Fetching (Long-Running Service)
- [ ] Create `forge_executor2/source_fetcher.rs` with `SourceFetcher` long-running service
- [ ] Implement `new()` that starts background task with mpsc::channel
- [ ] Implement `request_fetch()` for non-blocking fetch submission
- [ ] Implement `get_contracts_for_group()` for cache-only lookup
- [ ] Integrate with `db_tools::get_or_fetch_contract`
- [ ] Add tests for concurrent fetching and cache behavior

### Phase 3: Script Assembly
- [ ] Copy `forge_executor/assembler.rs` to `forge_executor2/assembler.rs`
- [ ] Verify it works with BAML `CodeLine` and `ScriptBlock`
- [ ] Add tests with sample `ScriptBlock`

### Phase 4: Executor Core
- [ ] Create `forge_executor2/executor.rs` with `ForgeExecutor`
- [ ] Implement `new()` with background source fetching
- [ ] Implement `next_groups()` with concurrent execution
- [ ] Implement `execute_single_group()` with full BAML→Script→Execution flow
- [ ] Add integration tests

### Phase 5: Tool Registration
- [ ] Create `forge_executor2/tools.rs`
- [ ] Implement `SetExecutionPlan` tool
- [ ] Implement `NextGroups` tool
- [ ] Add executor storage in `ForgeApp` context
- [ ] Register tools in tools registry

### Phase 6: Testing
- [ ] Unit test each module
- [ ] Integration test full flow: plan → fetch → execute
- [ ] Test concurrent execution of independent groups
- [ ] Test dependency ordering (group 2 waits for group 1)
- [ ] Test error handling and partial failures

## Key Design Decisions

1. **Long-Running SourceFetcher Service**: Lives as long as ForgeExecutor, uses mpsc::channel to continuously process fetch requests and maintain a shared cache
2. **Non-Blocking Fetch Submission**: `request_fetch()` is non-blocking, agent gets immediate response from `new()`
3. **Cache-Only Lookups**: `get_contracts_for_group()` only checks cache, doesn't block on fetching
4. **Batch Execution**: Returns `Vec<GroupResult>` for concurrent groups
5. **Dependency Resolution**: `next_ready_batch()` only returns groups with satisfied dependencies
6. **Backend Never Lands Transactions**: GroupStatus::Done contains `transactions` + `generated_code`, NOT `tx_hash` - wallet is responsible for landing transactions
7. **Reuse Existing Code**: `ContractSession`, `get_or_fetch_contract`, `ScriptAssembler`, transaction extraction
8. **Stateful Executor**: Stored in `ForgeApp`, maintains state between tool calls

## File Structure

```
aomi/crates/tools/src/forge_executor2/
├── DETAIL-EXECUTOR.md          # This file
├── mod.rs                      # Public API
├── plan.rs                     # ExecutionPlan, OperationGroup, GroupStatus
├── source_fetcher.rs           # SourceFetcherHandle, background fetching
├── assembler.rs                # ScriptAssembler (modified for CodeLine)
├── executor.rs                 # ForgeExecutor main implementation
├── tools.rs                    # SetExecutionPlan, NextGroups tools
└── types.rs                    # GroupResult, TransactionData
```

## Timeline Estimate

| Phase | Estimated Time |
|-------|---------------|
| Phase 1: Core structures | 2 hours |
| Phase 2: Source fetching | 3 hours |
| Phase 3: Script assembly | 2 hours |
| Phase 4: Executor core | 4 hours |
| Phase 5: Tool registration | 2 hours |
| Phase 6: Testing | 3 hours |
| **Total** | **~16 hours** |
