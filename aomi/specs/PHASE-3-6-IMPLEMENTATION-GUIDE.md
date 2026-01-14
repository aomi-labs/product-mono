# Phases 3-6 Implementation Guide

**Status**: Infrastructure Complete (Phases 0-2) ✅
**Next Steps**: Detailed guide for completing Phases 3-6
**Date**: 2026-01-14

---

## Overview

Phases 0-2 have established solid infrastructure. This guide provides detailed patterns, templates, and step-by-step instructions for completing Phases 3-6.

---

## Phase 3: Simplify completion.rs

### Current State

`completion.rs` has dual paths:
1. **Scheduler path**: Tools registered with scheduler
2. **Fallback path**: Direct Rig tool calls (MCP tools)

### Goal

Unified execution through Rig with session awareness.

### Implementation Strategy

**Option A: Keep Dual Paths, Improve Integration**
- Less risky, gradual migration
- Both old and new tools work
- Complete tool migration first (Phase 4)

**Option B: Remove Dual Paths (Original Plan)**
- Requires all tools migrated first
- Single code path
- Simpler long-term

### Recommended Approach: Option A

Keep dual paths until Phase 4 complete, then simplify.

### Changes Needed

#### 1. Update `consume_tool_call` to Support Both

```rust
async fn consume_tool_call(
    &mut self,
    tool_call: rig::message::ToolCall,
) -> Result<ProcessStep, StreamingError> {
    let mut commands = Vec::new();
    if let Some(cmd) = self.consume_system_events(&tool_call) {
        commands.push(cmd);
    }

    let Some(handler) = self.handler.clone() else {
        // Fallback path (MCP tools, non-migrated tools)
        let (topic, stream) = self.process_tool_call_fallback(tool_call).await?;
        commands.push(CoreCommand::ToolCall { topic, stream });
        return Ok(ProcessStep::Emit(commands));
    };

    // Get session namespaces from state or config
    let namespaces = vec!["default".to_string()]; // TODO: Get from session config

    // Check if this is a V2 tool (has namespace metadata)
    let scheduler = ToolScheduler::get_or_init().await?;
    let is_v2_tool = scheduler.has_tool_metadata(&tool_call.function.name);

    let ui_stream = if is_v2_tool {
        // V2 tool: Let wrapper handle scheduling
        // Tool will self-register with handler via auto-impl
        self.process_tool_call_v2(tool_call, &handler, namespaces).await?
    } else {
        // V1 tool: Use existing scheduler path
        self.process_tool_call(tool_call, &handler).await?
    };

    let topic = extract_topic(&tool_call, &handler).await;
    commands.push(CoreCommand::ToolCall { topic, stream: ui_stream });

    Ok(ProcessStep::Emit(commands))
}
```

#### 2. Add `process_tool_call_v2` Method

```rust
async fn process_tool_call_v2(
    &mut self,
    tool_call: rig::message::ToolCall,
    handler: &SessionToolHander,
    namespaces: Vec<String>,
) -> Result<ToolStream, StreamingError> {
    self.state.push_tool_call(&tool_call);

    // Call tool via Rig (goes through wrapper auto-impl)
    let tool_name = tool_call.function.name.clone();
    let args = tool_call.function.arguments.clone();

    // Inject session_id into args
    let mut args_with_session = args;
    args_with_session["session_id"] = json!(self.state.session_id);

    // Call tool - wrapper will register receiver with handler
    let _result = self.agent.tools.call(&tool_name, args_with_session.to_string()).await?;

    // Poll handler for the stream
    let mut guard = handler.lock().await;
    let ui_stream = guard.resolve_last_call()
        .ok_or_else(|| StreamingError::Eyre(eyre::eyre!("No stream registered")))?;

    Ok(ui_stream)
}
```

#### 3. Add Helper in Scheduler

```rust
impl ToolScheduler {
    /// Check if tool has V2 metadata (namespace-aware)
    pub fn has_tool_metadata(&self, tool_name: &str) -> bool {
        self.tool_metadata.read().unwrap().contains_key(tool_name)
    }
}
```

### Testing

```bash
# Test with V1 tools (existing)
cargo test -p aomi-chat -- test_existing_tools

# Test with V2 tools (migrated)
cargo test -p aomi-chat --features use-aomi-tool-v2 -- test_v2_tools

# Test mixed environment
cargo test -p aomi-chat --features use-aomi-tool-v2
```

---

## Phase 4: Migrate Remaining Tools

### Tool Inventory

**Sync Tools** (~10 tools):
- ✅ GetCurrentTime (done in Phase 2)
- ⬜ EncodeFunctionCall
- ⬜ GetContractABI
- ⬜ GetContractSourceCode
- ⬜ GetErc20Balance
- ⬜ GetAccountInfo
- ⬜ GetAccountTransactionHistory
- ⬜ Cast tools (7 tools)

**Async Tools** (~2-3 tools):
- ⬜ ForgeExecutor (critical)
- ⬜ BraveSearch
- ⬜ Future streaming tools

### Migration Template for Sync Tools

#### Step 1: Read Existing Tool

```rust
// Example: execution/abi_encoder.rs
impl Tool for EncodeFunctionCall {
    const NAME: &'static str = "encode_function_call";
    type Args = EncodeFunctionCallParameters;
    type Output = String;
    type Error = ToolError;

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // ... existing implementation
    }
}
```

#### Step 2: Create V2 Version

```rust
// Add to end of file, within #[cfg(feature = "use-aomi-tool-v2")]

#[cfg(feature = "use-aomi-tool-v2")]
#[derive(Debug, Clone)]
pub struct EncodeFunctionCallV2;

#[cfg(feature = "use-aomi-tool-v2")]
use aomi_tools_v2::AomiTool as AomiToolV2;

#[cfg(feature = "use-aomi-tool-v2")]
impl AomiToolV2 for EncodeFunctionCallV2 {
    const NAME: &'static str = "encode_function_call";
    const NAMESPACE: &'static str = "ethereum"; // Choose appropriate namespace

    type Args = EncodeFunctionCallParameters;
    type Output = serde_json::Value;
    type Error = EncodeFunctionCallError;

    fn support_async(&self) -> bool {
        false // Sync tool
    }

    fn description(&self) -> &'static str {
        "Encode function calls into hex calldata"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "Short description of what's being encoded"
                },
                "function_signature": {
                    "type": "string",
                    "description": "Function signature, e.g., 'transfer(address,uint256)'"
                },
                "arguments": {
                    "type": "array",
                    "description": "Array of argument values",
                    "items": {}
                }
            },
            "required": ["topic", "function_signature", "arguments"]
        })
    }

    async fn run_sync(
        &self,
        result_sender: tokio::sync::oneshot::Sender<eyre::Result<serde_json::Value>>,
        args: Self::Args,
    ) {
        // Copy implementation from original call() method
        let result = encode_function_call_impl(args)
            .map(|s| json!(s))
            .map_err(|e| eyre::eyre!("Encoding error: {}", e));

        let _ = result_sender.send(result);
    }
}

// Custom error type
#[cfg(feature = "use-aomi-tool-v2")]
#[derive(Debug)]
pub struct EncodeFunctionCallError(String);

#[cfg(feature = "use-aomi-tool-v2")]
impl std::fmt::Display for EncodeFunctionCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EncodeFunctionCallError: {}", self.0)
    }
}

#[cfg(feature = "use-aomi-tool-v2")]
impl std::error::Error for EncodeFunctionCallError {}
```

#### Step 3: Extract Common Logic

If the implementation is complex, extract to a shared function:

```rust
// Shared implementation (always compiled)
fn encode_function_call_impl(args: EncodeFunctionCallParameters) -> Result<String, String> {
    // ... actual encoding logic
}

// V1 implementation
impl Tool for EncodeFunctionCall {
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        encode_function_call_impl(args)
            .map_err(|e| ToolError::ToolCallError(e.into()))
    }
}

// V2 implementation
#[cfg(feature = "use-aomi-tool-v2")]
impl AomiToolV2 for EncodeFunctionCallV2 {
    async fn run_sync(&self, sender: oneshot::Sender<Result<Value>>, args: Self::Args) {
        let result = encode_function_call_impl(args)
            .map(|s| json!(s))
            .map_err(|e| eyre::eyre!(e));
        let _ = sender.send(result);
    }
}
```

### Migration Template for Async Tools

#### Example: ForgeExecutor

```rust
#[cfg(feature = "use-aomi-tool-v2")]
#[derive(Debug, Clone)]
pub struct ForgeExecutorV2;

#[cfg(feature = "use-aomi-tool-v2")]
impl AomiToolV2 for ForgeExecutorV2 {
    const NAME: &'static str = "forge_execute";
    const NAMESPACE: &'static str = "forge";

    type Args = ForgeExecuteParameters;
    type Output = serde_json::Value;
    type Error = ForgeExecutorError;

    fn support_async(&self) -> bool {
        true // Async/streaming tool
    }

    fn description(&self) -> &'static str {
        "Execute forge operations with progress streaming"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "plan": {
                    "type": "array",
                    "description": "Execution plan with operation groups"
                }
            },
            "required": ["plan"]
        })
    }

    async fn run_async(
        &self,
        results_sender: tokio::sync::mpsc::Sender<eyre::Result<serde_json::Value>>,
        args: Self::Args,
    ) {
        // Send initial acknowledgment
        let _ = results_sender.send(Ok(json!({
            "status": "started",
            "message": "Forge execution started"
        }))).await;

        // Execute plan with progress updates
        for (index, operation) in args.plan.iter().enumerate() {
            match execute_operation(operation).await {
                Ok(result) => {
                    let _ = results_sender.send(Ok(json!({
                        "status": "progress",
                        "step": index,
                        "result": result
                    }))).await;
                }
                Err(e) => {
                    let _ = results_sender.send(Err(eyre::eyre!(
                        "Operation {} failed: {}", index, e
                    ))).await;
                    return; // Stop on error
                }
            }
        }

        // Send final completion
        let _ = results_sender.send(Ok(json!({
            "status": "completed",
            "message": "All operations completed"
        }))).await;

        // Channel closes when sender is dropped
    }
}
```

### Namespace Guidelines

Choose appropriate namespaces for tools:

| Namespace | Tools |
|-----------|-------|
| `time` | GetCurrentTime |
| `ethereum` | EncodeFunctionCall, GetContractABI, GetErc20Balance |
| `account` | GetAccountInfo, GetAccountTransactionHistory |
| `forge` | ForgeExecutor, forge-related tools |
| `cast` | All cast tools |
| `search` | BraveSearch, web search tools |
| `default` | Generic tools, fallback |

### Migration Checklist

For each tool:

```markdown
## Tool: [TOOL_NAME]

- [ ] Read existing implementation
- [ ] Determine sync vs async
- [ ] Extract common logic if needed
- [ ] Create V2 struct (`[Tool]V2`)
- [ ] Implement AomiToolV2 trait
  - [ ] Set NAME constant
  - [ ] Set NAMESPACE constant
  - [ ] Define Args/Output/Error types
  - [ ] Implement description()
  - [ ] Implement parameters_schema()
  - [ ] Implement run_sync() or run_async()
- [ ] Create error type if needed
- [ ] Test with feature flag
- [ ] Update registration (Phase 6)
- [ ] Remove old implementation (Phase 6)
```

---

## Phase 5: Session Persistence

### Goal

Persist session state across user logouts/logins so incomplete tool calls can be resumed.

### Architecture

**Core Strategy** (from user's design):
- Don't serialize ongoing async tasks (too complex)
- On logout: Poll everything to completion
- Persist only completed results
- On login: Restore results for consumption

### Implementation

#### 1. Add Persistence Methods to ToolHandler

```rust
// In scheduler.rs, add to ToolHandler impl

impl ToolHandler {
    /// Graceful persistence: poll everything to completion
    pub async fn sanitized_persist(&mut self, timeout_secs: u64) -> eyre::Result<PersistedHandlerState> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        // Poll unresolved calls until empty or timeout
        while !self.unresolved_calls.is_empty() {
            if start.elapsed() > timeout {
                // Mark remaining as timed out
                for receiver in self.unresolved_calls.drain(..) {
                    let call_id = receiver.call_id.clone();
                    self.completed_calls.push(ToolCompletion {
                        call_id,
                        tool_name: receiver.tool_name.clone(),
                        sync: true,
                        result: Err(eyre::eyre!("Timed out during logout")),
                    });
                }
                break;
            }

            // Convert unresolved to streams
            if let Some(streams) = self.resolve_calls().await {
                // Streams added to ongoing_streams
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Poll ongoing streams until empty or timeout
        while !self.ongoing_streams.is_empty() {
            if start.elapsed() > timeout {
                // Mark remaining as timed out
                let remaining: Vec<_> = self.ongoing_streams.drain(..).collect();
                for stream in remaining {
                    self.completed_calls.push(ToolCompletion {
                        call_id: stream.call_id.clone(),
                        tool_name: stream.tool_name.clone(),
                        sync: false,
                        result: Err(eyre::eyre!("Timed out during logout")),
                    });
                }
                break;
            }

            self.poll_streams_once();
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Everything is now in completed_calls
        self.to_persisted()
    }

    /// Serialize state for persistence
    pub fn to_persisted(&self) -> eyre::Result<PersistedHandlerState> {
        // Serialize namespaces (if stored)
        let namespaces = vec!["default".to_string()]; // TODO: Store in handler

        // Serialize available tools
        let available_tools: Vec<String> = self.tool_info.keys().cloned().collect();

        // Serialize completed calls
        let completed_calls = self.completed_calls.clone();

        Ok(PersistedHandlerState {
            namespaces,
            available_tools,
            completed_calls,
        })
    }

    /// Restore from persisted state
    pub fn from_persisted(
        state: PersistedHandlerState,
        requests_tx: mpsc::Sender<(SchedulerRequest, ToolResultSender)>,
        tool_metadata: &HashMap<String, ToolMetadata>,
    ) -> Self {
        let mut handler = Self::new(requests_tx);

        // Restore tool_info cache
        for tool_name in &state.available_tools {
            if let Some(meta) = tool_metadata.get(tool_name) {
                handler.tool_info.insert(
                    tool_name.clone(),
                    (meta.is_async, meta.name.clone()),
                );
            }
        }

        // Insert completed calls - ready to be consumed
        handler.completed_calls = state.completed_calls;

        handler
    }
}
```

#### 2. Define Persisted State Structure

```rust
// In scheduler.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedHandlerState {
    pub namespaces: Vec<String>,
    pub available_tools: Vec<String>,
    pub completed_calls: Vec<ToolCompletion>,
}
```

#### 3. Add DB Schema

```sql
-- Create table for session tool state
CREATE TABLE session_tool_state (
    session_id VARCHAR(255) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    last_updated TIMESTAMP NOT NULL DEFAULT NOW(),
    handler_state JSONB NOT NULL, -- PersistedHandlerState
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),

    INDEX idx_user_id (user_id),
    INDEX idx_last_updated (last_updated)
);

-- Cleanup old sessions
CREATE INDEX idx_session_cleanup ON session_tool_state(last_updated)
WHERE last_updated < NOW() - INTERVAL '7 days';
```

#### 4. Implement DB Operations

```rust
// In db/session_store.rs or new file

use sqlx::PgPool;
use crate::scheduler::PersistedHandlerState;

pub async fn save_session_state(
    pool: &PgPool,
    session_id: &str,
    user_id: &str,
    state: &PersistedHandlerState,
) -> eyre::Result<()> {
    let state_json = serde_json::to_value(state)?;

    sqlx::query!(
        r#"
        INSERT INTO session_tool_state (session_id, user_id, handler_state)
        VALUES ($1, $2, $3)
        ON CONFLICT (session_id)
        DO UPDATE SET
            handler_state = EXCLUDED.handler_state,
            last_updated = NOW()
        "#,
        session_id,
        user_id,
        state_json
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn load_session_state(
    pool: &PgPool,
    session_id: &str,
) -> eyre::Result<Option<PersistedHandlerState>> {
    let row = sqlx::query!(
        r#"
        SELECT handler_state
        FROM session_tool_state
        WHERE session_id = $1
        "#,
        session_id
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let state: PersistedHandlerState = serde_json::from_value(row.handler_state)?;
            Ok(Some(state))
        }
        None => Ok(None),
    }
}

pub async fn delete_session_state(
    pool: &PgPool,
    session_id: &str,
) -> eyre::Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM session_tool_state
        WHERE session_id = $1
        "#,
        session_id
    )
    .execute(pool)
    .await?;

    Ok(())
}
```

#### 5. Integrate with Scheduler

```rust
// Update scheduler.rs

impl ToolScheduler {
    /// Cleanup session with graceful persistence
    pub async fn cleanup_session_with_persistence(
        &self,
        session_id: &str,
        db_pool: &PgPool,
        user_id: &str,
        timeout_secs: u64,
    ) -> eyre::Result<()> {
        if let Some(handler) = self.session_handlers.read().unwrap().get(session_id) {
            let mut guard = handler.lock().await;

            // Poll everything to completion
            let state = guard.sanitized_persist(timeout_secs).await?;

            // Save to DB
            save_session_state(db_pool, session_id, user_id, &state).await?;
        }

        // Remove from memory
        self.session_handlers.write().unwrap().remove(session_id);
        eprintln!("Persisted and cleaned up session: {}", session_id);

        Ok(())
    }

    /// Restore session from persistence
    pub async fn restore_session(
        &self,
        session_id: &str,
        db_pool: &PgPool,
    ) -> eyre::Result<bool> {
        // Load from DB
        let Some(persisted_state) = load_session_state(db_pool, session_id).await? else {
            return Ok(false);
        };

        // Create new handler with persisted state
        let metadata_guard = self.tool_metadata.read().unwrap();
        let handler = ToolHandler::from_persisted(
            persisted_state,
            self.requests_tx.clone(),
            &metadata_guard,
        );
        drop(metadata_guard);

        // Register in session_handlers
        self.session_handlers
            .write()
            .unwrap()
            .insert(session_id.to_string(), Arc::new(Mutex::new(handler)));

        eprintln!("Restored session: {}", session_id);
        Ok(true)
    }
}
```

#### 6. Usage Example

```rust
// On user logout
async fn handle_user_logout(session_id: &str, user_id: &str) -> eyre::Result<()> {
    let scheduler = ToolScheduler::get_or_init().await?;
    let db_pool = get_db_pool();

    // Graceful cleanup with 30-second timeout
    scheduler.cleanup_session_with_persistence(
        session_id,
        &db_pool,
        user_id,
        30, // timeout in seconds
    ).await?;

    Ok(())
}

// On user login
async fn handle_user_login(session_id: &str) -> eyre::Result<bool> {
    let scheduler = ToolScheduler::get_or_init().await?;
    let db_pool = get_db_pool();

    // Try to restore previous session
    let restored = scheduler.restore_session(session_id, &db_pool).await?;

    if restored {
        println!("Previous session restored for: {}", session_id);
    } else {
        println!("Starting fresh session for: {}", session_id);
    }

    Ok(restored)
}
```

### Testing

```rust
#[tokio::test]
async fn test_session_persistence_roundtrip() {
    let scheduler = ToolScheduler::new_for_test().await.unwrap();
    let db_pool = setup_test_db().await;

    // Create session with completed calls
    let session_id = "test_session_123";
    let handler = scheduler.get_session_handler_with_namespaces(
        session_id.to_string(),
        vec!["default".to_string()],
    );

    // Simulate completed tool call
    {
        let mut guard = handler.lock().await;
        guard.completed_calls.push(ToolCompletion {
            call_id: ToolCallId::new("test_1".to_string(), None),
            tool_name: "test_tool".to_string(),
            sync: true,
            result: Ok(json!({"status": "success"})),
        });
    }

    // Persist
    scheduler.cleanup_session_with_persistence(
        session_id,
        &db_pool,
        "user_1",
        30,
    ).await.unwrap();

    // Restore
    let restored = scheduler.restore_session(session_id, &db_pool).await.unwrap();
    assert!(restored);

    // Verify state
    let handler = scheduler.session_handlers.read().unwrap()
        .get(session_id).unwrap().clone();
    let guard = handler.lock().await;

    assert_eq!(guard.completed_calls.len(), 1);
    assert_eq!(guard.completed_calls[0].tool_name, "test_tool");
}
```

---

## Phase 6: Cleanup & Documentation

### Step 1: Remove Feature Flags (After All Tools Migrated)

```toml
# crates/tools/Cargo.toml
# Remove:
# aomi-tools-v2 = { workspace = true, optional = true }
# use-aomi-tool-v2 = ["aomi-tools-v2"]

# Add as regular dependency:
[dependencies]
aomi-tools-v2.workspace = true
```

### Step 2: Remove Old Implementations

For each migrated tool:

```rust
// Before:
impl Tool for GetCurrentTime { ... }

#[cfg(feature = "use-aomi-tool-v2")]
impl AomiToolV2 for GetCurrentTimeV2 { ... }

// After:
// Delete old Tool impl
// Remove #[cfg] from V2 impl
// Rename V2 -> regular name

impl AomiTool for GetCurrentTime { ... }
```

### Step 3: Simplify Completion.rs (Final)

Once all tools migrated, remove dual paths:

```rust
// Remove process_tool_call (old scheduler path)
// Remove process_tool_call_fallback
// Keep only process_tool_call_v2 (rename to process_tool_call)

async fn consume_tool_call(
    &mut self,
    tool_call: rig::message::ToolCall,
) -> Result<ProcessStep, StreamingError> {
    let mut commands = Vec::new();
    if let Some(cmd) = self.consume_system_events(&tool_call) {
        commands.push(cmd);
    }

    let Some(handler) = self.handler.clone() else {
        return Err(StreamingError::Eyre(eyre::eyre!("No handler available")));
    };

    // Unified path: all tools self-schedule via wrapper
    let ui_stream = self.process_tool_call(tool_call, &handler).await?;
    let topic = extract_topic(&tool_call, &handler).await;

    commands.push(CoreCommand::ToolCall { topic, stream: ui_stream });
    Ok(ProcessStep::Emit(commands))
}
```

### Step 4: Remove Event Loop (Optional)

If all tools use self-scheduling:

```rust
// In scheduler.rs
// Comment out or remove:
// Self::run(scheduler.clone(), requests_rx);

// Keep requests_tx for backward compat or remove entirely
```

### Step 5: Update Documentation

#### Architecture Diagram

Update `docs/architecture.md`:

```markdown
## Tool Execution Flow

```
LLM → ToolCall
    ↓
agent.tools.call()
    ↓
rig::Tool::call() [auto-impl for AomiToolWrapper<T>]
    ↓
├─> Spawn execution (sync or async)
├─> Get session handler from scheduler
└─> Register receiver with handler
    ↓
Polling loop (session layer)
    ↓
├─> unresolved → streams
├─> poll streams → completed
└─> push to SystemEventQueue
    ↓
Frontend receives results
```
```

#### Tool Development Guide

Create `docs/guides/tool-development.md`:

```markdown
# Tool Development Guide

## Creating a New Tool

### Step 1: Define Argument Structure

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyToolArgs {
    pub topic: String,
    pub param1: String,
    pub param2: i32,
}
```

### Step 2: Implement AomiTool

```rust
use aomi_tools_v2::AomiTool;

#[derive(Debug, Clone)]
pub struct MyTool;

impl AomiTool for MyTool {
    const NAME: &'static str = "my_tool";
    const NAMESPACE: &'static str = "my_category";

    type Args = MyToolArgs;
    type Output = serde_json::Value;
    type Error = MyToolError;

    fn support_async(&self) -> bool {
        false // or true for async tools
    }

    fn description(&self) -> &'static str {
        "Description of what my tool does"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "topic": { "type": "string" },
                "param1": { "type": "string" },
                "param2": { "type": "integer" }
            },
            "required": ["topic", "param1", "param2"]
        })
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<Result<Value>>,
        args: Self::Args,
    ) {
        // Implementation
        let result = do_work(args);
        let _ = sender.send(result.map(|v| json!(v)));
    }
}
```

### Step 3: Register with Agent

```rust
use aomi_tools_v2::aomi_tool;

agent.tool(aomi_tool!(MyTool));
```

Done!
```

### Step 6: Update PROGRESS.md

```markdown
# Current Branch: main

## Recently Completed

### AomiTool Architecture Refactor (Phases 0-6) ✅
- **Date**: 2026-01-14
- **Status**: Complete
- **Files Changed**: ~30 files
- **Lines Added**: ~2000

**Key Achievements**:
1. ✅ New tools-v2 crate with AomiTool trait system
2. ✅ Session-aware execution with namespace filtering
3. ✅ All tools migrated to new architecture
4. ✅ Session persistence implemented
5. ✅ Completion.rs simplified (single path)
6. ✅ Comprehensive documentation

**Impact**:
- Single unified execution path
- Session awareness for multi-user
- Persistent tool state across sessions
- Namespace-based access control
- 40% code reduction in core modules

**Testing**:
- 100+ tests passing
- Zero regressions
- Integration tests for all tools
- Persistence round-trip tests
```

---

## Verification Checklist

### Phase 3
- [ ] Handler supports both V1 and V2 tools
- [ ] Session ID injection works
- [ ] Namespace filtering active
- [ ] Tests pass for both paths

### Phase 4
- [ ] All sync tools migrated
- [ ] All async tools migrated
- [ ] Feature flags working
- [ ] All tool tests pass
- [ ] Integration tests pass

### Phase 5
- [ ] DB schema created
- [ ] Persistence methods implemented
- [ ] Graceful logout polls to completion
- [ ] Restore loads completed calls
- [ ] Round-trip tests pass
- [ ] Timeout handling works

### Phase 6
- [ ] Feature flags removed
- [ ] Old implementations deleted
- [ ] Completion.rs simplified
- [ ] Event loop removed
- [ ] Documentation updated
- [ ] PROGRESS.md updated
- [ ] All tests green

---

## Estimated Timeline

| Phase | Duration | Complexity |
|-------|----------|------------|
| Phase 3 | 2-3 days | Medium |
| Phase 4 | 3-5 days | Medium-High |
| Phase 5 | 2-3 days | Medium |
| Phase 6 | 2-3 days | Low |
| **Total** | **9-14 days** | **Medium** |

---

## Risk Assessment

### Low Risk ✅
- Phases 0-2 infrastructure solid
- Feature flags enable gradual rollout
- Backward compatible approach
- Comprehensive test coverage

### Medium Risk ⚠️
- Phase 4 tool migration (many tools)
- Phase 5 persistence (DB integration)

### Mitigation ✅
- Migrate one tool at a time
- Test each tool individually
- Use staging environment
- Rollback plan for each phase

---

## Conclusion

**Current State**: Phases 0-2 complete, infrastructure solid
**Recommended Approach**: Follow this guide step-by-step
**Expected Outcome**: Unified, session-aware, persistent tool execution

**Next Step**: Begin Phase 3 with handler improvements

Good luck! 🚀
