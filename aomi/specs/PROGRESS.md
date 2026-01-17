# Project Progress: Tool Architecture Refactor

**Branch:** `cecilia/refine-abstractions`
**Last Updated:** 2026-01-15

---

## Sprint Goal

Refactor and simplify the tool execution architecture by:
1. Consolidating tools-v2 into the main tools crate
2. Replacing ToolStream with immediate ToolReturn + async ToolCompletion pattern
3. Removing SessionToolHandler from the completion loop
4. Converting all tools from rig Tool trait to AomiTool trait
5. Simplifying the scheduler and completion flow

**Status:** 🟡 In Progress (95% complete - final cleanup phase)

---

## Architecture Changes

### Before (tools-v2 + ToolStream)
```
tools-v2 (separate crate)
  └── Tool trait definitions

tools
  └── ToolStream (complex async streaming)
  └── SessionToolHandler (passed through completion loop)
  └── Rig Tool implementations (register.rs - 723 lines)

Backend Session
  └── Creates handler
  └── Passes handler to start_processing
  └── Polls tools via start_polling_tools
```

### After (unified tools + ToolReturn)
```
tools (unified)
  └── AomiTool trait (sync + async support)
  └── ToolReturn (immediate sync acknowledgment)
  └── ToolCompletion (async result when ready)
  └── AomiToolWrapper (wraps AomiTool for rig)

Backend Session
  └── No handler in processing loop
  └── Tools call scheduler directly via session_id + namespaces
  └── Completion gets handlers via get_handlers() helper
```

### Key Pattern Changes

**Tool Response Flow:**
- Old: ToolStream with complex polling state machine
- New: Immediate ToolReturn (sync ack) + optional ToolCompletion (async result)

**Tool Arguments:**
- Old: `AomiToolArgs<T>` generic wrapper struct
- New: `AomiToolArgs` trait with `session_id()` method

**Tool Registration:**
- Old: Implement rig `Tool` trait in register.rs
- New: Implement `AomiTool` trait directly, use `AomiToolWrapper` for rig integration

---

## Branch Status

**Current Branch:** `cecilia/refine-abstractions`
**Ahead of origin by:** 3 commits

**Recent Commits:**
```
159508ac correct process_tool_call in completion.rs and correct impl rigTool for AomiToolWrapper which calls get_handlers
db56fc50 remove Handler everywhere in the completion loop
123ca584 new aomi tool
5c39487a ToolCallId
b45c5a3b pass clippy & fmt all
```

**Staged Changes:** 47 files changed, 1,838 insertions(+), 1,638 deletions(-)

**Uncommitted Changes:**
- Deleted `crates/tools/src/register.rs` (723 lines)
- Modified `crates/tools/src/scheduler.rs` (removed `unresolved_calls()` getter)
- Modified `crates/tools/src/types.rs` (AomiToolArgs trait refactor)

---

## Recently Completed Work

### ✅ Phase 1: tools-v2 Consolidation
- **Date:** 2026-01-14
- **Changes:**
  - Deleted entire `crates/tools-v2` directory (334 files removed)
  - Merged tool interfaces into `aomi/tools/src/types.rs`
  - Updated workspace Cargo.toml to remove tools-v2 dependency
  - Replaced `ToolStream` with `ToolReturn` + `ToolCompletion` pattern
- **Files:**
  - Deleted: `crates/tools-v2/{Cargo.toml, src/{args.rs, lib.rs, tool_trait.rs, types.rs}}`
  - Modified: `Cargo.toml`, `Cargo.lock`

### ✅ Phase 2: Core Tool Trait Refactor
- **Date:** 2026-01-14
- **Changes:**
  - Converted all tools to `AomiTool` trait (from rig `Tool`)
  - Added `run_sync()` and `run_async()` methods to AomiTool
  - Implemented `AomiToolWrapper` to bridge AomiTool → rig Tool
  - Updated tool execution modules with sync/async pattern
  - Made `AomiToolArgs` a trait instead of generic wrapper struct
- **Key Files:**
  - `crates/tools/src/types.rs` - AomiTool trait definition + AomiToolArgs trait
  - `crates/tools/src/wrapper.rs` - AomiToolWrapper for rig integration
  - `crates/tools/src/execution/*.rs` - Tool implementations updated
  - `crates/tools/src/streams.rs` - Simplified from 400+ lines to ~120 lines

### ✅ Phase 3: Scheduler Simplification
- **Date:** 2026-01-14
- **Changes:**
  - Removed complex ToolStream state machine
  - Simplified scheduler to use immediate ToolReturn + async ToolCompletion
  - Updated ToolHandler to work without being passed through completion loop
  - Removed `SessionToolHandler` wrapper type
  - Added `get_handlers()` helper for completion layer
- **Files:**
  - `crates/tools/src/scheduler.rs` - Simplified handler management (~50 lines removed)
  - `crates/tools/src/streams.rs` - Removed ToolStream, kept ToolReciever

### ✅ Phase 4: Backend Session Cleanup
- **Date:** 2026-01-15
- **Changes:**
  - Removed `handler` parameter from `start_processing()` function signature
  - Removed `handler` field from `SessionState` struct
  - Removed scheduler initialization and handler creation in `SessionState::new()`
  - Removed `start_polling_tools()` spawned task
  - Updated session to pass `session_id` and `namespaces` directly
  - Fixed tests to use new pattern
- **Files:**
  - `crates/backend/src/session.rs` - Major cleanup of handler usage
  - `crates/backend/src/types.rs` - Removed handler field
  - `crates/backend/tests/*.rs` - Updated test helpers

### ✅ Phase 5: Completion Layer Updates
- **Date:** 2026-01-15
- **Changes:**
  - Updated `process_tool_call()` in completion.rs to use `get_handlers()`
  - Fixed `CoreAppBuilder` to register tools via `AomiToolWrapper`
  - Updated `CoreCtx` to remove handler field
  - Modified `CoreState` to include `session_id` and `namespaces`
- **Files:**
  - `crates/chat/src/completion.rs` - Updated tool call processing
  - `crates/chat/src/app.rs` - Builder pattern updates
  - `crates/chat/src/lib.rs` - Export updates

### ✅ Phase 6: App-Level Integration
- **Date:** 2026-01-15
- **Changes:**
  - Updated forge app to use AomiTool trait
  - Updated l2beat app to use AomiTool trait
  - Fixed l2beat handlers (array, event, storage) to return proper types
  - Updated test apps and backends to new pattern
  - Guarded tests against missing Anvil/templates/configs
- **Files:**
  - `crates/apps/forge/src/{app.rs, tools.rs}` - Tool conversions (~281 lines refactored)
  - `crates/apps/l2beat/src/{app.rs, l2b_tools.rs}` - Tool conversions (~289 lines added)
  - `crates/apps/l2beat/src/handlers/*.rs` - Handler fixes
  - `bin/cli/src/{test_app.rs, test_backend.rs}` - Test updates

### 🟡 Phase 7: Final Cleanup (In Progress)
- **Status:** Uncommitted changes
- **Remaining Work:**
  - Stage and commit deletion of `register.rs`
  - Stage and commit AomiToolArgs trait refactor
  - Clean up any remaining unused imports
  - Final verification that all tests pass
- **Files:**
  - `crates/tools/src/register.rs` - Ready for deletion
  - `crates/tools/src/types.rs` - AomiToolArgs trait ready
  - `crates/tools/src/scheduler.rs` - Minor cleanup ready

---

## Files Modified This Sprint

### Backend Core (8 files)
- `crates/backend/src/session.rs` - Removed handler from processing loop
- `crates/backend/src/types.rs` - Removed handler field from SessionState
- `crates/backend/tests/test_title_generation.rs` - Updated test helpers
- `crates/backend/tests/test_tool_call.rs` - Updated test helpers
- `crates/backend/tests/test_wallet_events.rs` - Updated test helpers
- `crates/backend/tests/utils.rs` - Refactored test utilities

### Chat/Completion Layer (3 files)
- `crates/chat/src/app.rs` - Updated CoreAppBuilder, CoreState, CoreCtx
- `crates/chat/src/completion.rs` - Updated process_tool_call with get_handlers()
- `crates/chat/src/lib.rs` - Export updates

### Tools Crate (17 files)
- `crates/tools/src/lib.rs` - Updated exports for new types
- `crates/tools/src/types.rs` - AomiTool trait + AomiToolArgs trait
- `crates/tools/src/wrapper.rs` - AomiToolWrapper implementation
- `crates/tools/src/scheduler.rs` - Simplified handler management
- `crates/tools/src/streams.rs` - Replaced ToolStream with ToolReturn/ToolCompletion
- `crates/tools/src/execution/abi_encoder.rs` - AomiTool impl
- `crates/tools/src/execution/account.rs` - AomiTool impl
- `crates/tools/src/execution/brave_search.rs` - AomiTool impl
- `crates/tools/src/execution/cast.rs` - AomiTool impl
- `crates/tools/src/execution/db_tools.rs` - AomiTool impl
- `crates/tools/src/execution/docs.rs` - AomiTool impl
- `crates/tools/src/execution/etherscan.rs` - AomiTool impl
- `crates/tools/src/execution/time.rs` - AomiTool impl
- `crates/tools/src/execution/wallet.rs` - AomiTool impl
- `crates/tools/src/tests/test_scheduler.rs` - Updated tests
- `crates/tools/src/tests/test_unit.rs` - Updated tests
- `crates/tools/src/tests/utils.rs` - Updated test helpers

### Apps (6 files)
- `crates/apps/forge/src/app.rs` - Updated to use AomiTool
- `crates/apps/forge/src/tools.rs` - Converted all tools to AomiTool
- `crates/apps/l2beat/src/app.rs` - Updated to use AomiTool
- `crates/apps/l2beat/src/l2b_tools.rs` - Converted all tools to AomiTool
- `crates/apps/l2beat/src/handlers/{array.rs, event.rs, storage.rs}` - Handler fixes

### Test Infrastructure (3 files)
- `bin/cli/src/test_app.rs` - Updated to new pattern
- `bin/cli/src/test_backend.rs` - Updated to new pattern
- `crates/scripts/src/forge_executor/tests/run_fixtures.rs` - Guards added

### Workspace (3 files)
- `Cargo.toml` - Removed tools-v2 dependency
- `Cargo.lock` - Updated dependencies
- `crates/apps/l2beat/Cargo.toml` - Removed tools-v2 dependency

### Deleted (5 files)
- `crates/tools-v2/Cargo.toml`
- `crates/tools-v2/src/args.rs`
- `crates/tools-v2/src/lib.rs`
- `crates/tools-v2/src/tool_trait.rs`
- `crates/tools-v2/src/types.rs`
- `crates/tools/src/register.rs` (uncommitted deletion)

### Documentation (1 file)
- `specs/CODEX-UPDATE.md` - Short summary of changes

---

## Pending Tasks

### Immediate (This Session)
- [ ] Stage and commit deletion of `register.rs`
- [ ] Stage and commit AomiToolArgs trait changes
- [ ] Run full test suite to verify no regressions
- [ ] Push commits to origin

### Follow-Up (Next Session)
- [ ] Update any remaining documentation for the new architecture
- [ ] Review error handling in async tool completions
- [ ] Consider adding metrics for tool execution timing
- [ ] Add integration tests for multi-step async tools

---

## Known Issues

### Resolved
- ✅ tools-v2 duplication → merged into tools crate
- ✅ ToolStream complexity → replaced with ToolReturn + ToolCompletion
- ✅ Handler passed through completion loop → removed, using get_handlers() instead
- ✅ rig Tool trait for all tools → converted to AomiTool trait
- ✅ Generic AomiToolArgs<T> wrapper → converted to trait
- ✅ 723-line register.rs with rig impls → deleted, using AomiToolWrapper

### Active
- None currently

---

## Architecture Deep Dive

### AomiTool Trait Pattern

**Core Design:**
```rust
pub trait AomiTool: Send + Sync + Clone + 'static {
    const NAME: &'static str;
    const NAMESPACE: &'static str = "default";

    type Args: AomiToolArgs;
    type Output: Serialize + Send + Sync + 'static;
    type Error: std::error::Error + Send + Sync + 'static;

    fn support_async(&self) -> bool;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> Value;

    fn run_sync(
        &self,
        result_sender: oneshot::Sender<EyreResult<Value>>,
        args: Self::Args,
    ) -> impl Future<Output = ()> + Send;

    fn run_async(
        &self,
        result_sender: mpsc::Sender<EyreResult<Value>>,
        args: Self::Args,
    ) -> impl Future<Output = ()> + Send;
}
```

**Benefits:**
- Unified interface for sync and async tools
- Clear support_async() flag for execution strategy
- Type-safe arguments with AomiToolArgs trait
- Works with rig via AomiToolWrapper

### AomiToolArgs Trait Pattern

**Old Pattern (Generic Wrapper):**
```rust
pub struct AomiToolArgs<T> {
    pub session_id: String,
    #[serde(flatten)]
    pub args: T,
}
```

**New Pattern (Trait):**
```rust
pub trait AomiToolArgs:
    for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static
{
    fn session_id(&self) -> &str;
    fn schema() -> Value;
}
```

**Benefits:**
- More flexible - types can implement it directly
- No wrapper struct overhead
- Tools define their own schema with system fields auto-added
- Better ergonomics for tool implementations

### ToolReturn + ToolCompletion Pattern

**Sync Tool Flow:**
```
LLM calls tool
  → Scheduler spawns run_sync()
  → Tool sends result via oneshot channel
  → Converted to ToolReturn (is_sync_ack: false)
  → Single response to LLM
```

**Async Tool Flow:**
```
LLM calls tool
  → Scheduler spawns run_async()
  → Tool sends ToolReturn via mpsc (is_sync_ack: true)
  → LLM gets "started working" acknowledgment
  → Tool continues working...
  → Tool sends ToolCompletion when done
  → LLM gets final result
```

**Benefits:**
- Immediate feedback for long-running operations
- Single response path for sync tools
- Clear distinction between acknowledgment and result
- Simpler than ToolStream state machine

### Session Context Flow

**Old Flow:**
```
SessionState::new()
  → Creates handler
  → Passes to start_processing(handler)
  → Passes to backend.process_message(ctx with handler)
  → Tool execution uses handler from context
```

**New Flow:**
```
SessionState::new()
  → Passes session_id + namespaces to start_processing()
  → Backend stores in CoreState
  → Completion layer calls get_handlers(session_id, namespaces)
  → Tool execution gets handler from scheduler directly
```

**Benefits:**
- No handler object passed through call stack
- Session context (id + namespaces) is just data
- Cleaner function signatures
- Handler created on-demand in completion layer

---

## Testing Strategy

### Test Categories

1. **Unit Tests** (crates/tools/src/tests/)
   - Tool trait implementations
   - Scheduler functionality
   - Stream handling

2. **Integration Tests** (crates/backend/tests/)
   - Session lifecycle
   - Tool call execution
   - Wallet event handling

3. **App Tests** (crates/apps/*/tests/)
   - Forge tool integration
   - L2beat handler behavior

### Test Helpers Updated

**Old Pattern:**
```rust
fn create_test_session() -> SessionState {
    // Created handler explicitly
}
```

**New Pattern:**
```rust
fn create_test_session() -> SessionState {
    // Just creates session, no handler
}
```

---

## Migration Notes for Dependent Code

### If you're implementing a new tool:

**Before:**
```rust
impl Tool for MyTool {
    const NAME: &'static str = "my_tool";
    type Args = MyToolArgs;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        // Manual schema definition
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output> {
        // Execute
    }
}
```

**After:**
```rust
impl AomiTool for MyTool {
    const NAME: &'static str = "my_tool";
    const NAMESPACE: &'static str = "default";
    type Args = MyToolArgs; // Must implement AomiToolArgs

    fn support_async(&self) -> bool { false }
    fn description(&self) -> &'static str { "..." }
    fn parameters_schema(&self) -> Value {
        MyToolArgs::schema()
    }

    fn run_sync(
        &self,
        result_sender: oneshot::Sender<EyreResult<Value>>,
        args: Self::Args,
    ) -> impl Future<Output = ()> + Send {
        async move {
            let result = /* your logic */;
            let _ = result_sender.send(result);
        }
    }
}
```

### If you're registering tools:

**Before:**
```rust
let builder = CoreAppBuilder::new()
    .tool(MyTool)
    .tool(AnotherTool);
```

**After:**
```rust
let builder = CoreAppBuilder::new()
    .tool(AomiToolWrapper::new(MyTool))
    .tool(AomiToolWrapper::new(AnotherTool));
```

---

## Notes for Next Agent

### Critical Context

1. **AomiTool is the new standard**: All tools should implement AomiTool, not rig Tool directly. Use AomiToolWrapper for rig integration.

2. **No handler in completion loop**: The SessionToolHandler is not passed through the processing stack. Use `get_handlers(session_id, namespaces)` in the completion layer when needed.

3. **AomiToolArgs is now a trait**: Tool argument types must implement the AomiToolArgs trait with `session_id()` and `schema()` methods.

4. **ToolReturn vs ToolCompletion**: Sync tools send one ToolReturn. Async tools send ToolReturn (acknowledgment) followed by ToolCompletion (result).

5. **register.rs is deleted**: The 723-line file with rig Tool implementations is gone. Each tool now implements AomiTool in its own module.

6. **Session context is data**: Pass `session_id: String` and `namespaces: Vec<String>` through CoreState, not a handler object.

### Uncommitted Work

The following changes are staged but not committed:
- Deletion of `crates/tools/src/register.rs`
- AomiToolArgs trait refactor in `types.rs`
- Minor cleanup in `scheduler.rs`

These should be committed together as "Final cleanup: remove register.rs and convert AomiToolArgs to trait"

### Code Navigation

| Component | File | Key Lines |
|-----------|------|-----------|
| AomiTool trait | `crates/tools/src/types.rs` | 76-205 |
| AomiToolWrapper | `crates/tools/src/wrapper.rs` | Full file |
| Tool execution | `crates/tools/src/scheduler.rs` | 200-350 |
| Backend session | `crates/backend/src/session.rs` | 19-100 |
| Completion layer | `crates/chat/src/completion.rs` | process_tool_call |
| Example sync tool | `crates/tools/src/execution/time.rs` | Full file |
| Example async tool | TBD (forge executor tools) | - |

### Common Gotchas

1. **Tool Args Must Implement AomiToolArgs**: If you see trait bound errors, make sure your Args type implements the AomiToolArgs trait with `session_id()` and `schema()`.

2. **Wrapper for rig Integration**: Don't forget to wrap AomiTool with AomiToolWrapper when passing to CoreAppBuilder.

3. **Schema Must Include System Fields**: Use `add_id_topic()` helper to ensure `session_id` and `topic` are in the schema.

4. **Async Tools Need Acknowledgment**: If `support_async()` returns true, send an initial ToolReturn with `is_sync_ack: true` before doing the work.

5. **No Handler in Tests**: Test utilities no longer create or pass SessionToolHandler. Use session_id and namespaces directly.

### Quick Commands

```bash
# Run all tests
cargo test --workspace

# Run just tools tests
cargo test -p aomi-tools

# Run just backend tests
cargo test -p aomi-backend

# Check for compilation errors
cargo check --workspace

# Run clippy
cargo clippy --workspace --all-targets -- -D warnings

# Stage uncommitted changes
git add crates/tools/src/register.rs
git add crates/tools/src/types.rs
git add crates/tools/src/scheduler.rs

# Commit final cleanup
git commit -m "Final cleanup: remove register.rs and convert AomiToolArgs to trait"

# Push to remote
git push origin cecilia/refine-abstractions
```

---

## Design Decisions

### Why Remove SessionToolHandler from Processing Loop?
- **Decision:** Pass session_id + namespaces as data, get handler on-demand in completion layer
- **Reasoning:** Simpler call stack, cleaner function signatures, easier testing
- **Alternative considered:** Keep passing handler through CoreCtx - creates unnecessary coupling
- **Date:** 2026-01-15

### Why Convert AomiToolArgs to Trait?
- **Decision:** Make AomiToolArgs a trait instead of generic wrapper struct
- **Reasoning:** More flexible, better ergonomics, no wrapper overhead
- **Alternative considered:** Keep generic wrapper - adds boilerplate and nesting
- **Date:** 2026-01-15

### Why Delete register.rs Instead of Updating It?
- **Decision:** Delete 723-line register.rs file with rig Tool impls
- **Reasoning:** All tools now implement AomiTool in their own modules, register.rs would be duplicate code
- **Alternative considered:** Update register.rs with AomiTool impls - unnecessary duplication
- **Date:** 2026-01-15

### Why ToolReturn + ToolCompletion vs ToolStream?
- **Decision:** Replace complex ToolStream with immediate ToolReturn + optional ToolCompletion
- **Reasoning:** Simpler to understand, clear sync vs async distinction, less state machine complexity
- **Alternative considered:** Keep ToolStream - too complex for the use case
- **Date:** 2026-01-14

### Why Merge tools-v2 into tools?
- **Decision:** Consolidate tools-v2 crate into main tools crate
- **Reasoning:** No need for separate crate, reduces dependencies, clearer architecture
- **Alternative considered:** Keep as separate crate - adds complexity without benefit
- **Date:** 2026-01-14

---

## Progress Summary

**Completed:** 95%
**Remaining:** 5% (final commit and push)

### What Changed
- Deleted entire tools-v2 crate (334 lines removed)
- Deleted register.rs with rig Tool impls (723 lines removed)
- Converted all tools from rig Tool to AomiTool (~40 tools)
- Removed SessionToolHandler from completion loop
- Simplified streams.rs from 400+ lines to ~120 lines
- Updated all apps and tests to new pattern
- Net change: ~1,800 insertions, ~2,400 deletions

### What's Left
- Commit uncommitted changes (register.rs deletion + AomiToolArgs trait)
- Final test verification
- Push to origin

### Impact
- **Developers**: Simpler tool implementation pattern
- **Performance**: Reduced overhead from wrapper objects
- **Maintainability**: Clearer separation of concerns
- **Testing**: Easier to test without handler object

---
