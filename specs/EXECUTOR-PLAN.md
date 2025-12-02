# Forge Executor Implementation Plan

## Overview

Two-phase execution system for generating and executing Forge scripts from natural language operations.

```
Agent → set_execution_plan(groups) → ForgeExecutor
Agent → next_group() → BAML Phase 1 → BAML Phase 2 → Script Execution → Transactions
```

---

## Design Adjustments (from tmp.txt)

- Plan-driven flow: the agent precomputes operation groups with explicit dependencies and contract tuples; `next_group()` should always pick a group whose dependencies are done/in-progress to avoid blocking on unmet prereqs.
- Per-group invocation: call BAML + execution one group at a time; do not batch multiple groups into a single script.
- Placeholders: allow `{$X}` style placeholders in operations; executor should pass them through to BAML and support downstream computation tools (e.g., token price helpers) before emitting final transactions. Keep placeholders as variables in the generated script unless a deterministic value is provided.
- Contract discovery rails: OperationGroup includes `(chain_id, address, name)` for each contract; source fetcher must ensure ABI/source availability before calling BAML.
- Per-group builder: executor assembles the BAML request per group (mirrors legacy ForgeScriptBuilderParameters semantics) instead of a monolithic parameter set.

---

## Architecture

### Module Structure

```
aomi/crates/tools/src/
├── baml/
│   ├── baml_src/               # Source `.baml` specs (phase1/phase2)
│   ├── baml_client/            # Generated Rust client (checked in)
│   ├── mod.rs                  # Public API
│   ├── types.rs                # Rust types matching BAML schemas
│   └── client.rs               # BAML client wrapper (builds on generated client)
│
├── forge_executor/
│   ├── mod.rs                  # Public API
│   ├── executor.rs             # ForgeExecutor implementation
│   ├── plan.rs                 # ExecutionPlan, OperationGroup
│   └── source_fetcher.rs       # Async contract source fetching
│
├── contract/                   # Existing module with ContractRunner
└── forge_script_builder.rs     # Legacy tool to be sunset after parity
```

---

## Data Flow

### Agent Creates Plan

```rust
// Agent generates this structure
let groups = vec![
    OperationGroup {
        description: "Wrap ETH and add liquidity".to_string(),
        operations: vec![
            "wrap 0.75 ETH using IWETH.wrap() at 0xC02...".to_string(),
            "compute equivalent USDC {$X} for 0.75 WETH".to_string(),
            "add liquidity with 0.75 WETH and {$X} USDC".to_string(),
        ],
        dependencies: vec![],  // No deps
        contracts: vec![
            ("1".to_string(), "0xC02aaa...".to_string(), "WETH".to_string()),
            ("1".to_string(), "0xb27308...".to_string(), "Quoter".to_string()),
            ("1".to_string(), "0x7a250d...".to_string(), "Router".to_string()),
        ],
    },
    OperationGroup {
        description: "Verify LP token balance".to_string(),
        operations: vec![
            "verify UNI-V2 LP token balance at 0xB4e...".to_string(),
        ],
        dependencies: vec![0],  // Depends on group 0
        contracts: vec![
            ("1".to_string(), "0xB4e16d...".to_string(), "UniswapV2Pair".to_string()),
        ],
    },
];

// Tool call
ForgeExecutor::set_execution_plan(groups)
```

### ForgeExecutor Initializes

```rust
impl ForgeExecutor {
    pub async fn new(groups: Vec<OperationGroup>) -> Result<Self> {
        let plan = ExecutionPlan::from(groups);

        // Start async contract source fetching
        let source_fetcher = SourceFetcher::new(
            plan.all_contracts()  // Extract all unique contracts
        );

        Ok(Self {
            plan,
            source_fetcher,
            baml_client: BamlClient::new()?,
            current_index: 0,
            group_statuses: vec![GroupStatus::Todo; plan.groups.len()],
        })
    }
}
```

### Agent Calls next_group()

```rust
impl ForgeExecutor {
    pub async fn next_group(&mut self) -> Result<NextGroupResult> {
        // 1. Get next ready group (respecting dependencies)
        let group = self.plan.next_ready_group()?;

        // 2. Wait for contract sources if needed
        let sources = self.source_fetcher
            .get_contracts_for_group(&group)
            .await?;

        // 3. BAML Phase 1: Extract contract info
        let extracted_infos = self.baml_client.extract_contract_info(
            &group.operations,
            &sources,
        ).await?;

        // 4. BAML Phase 2: Generate script
        let script_block = self.baml_client.generate_script(
            &group.operations,
            &extracted_infos,
        ).await?;

        // 5. Assemble complete Forge script
        let script = self.assemble_script(script_block)?;

        // 6. Execute script (compile + simulate)
        let transactions = self.execute_script(&script).await?;

        // 7. Mark in progress
        self.mark_in_progress();

        Ok(NextGroupResult {
            group_description: group.description.clone(),
            operations: group.operations.clone(),
            script,
            transactions,
        })
    }
}
```

---

## Phase 1: BAML Contract Info Extraction

### BAML Schema (`forge_transactions.baml`)

```baml
// ============================================================================
// Phase 1: Extract Relevant Contract Information
// ============================================================================

class ContractInfo {
    description string?    // Optional human description
    address string         // Contract address
    abi string             // Full ABI JSON
    source_code string?    // Full source code (if available)
}

class Function {
    description string?    // Description from the agent or source
    signature string       // "wrap()" or "addLiquidity(...)"
    abi string             // JSON snippet for this function
    body string?           // Function body from source code
    arguments string?      // Human-readable args, if available
}

class Storage {
    description string?    // What is stored here
    declaration string?    // "uint256 public totalSupply;"
    index int?             // Order of declaration
}

class Event {
    signature string       // "Transfer(address indexed from, address indexed to, uint256 value)"
    abi string             // JSON snippet
}

class ExtractedContractInfo {
    description string?    // Optional contract description
    address string         // Address of the contract
    interface_name string  // "IWETH", "IQuoter"
    functions Function[]   // Functions needed for this group
    storages Storage[]     // Storage touched
    events Event[]         // Events emitted/read
}

function ExtractContractInfo(
    group_operations: string[],
    contracts: ContractInfo[]
) -> ExtractedContractInfo[] {
    client CustomHaiku
    prompt #"
        Given these operations:
        {{ group_operations }}

        And these contracts:
        {{ contracts }}

        For EACH contract, extract:
        1. Which functions are needed?
        2. What storage variables are accessed?
        3. What events are emitted?

        Include function bodies to understand behavior.

        {{ ctx.output_format }}
    "#
}
```

### Rust Implementation (helper in `baml/client.rs`)

```rust
use baml_client::models::*;
use baml_client::apis::{configuration::Configuration, default_api};

pub async fn extract_contract_info(
    config: &Configuration,
    operations: &[String],
    contracts: &[ContractSource],
) -> Result<Vec<ExtractedContractInfo>> {
    let baml_contracts: Vec<baml_client::models::ContractInfo> = contracts
        .iter()
        .map(|c| baml_client::models::ContractInfo {
            address: c.address.clone(),
            abi: c.abi.clone(),
            source_code: c.source_code.clone(),
        })
        .collect();

    let request = ExtractContractInfoRequest {
        group_operations: operations.to_vec(),
        contracts: baml_contracts,
    };

    let result = default_api::extract_contract_info(config, request)
        .await
        .map_err(|e| anyhow::anyhow!("BAML Phase 1 failed: {}", e))?;

    Ok(result)
}
```

---

## Phase 2: BAML Script Generation

### BAML Schema (`forge_transactions.baml`)

```baml
// ============================================================================
// Phase 2: Generate Forge Script
// ============================================================================

class Import {
    interface_name string  // "IERC20"
    source string         // "forge-std/interfaces/IERC20.sol"
}

class Interface {
    name string           // "IWETH"
    solidity_code string  // "interface IWETH { function wrap() external payable; }"
}

class CodeLine {
    line string           // "IWETH(0xC02...).wrap{value: 0.75 ether}();"
    import Import?        // If from forge-std
    interface Interface?  // If custom interface
}

class ScriptBlock {
    codelines CodeLine[]
}

function GenerateScript(
    group_operations: string[],
    extracted_infos: ExtractedContractInfo[]
) -> ScriptBlock {
    client CustomHaiku
    prompt #"
        Generate Forge script for: {{ group_operations }}

        Using: {{ extracted_infos }}

        For each operation, output:
        1. Solidity code line
        2. Import if forge-std interface (IERC20, etc.)
        3. Interface definition if custom (IWETH, IQuoter)

        Handle placeholders like {$X} as variable names.

        Example:
        {
          "codelines": [
            {
              "line": "uint256 ethAmount = 75 * 10**16;",
              "import": null,
              "interface": null
            },
            {
              "line": "IWETH(0xC02...).wrap{value: ethAmount}();",
              "import": null,
              "interface": {
                "name": "IWETH",
                "solidity_code": "interface IWETH { function wrap() external payable; }"
              }
            }
          ]
        }

        {{ ctx.output_format }}
    "#
}
```

### Rust Implementation (helper in `baml/client.rs`)

```rust
pub async fn generate_script(
    config: &Configuration,
    operations: &[String],
    extracted_infos: &[ExtractedContractInfo],
) -> Result<ScriptBlock> {
    let request = GenerateScriptRequest {
        group_operations: operations.to_vec(),
        extracted_infos: extracted_infos.to_vec(),
    };

    let result = default_api::generate_script(config, request)
        .await
        .map_err(|e| anyhow::anyhow!("BAML Phase 2 failed: {}", e))?;

    Ok(result)
}
```

---

## Script Assembly

### Implementation (`forge_executor/executor.rs`)

```rust
impl ForgeExecutor {
    fn assemble_script(&self, block: ScriptBlock) -> Result<String> {
        let mut script = String::new();

        // Pragma
        script.push_str("pragma solidity ^0.8.20;\n\n");

        // Imports (deduplicated)
        script.push_str("import {Script} from \"forge-std/Script.sol\";\n");
        script.push_str("import {StdCheats} from \"forge-std/StdCheats.sol\";\n");

        let mut imports = HashSet::new();
        for codeline in &block.codelines {
            if let Some(import) = &codeline.import {
                imports.insert((
                    import.interface_name.clone(),
                    import.source.clone(),
                ));
            }
        }
        for (name, source) in imports {
            script.push_str(&format!("import {{{}}} from \"{}\";\n", name, source));
        }
        script.push_str("\n");

        // Interfaces (deduplicated)
        let mut interfaces = HashMap::new();
        for codeline in &block.codelines {
            if let Some(interface) = &codeline.interface {
                interfaces.insert(
                    interface.name.clone(),
                    interface.solidity_code.clone(),
                );
            }
        }
        for code in interfaces.values() {
            script.push_str(code);
            script.push_str("\n\n");
        }

        // Contract wrapper
        script.push_str("contract forge_script is Script, StdCheats {\n");
        script.push_str("    function run() public {\n");
        script.push_str("        vm.deal(msg.sender, 10 ether);\n");
        script.push_str("        vm.startBroadcast();\n\n");

        // Code lines
        for codeline in &block.codelines {
            script.push_str("        ");
            script.push_str(&codeline.line);
            if !codeline.line.ends_with(';') {
                script.push_str(";");
            }
            script.push_str("\n");
        }

        script.push_str("\n        vm.stopBroadcast();\n");
        script.push_str("    }\n");
        script.push_str("}\n");

        Ok(script)
    }
}
```

---

## Script Execution

### Execution (reuse existing `contract::ContractRunner`)

`ForgeExecutor` should not introduce a new runner. Instead, assemble the script, then:

1) Build a `ContractConfig` using the existing `contract` module (same pattern as `forge_script_builder.rs`).
2) Create a `ContractSession` + `ContractRunner`.
3) Compile the assembled script from a temp path (`forge_script.sol`).
4) Deploy and call `run()` using `ContractRunner`.
5) Return `broadcastable_transactions` as `TransactionData`.

---

## Source Fetching

### Implementation (`forge_executor/source_fetcher.rs`)

```rust
use tokio::sync::mpsc;

pub struct ContractSource {
    pub chain_id: String,
    pub address: String,
    pub name: String,
    pub abi: String,
    pub source_code: Option<String>,
}

pub struct SourceFetcher {
    sources: HashMap<String, ContractSource>,
    fetch_task: JoinHandle<()>,
}

impl SourceFetcher {
    pub fn new(contracts: Vec<(String, String, String)>) -> Self {
        let (tx, mut rx) = mpsc::channel(100);

        let fetch_task = tokio::spawn(async move {
            // TODO: Fetch from DB or Etherscan
            // For now, placeholder
            for (chain_id, address, name) in contracts {
                let source = ContractSource {
                    chain_id: chain_id.clone(),
                    address: address.clone(),
                    name: name.clone(),
                    abi: "[]".to_string(),  // TODO: fetch from DB
                    source_code: None,      // TODO: fetch if available
                };
                let _ = tx.send((address, source)).await;
            }
        });

        Self {
            sources: HashMap::new(),
            fetch_task,
        }
    }

    pub async fn get_contracts_for_group(
        &mut self,
        group: &OperationGroup,
    ) -> Result<Vec<ContractSource>> {
        // Wait for sources to be fetched
        // TODO: implement proper waiting logic

        let mut result = Vec::new();
        for (_, addr, _) in &group.contracts {
            if let Some(source) = self.sources.get(addr) {
                result.push(source.clone());
            }
        }

        Ok(result)
    }
}
```

---

## Types (`forge_executor/plan.rs`)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationGroup {
    pub description: String,
    pub operations: Vec<String>,
    pub dependencies: Vec<usize>,
    pub contracts: Vec<(String, String, String)>,  // (chain_id, address, name)
}

#[derive(Clone, Debug, PartialEq)]
pub enum GroupStatus {
    Todo,
    InProgress,
    Done { tx_hash: Option<String> },
    Failed { error: String },
}

pub struct ExecutionPlan {
    pub groups: Vec<OperationGroup>,
    pub dependency_graph: Vec<Vec<usize>>,
}

impl ExecutionPlan {
    pub fn from(groups: Vec<OperationGroup>) -> Self {
        let dependency_graph = groups
            .iter()
            .map(|g| g.dependencies.clone())
            .collect();

        Self {
            groups,
            dependency_graph,
        }
    }

    pub fn next_ready_group(&self, statuses: &[GroupStatus]) -> Option<&OperationGroup> {
        for (idx, group) in self.groups.iter().enumerate() {
            // Must be Todo
            if !matches!(statuses[idx], GroupStatus::Todo) {
                continue;
            }

            // All dependencies must be Done
            let deps_done = group.dependencies.iter().all(|dep_idx| {
                matches!(statuses[*dep_idx], GroupStatus::Done { .. })
            });

            if deps_done {
                return Some(group);
            }
        }

        None
    }

    pub fn all_contracts(&self) -> Vec<(String, String, String)> {
        let mut all = Vec::new();
        for group in &self.groups {
            all.extend(group.contracts.clone());
        }

        // Deduplicate by address
        let mut seen = HashSet::new();
        all.retain(|(_, addr, _)| seen.insert(addr.clone()));

        all
    }
}
```

---

## Tool Integration

### Tool: `set_execution_plan`

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

impl Tool for SetExecutionPlan {
    type Args = SetExecutionPlanParams;
    type Output = SetExecutionPlanResult;

    async fn definition(&self, _: &LLM) -> ToolDefinition {
        serde_json::from_value(json!({
            "name": "set_execution_plan",
            "description": "Initialize execution plan with operation groups",
            "input_schema": {
                "type": "object",
                "properties": {
                    "groups": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "description": { "type": "string" },
                                "operations": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "dependencies": {
                                    "type": "array",
                                    "items": { "type": "number" }
                                },
                                "contracts": {
                                    "type": "array",
                                    "items": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    }
                                }
                            }
                        }
                    }
                },
                "required": ["groups"]
            }
        }))
        .unwrap()
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output> {
        // Store executor in app context
        let executor = ForgeExecutor::new(args.groups).await?;

        // TODO: Store executor in ForgeApp context

        Ok(SetExecutionPlanResult {
            success: true,
            message: format!("Plan created with {} groups. Call next_group() to start.", executor.plan.groups.len()),
            total_groups: executor.plan.groups.len(),
        })
    }
}
```

### Tool: `next_group`

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NextGroupResult {
    pub group_description: String,
    pub operations: Vec<String>,
    pub script: String,
    pub transactions: Vec<TransactionData>,
}

impl Tool for NextGroup {
    type Args = ();  // No args
    type Output = NextGroupResult;

    async fn definition(&self, _: &LLM) -> ToolDefinition {
        serde_json::from_value(json!({
            "name": "next_group",
            "description": "Execute next ready operation group",
            "input_schema": {
                "type": "object",
                "properties": {}
            }
        }))
        .unwrap()
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output> {
        // TODO: Get executor from ForgeApp context
        let mut executor = get_executor()?;

        executor.next_group().await
    }
}
```

---

## Implementation Steps

### Step 1: BAML Schema Updates
- [ ] Update `forge_transactions.baml` with Phase 1 and Phase 2 functions
- [ ] Add all necessary types (ContractInfo, ExtractedContractInfo, ScriptBlock, etc.)
- [ ] Write comprehensive BAML tests
- [ ] Regenerate BAML client into `baml/baml_client` from `baml/baml_src`: run `baml-cli generate` in `aomi/crates/tools` pointing src→out paths
- Tests:
  - Phase1 schema round-trip: feed sample contracts + ops through generated client mock, assert required fields (address, functions, events) survive serialization.
  - Phase2 schema round-trip: sample ScriptBlock with imports/interfaces serializes and deserializes intact (no missing fields).

### Step 2: BAML Module (`aomi/crates/tools/src/baml/`)
- [ ] `baml_src/`: Check in `.baml` specs and keep them in sync with generated client
- [ ] `baml_client/`: Regenerated client, vendored in-repo
- [ ] `types.rs`: Define Rust types matching BAML schemas
- [ ] `client.rs`: Wrapper for BAML client configuration (builds on generated client)
- [ ] `mod.rs`: Public API exports
- Tests:
  - Client happy path: mock BAML endpoints to return `ExtractedContractInfo` and `ScriptBlock`; assert wrapper passes through and preserves lines/order.
  - Configuration default: if `BAML_API_URL` unset, uses localhost default.

### Step 3: Forge Executor Module (`aomi/crates/tools/src/forge_executor/`)
- [ ] `plan.rs`: `ExecutionPlan`, `OperationGroup`, `GroupStatus`
- [ ] `source_fetcher.rs`: `SourceFetcher` for async contract fetching
- [ ] `executor.rs`: `ForgeExecutor` main implementation
- [ ] `mod.rs`: Public API exports
- Tests:
  - ExecutionPlan dependency gating: groups with unmet deps are skipped; ready groups are returned in order; deduped `all_contracts`.
  - SourceFetcher basic fetch: seed with two contracts, ensure `get_contracts_for_group` returns matching ABIs/names for that group.
  - Script assembly minimal: given ScriptBlock with import + interface + two lines, assembled script contains pragma/imports/interface and lines inside run().
  - Executor happy path (with mocked BAML + mock fetcher + stub runner): next_group returns script and transactions, marks status updated; ensure ContractRunner is invoked once.
  - Placeholder pass-through: operations containing `{$X}` result in script containing the same placeholder (or a variable declaration) without being dropped or mangled.

### Step 4: Tool Registration
- [ ] Add `SetExecutionPlan` tool to `tools.rs`
- [ ] Add `NextGroup` tool to `tools.rs`
- [ ] Register tools in `ForgeApp`
- Tests:
  - set_execution_plan tool schema: validates required fields and rejects missing groups.
  - next_group tool wiring: uses stored executor, errors when none is set.

### Step 5: State Management in ForgeApp
- [ ] Add `executor: Option<ForgeExecutor>` to `ForgeApp` or create `ForgeContext`
- [ ] Implement getter/setter for executor access from tools
- Tests:
  - ForgeApp state round-trip: set executor, retrieve it, clear it.
  - Concurrency guard: two set calls replace previous executor without panicking.

### Step 6: Testing
- [ ] Unit tests for each module
- [ ] Integration test: full flow from groups → transactions
- [ ] Test dependency resolution (group 2 waits for group 1)
- [ ] Test parallel execution (independent groups)

### Step 7: Deprecation
- [ ] Mark `forge_script_builder.rs` as deprecated
- [ ] Update documentation
- [ ] Migration guide for existing code

---

## Dependencies

### Crates
- `baml_client` - Generated BAML client
- `foundry_config`, `foundry_evm` - Script execution
- `tokio` - Async runtime
- `serde`, `serde_json` - Serialization
- `anyhow` - Error handling

### Environment Variables
- `BAML_API_URL` - BAML server (default: `http://localhost:2024`)
- `AOMI_FORK_RPC` - Ethereum fork RPC URL

---

## Open Questions

1. **State persistence**: Where to store `ForgeExecutor` between tool calls?
   - Option A: In `ForgeApp` context
   - Option B: Serialize and pass back to agent
   - **Recommendation**: Option A for simplicity

2. **Source fetching**: How to get ABIs and source code?
   - Phase 1: Placeholder implementation
   - Phase 2: Integrate with existing DB or Etherscan tool

3. **Parallel execution**: Should `next_group()` support parallel execution?
   - Current design: Sequential
   - Future: Return multiple ready groups, agent calls tools in parallel

4. **Transaction signing**: Where does user approval happen?
   - Current: Return transactions to agent, agent calls wallet tool
   - Future: Built-in approval flow?

---

## Timeline Estimate

| Phase | Estimated Time |
|-------|---------------|
| BAML schema updates | 2 hours |
| BAML module implementation | 3 hours |
| Forge executor module | 4 hours |
| Tool integration | 2 hours |
| Testing & debugging | 4 hours |
| **Total** | **~15 hours** |
