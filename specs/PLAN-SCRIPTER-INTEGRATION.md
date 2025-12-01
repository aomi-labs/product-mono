# Integration Testing Plan: Scripter (Forge) Module

## Overview

Create integration tests for the Forge/Scripter functionality following the existing eval module patterns from `test_entry.rs`. The goal is to test that the ForgeApp agent can:
1. Understand user intents for smart contract deployment/interaction
2. Call the `ForgeScriptBuilder` tool correctly
3. Generate and simulate valid Forge scripts

---

## Phase 0: Merge feat/eval-part2 into mono-be-foundry

### 0.1 Why Merge First?

The `feat/eval-part2` branch (in `forge-mcp-backend` repo) has significant improvements to the eval framework:
- **`EvalCase`** - Builder pattern for test cases with expectations and assertions
- **`assertions.rs`** - Deterministic on-chain assertion framework (772 lines)
- **Updated `Harness`** - Accepts any `Arc<BackendwithTool>`, not just ChatApp
- **USDC prefunding** - Whale impersonation for test accounts
- **Improved test suite** - Basic, DeFi, Lido, Aave test categories

### 0.2 Conflict Analysis

**Common ancestor**: `95e5799` (both branches diverged from same main commit)

#### Files with Overlapping Changes (Potential Conflicts)

| Category | Files | Conflict Risk |
|----------|-------|---------------|
| **Eval Crate** | `harness.rs`, `eval_state.rs`, `eval_app.rs`, `lib.rs`, `test_entry.rs` | HIGH - Both branches modified |
| **Tools Crate** | `lib.rs`, `scheduler.rs`, `cast.rs`, `clients.rs` | MEDIUM - Need to merge exports |
| **Backend Crate** | `session.rs`, `manager.rs`, `history.rs` | MEDIUM - API changes |
| **Chat Crate** | `app.rs`, `prompts.rs`, `completion.rs` | MEDIUM |
| **Cargo.toml files** | workspace, tools, eval, backend | LOW - Additive changes |

#### Unique Files (No Conflicts)

**mono-be-foundry only:**
- `aomi/crates/forge/` (entire new crate)
- `aomi/crates/tools/src/forge_script_builder.rs`
- `aomi/crates/tools/src/contract/` (entire new module with forge-std lib)

**feat/eval-part2 only:**
- `aomi/crates/eval/src/assertions.rs`

### 0.3 Merge Strategy

**Recommended approach**: Merge `feat/eval-part2` INTO `mono-be-foundry`

```bash
# On mono-be-foundry branch
git fetch origin
git merge origin/feat/eval-part2 --no-commit

# Resolve conflicts, then
git add .
git commit -m "Merge feat/eval-part2 into mono-be-foundry"
```

### 0.4 Conflict Resolution Guide

#### eval/src/harness.rs
- **Keep from eval-part2**: `EvalCase`, assertions integration, `default_with_cases()`, USDC prefunding
- **Add new**: `for_scripter()` helper function (see Phase 2)

#### eval/src/lib.rs
- **Keep from eval-part2**: `assertions` module export
- **Add new**: `test_scripter` module export

#### tools/src/lib.rs
- **Keep from eval-part2**: Any new tool exports
- **Add from mono-be-foundry**: `ForgeScriptBuilder`, `contract` module exports

#### tools/Cargo.toml
- **Merge both**: Combine dependencies from both branches (foundry deps + eval deps)

#### backend/src/session.rs
- **Primary**: Use eval-part2 version (has `BackendwithTool` type alias used by Harness)
- **Verify**: `ChatApp` still implements `AomiBackend` trait

---

## Phase 1: Complete ForgeApp Setup

### 1.1 Verify Tool Registration (DONE - already in app.rs:79)
The `ForgeScriptBuilder` tool is already registered in `ForgeApp::init_internal`:
```rust
builder.add_tool(aomi_tools::ForgeScriptBuilder)?;
```

### 1.2 Required Additional Tools for Full Workflow
The ForgeApp may need additional tools for a complete workflow:
- `GetContractFromEtherscan` - to fetch contract ABIs when needed
- `GetContractABI` - to lookup stored contract ABIs

**Action**: Check if these tools should be added to ForgeApp for end-to-end testing, or if the test cases should be self-contained with inline interfaces.

---

## Phase 2: Add Helper Functions to Existing Harness

### 2.1 Add `for_scripter()` to Harness

Instead of creating a separate harness, add a helper function to the existing `Harness`:

```rust
// In aomi/crates/eval/src/harness.rs

impl Harness {
    /// Create a harness with ForgeApp backend for scripter/forge tests.
    /// Unlike default_with_cases, this:
    /// - Uses ForgeApp instead of ChatApp
    /// - Does NOT prefund USDC (scripts are simulated, not broadcast)
    /// - Uses forge-specific preamble
    pub async fn for_scripter(cases: Vec<EvalCase>, max_round: usize) -> Result<Self> {
        ensure_anvil_network_configured();
        // Note: No USDC prefund - forge scripts are simulated, not executed on-chain

        let eval_app = EvaluationApp::headless().await?;

        // Use ForgeApp instead of ChatApp
        let forge_app = aomi_forge::ForgeApp::new_with_options(true, true)
            .await
            .map_err(|e| anyhow!("Failed to create ForgeApp: {}", e))?;

        // ForgeApp wraps ChatApp, which implements AomiBackend
        let backend: Arc<BackendwithTool> = Arc::new(forge_app.into_chat_app());

        Self::new(eval_app, backend, cases, max_round)
    }
}
```

### 2.2 Add `into_chat_app()` to ForgeApp

```rust
// In aomi/crates/forge/src/app.rs

impl ForgeApp {
    /// Consume ForgeApp and return the inner ChatApp for use as BackendwithTool
    pub fn into_chat_app(self) -> ChatApp {
        self.chat_app
    }
}
```

### 2.3 Add `has_tool_call()` Helper to TestResult

```rust
// In aomi/crates/eval/src/lib.rs

impl TestResult {
    /// Check if any round called a specific tool
    pub fn has_tool_call(&self, tool_name: &str) -> bool {
        self.rounds.iter().any(|r| {
            r.actions.iter().any(|a| {
                matches!(a, AgentAction::ToolCall(tc) if tc.topic == tool_name)
            })
        })
    }

    /// Get all tool call results for a specific tool
    pub fn get_tool_calls(&self, tool_name: &str) -> Vec<&ToolCall> {
        self.rounds
            .iter()
            .flat_map(|r| r.actions.iter())
            .filter_map(|a| match a {
                AgentAction::ToolCall(tc) if tc.topic == tool_name => Some(tc),
                _ => None,
            })
            .collect()
    }
}
```

---

## Phase 3: Create Test Entry File

### 3.1 File Location
Create: `aomi/crates/eval/src/test_scripter.rs`

### 3.2 Test Cases

#### Test Case 1: Simple ETH Transfer Script
**Intent**: "Create a script to send 1 ETH to address 0x1234567890123456789012345678901234567890"

**Expectation**:
- Agent calls `ForgeScriptBuilder` with transfer operation
- Script compiles successfully
- Script simulation returns 1 broadcastable transaction
- Transaction has correct `to` address and `value`

```rust
use std::rc::Rc;
use anyhow::Result;
use crate::harness::{EvalCase, Harness};

fn skip_if_missing_keys() -> Result<bool> {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        println!("Skipping scripter tests: ANTHROPIC_API_KEY not set");
        return Ok(true);
    }
    if std::env::var("BAML_API_URL").is_err() &&
       !check_baml_server_running() {
        println!("Skipping scripter tests: BAML server not available");
        return Ok(true);
    }
    Ok(false)
}

fn check_baml_server_running() -> bool {
    // Quick health check for BAML server at default URL
    std::net::TcpStream::connect("127.0.0.1:2024").is_ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Scripter eval requires BAML server and ANTHROPIC_API_KEY"]
async fn test_simple_eth_transfer_script() -> Result<()> {
    if skip_if_missing_keys()? {
        return Ok(());
    }

    let cases = vec![
        EvalCase::new("Create a forge script to send 1 ETH to 0x1234567890123456789012345678901234567890")
            .with_expectation("A valid Forge script was generated that transfers 1 ETH to the specified address."),
    ];

    let harness = Rc::new(Harness::for_scripter(cases.clone(), 3).await?);
    let results = harness.run_suites().await?;

    // Verify ForgeScriptBuilder was called
    for result in &results {
        assert!(
            result.has_tool_call("ForgeScriptBuilder"),
            "Expected ForgeScriptBuilder tool call, got: {:?}",
            result.rounds
        );
    }

    let verdicts = harness.verify_case_expectations().await?;
    for (i, verdict) in verdicts.iter().enumerate() {
        if let Some(v) = verdict {
            assert!(v.satisfied, "Test {}: {}", i, v.explanation);
        }
    }

    harness.flush()?;
    Ok(())
}
```

#### Test Case 2: ERC20 Token Approval Script

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Scripter eval requires BAML server and ANTHROPIC_API_KEY"]
async fn test_erc20_approval_script() -> Result<()> {
    if skip_if_missing_keys()? {
        return Ok(());
    }

    let cases = vec![
        EvalCase::new("Create a forge script to approve 1000 USDC for the Uniswap V2 router on mainnet. Use the IERC20 interface from forge-std.")
            .with_expectation("A valid Forge script was generated with an ERC20 approve call for USDC."),
    ];

    let harness = Rc::new(Harness::for_scripter(cases.clone(), 4).await?);
    let results = harness.run_suites().await?;

    // Verify ForgeScriptBuilder was called
    for result in &results {
        assert!(
            result.has_tool_call("ForgeScriptBuilder"),
            "Expected ForgeScriptBuilder tool call"
        );
    }

    let verdicts = harness.verify_case_expectations().await?;
    for (i, verdict) in verdicts.iter().enumerate() {
        if let Some(v) = verdict {
            assert!(v.satisfied, "Test {}: {}", i, v.explanation);
        }
    }

    harness.flush()?;
    Ok(())
}
```

---

## Phase 4: Implementation Steps

### Step 1: Merge feat/eval-part2
```bash
cd /Users/ceciliazhang/Code/aomi-product
git fetch origin
git merge origin/feat/eval-part2 --no-commit
# Resolve conflicts per 0.4 guide
git add .
git commit -m "Merge feat/eval-part2 into mono-be-foundry for improved eval harness"
```

### Step 2: Add ForgeApp Dependency to eval crate
Update `aomi/crates/eval/Cargo.toml`:
```toml
[dependencies]
aomi-forge.workspace = true
```

### Step 3: Add into_chat_app() to ForgeApp
Edit `aomi/crates/forge/src/app.rs`

### Step 4: Add for_scripter() to Harness
Edit `aomi/crates/eval/src/harness.rs`

### Step 5: Add helper methods to TestResult
Edit `aomi/crates/eval/src/lib.rs`

### Step 6: Add test_scripter module
Edit `aomi/crates/eval/src/lib.rs`:
```rust
#[cfg(test)]
#[cfg(feature = "eval-test")]
pub mod test_scripter;
```

### Step 7: Create test_scripter.rs
Create `aomi/crates/eval/src/test_scripter.rs` with test cases

### Step 8: Verify compilation and run tests
```bash
cargo check --package aomi-eval --features eval-test
cargo clippy --package aomi-eval --features eval-test
cargo test --package aomi-eval --features eval-test test_scripter -- --ignored --nocapture
```

---

## Phase 5: Verification Checklist

### Pre-Integration Requirements
- [ ] Merge `feat/eval-part2` into `mono-be-foundry` completed
- [ ] All conflicts resolved
- [ ] `cargo check` passes
- [ ] BAML server running at `http://localhost:2024`
- [ ] `ANTHROPIC_API_KEY` environment variable set
- [ ] Foundry installed locally for script compilation
- [ ] (Optional) `AOMI_FORK_RPC` for mainnet forking

### Test Execution
```bash
# Start BAML server first
cd aomi/crates/l2beat && docker-compose up -d baml

# Run scripter tests specifically
ANTHROPIC_API_KEY=<key> cargo test --package aomi-eval --features eval-test test_scripter -- --ignored --nocapture

# With debug output
DEBUG=1 ANTHROPIC_API_KEY=<key> cargo test --package aomi-eval --features eval-test test_scripter -- --ignored --nocapture
```

### Expected Outputs
1. Agent conversation logs showing intent understanding
2. ForgeScriptBuilder tool calls with structured operations
3. Generated Solidity script output
4. Simulation results with broadcastable transactions

---

## Dependencies

### Required Crates in eval/Cargo.toml
```toml
aomi-forge.workspace = true
```

### Feature Flags
The `eval-test` feature should enable:
- Test modules compilation
- Extended debug logging

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Merge conflicts | Follow conflict resolution guide in 0.4 |
| BAML server not running | Skip test with clear message |
| Anthropic API rate limits | Use `#[ignore]` attribute, run manually |
| Script compilation failures | Detailed error messages, fallback assertions |
| Flaky LLM responses | Multiple rounds (max_round = 3-4), verify tool calls not just final output |
| ForgeApp doesn't implement AomiBackend | Use `into_chat_app()` to extract inner ChatApp |

---

## Success Criteria

1. **Merge completed**: No remaining conflicts, `cargo check` passes
2. **Test Case 1 (ETH Transfer)**:
   - ForgeScriptBuilder called with correct parameters
   - Script compiles without errors
   - 1 broadcastable transaction extracted
3. **Test Case 2 (ERC20 Approval)**:
   - Agent identifies contract addresses
   - ForgeScriptBuilder called with approve operation
   - Script includes necessary interfaces
   - Script compiles and simulates successfully
