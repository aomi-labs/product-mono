# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**System Event Buffer** — Separate system-side signals from the LLM/user chat stream so wallet, backend, and external events can flow without polluting chat history.

---

## Branch Status

Current branch: `system-event-buff-v2` (base: `main`)

**Recent Commits** (last 10):
```
58f70067 refactored async events, purely event driven and moved finalization to session layer
5dd2b9cc refactored completion module, added async spawn for tool call to the scheduler's handler
4271efa3 added async tool calls to completion module and cli
cb130b12 fix clippy error
ddb953d5 refactored forge executor to allow multiple plans at the same time
79ae96c6 consolidated branches
b4f0cf09 fixing more clippy errors
828154ad fixed cli to work with forge
12d70bbc merge updated system-event-buff branch
ba8d678f Add cli system events tests
```

---

## Recently Completed Work

### System Event Initialization Refactor (c8f43fc)
| Change | Description |
|--------|-------------|
| **Moved system_events init** | System events now initialized where history starts, cleaner lifecycle |
| **ChatApp refactor** | Simplified queue handling in app.rs (74 lines changed) |
| **connections.rs cleanup** | Reduced complexity in connection handling (75→reduced) |
| **Eval crate updates** | eval_app.rs and eval_state.rs updated for new initialization pattern |
| **L2Beat integration** | l2beat/app.rs updated to match new system event flow |

### Processed System Event Index (a905490)
| Change | Description |
|--------|-------------|
| **processed_system_event_idx** | Tracks which events have been processed to avoid re-processing (session.rs:72) |
| **slice_from() usage** | Efficiently gets only new events since last check (session.rs:309) |
| **system_events Vec** | Stores processed events for sync_state response (session.rs:71) |
| **EXECUTOR-PLAN.md** | Added detailed plan for Forge Executor implementation (817 lines) |

### System Event Queue Implementation (a945e7e)
| Change | Description |
|--------|-------------|
| **SystemEvent enum** | Defines all system-level events: notices, errors, wallet requests/responses, user requests/responses (lib.rs:24-46) |
| **SystemEventQueue** | Thread-safe shared queue with `push()` and `drain()` methods using `Arc<Mutex<VecDeque>>` (lib.rs:48-72) |
| **CoreCommand cleanup** | Removed system variants from CoreCommand, now only contains: StreamingText, ToolCall, Complete, Error, Interrupted (lib.rs:75-93) |

### ChatApp Integration
| Change | Description |
|--------|-------------|
| **CoreAppBuilder changes** | Now accepts `SystemEventQueue` in constructor and passes through build process (app.rs) |
| **System event routing** | `MissingApiKey` and system notices now pushed to queue instead of CoreCommand channel |
| **`new()` and `new_with_retries()`** | Updated signatures to accept `SystemEventQueue` parameter |

### SessionState Changes (session.rs)
| Change | Description |
|--------|-------------|
| **system_event_queue field** | Added `SystemEventQueue` to SessionState struct |
| **system_events field** | Added `Vec<SystemEvent>` for drained events |
| **AomiBackend trait** | Added `fn system_events(&self) -> SystemEventQueue` method |
| **Removed CoreCommand variants** | Deleted handlers for `WalletTransactionRequest`, `System`, `BackendConnected`, `BackendConnecting`, `MissingApiKey` |
| **BackendConnected routing** | Now pushes `SystemEvent::BackendConnected` to queue instead of sending CoreCommand |

### Other Updates
| Change | Description |
|--------|-------------|
| **connections.rs** | Updated to use SystemEventQueue for system messages |
| **completion.rs** | Updated to route system events to queue |
| **l2beat/app.rs** | Implements `system_events()` trait method |
| **eval crates** | Updated to conform to new API (eval_app.rs, harness.rs) |
| **Test utilities** | Updated session tests and utils for new API |

### Specs & Commands
| Change | Description |
|--------|-------------|
| **SYSTEM-BUS-PLAN.md** | Detailed design document for system event architecture |
| **Claude commands** | Added `read-specs.md`, `update-specs.md`, `cleanup-md.md` |
| **DOMAIN.md, METADATA.md** | Project documentation added |

### ToolScheduler Refactor for Multi-Step Results (ddbfdff → c76fbf31)

**Purpose**: Enable multi-step tool calls to route subsequent results to system event buffer, allowing async tool progress to appear as UI notifications without polluting LLM chat history.

```
Architecture (final):

unresolved_calls: Vec<ToolReciever>        ongoing_streams: Vec<ToolStreamream>
         │                                            │
         │ resolve_calls_to_streams()                 │ poll_streams_to_next_result()
         │ converts calls → streams                   │ polls streams → ToolCompletion
         ▼                                            ▼
┌─────────────────────┐                    ┌─────────────────────┐
│    ToolReciever     │ ──────────────────▶│  ToolStreamream   │
│  (internal channel) │  into_shared_      │  (UI-facing stream) │
│  - single_rx        │  streams()         │  - Single(Shared)   │
│  - multi_step_rx    │                    │  - Multi(mpsc)      │
│                     │                    │  + tool_name        │
│                     │                    │  + is_multi_step    │
└─────────────────────┘                    └─────────────────────┘
                                                      │
                                                      ▼
                                           ┌─────────────────────┐
                                           │   ToolCompletion    │
                                           │  - call_id          │
                                           │  - tool_name        │
                                           │  - is_multi_step    │
                                           │  - result           │
                                           └─────────────────────┘
```

| Change | Description |
|--------|-------------|
| **tool_stream.rs (NEW)** | Separated `ToolReciever` and `ToolStreamream` into dedicated module |
| **ToolReciever** | Internal type holding raw channel receivers (`single_rx`, `multi_step_rx`) |
| **ToolStreamream** | UI-facing stream with metadata fields (`tool_name`, `is_multi_step`) |
| **ToolCompletion** | Return type from `poll_streams_to_next_result()` with full metadata |
| **into_shared_streams()** | Converts receiver → two streams: one for ongoing polling, one for UI ACK |
| **split_first_chunk_and_rest** | Multi-step spawns task to fan out first chunk to both streams |
| **Lock-free design** | Receiver owned exclusively by `ToolReciever`, no `Arc<Mutex>` needed |
| **resolve_calls_to_streams()** | Converts `unresolved_calls` to `ongoing_streams` |
| **poll_streams_to_next_result()** | Polls `ongoing_streams`, returns `ToolCompletion` |

**Key Files**:
- `crates/tools/src/tool_stream.rs` — `ToolReciever`, `ToolStreamream`, `ToolCompletion`, `ToolResultSender`
- `crates/tools/src/scheduler.rs` — `ToolScheduler`, `ToolHandler`, `SchedulerRuntime`
- `crates/tools/src/types.rs` — `AomiTool` trait with `MultiStepResults` associated type
- `crates/tools/src/test.rs` — Modular test suite with mock tools

### Multi-Step to SystemEventQueue (Phase 6)

**Purpose**: Route multi-step tool results to `SystemEventQueue` so frontend receives async tool progress as system events.

| Change | Description |
|--------|-------------|
| **types.rs** | Added `MultiStepResults` associated type to `AomiTool`, `validate_multi_step_result` method |
| **tool_stream.rs** | Added `ToolCompletion` struct, metadata fields on `ToolStreamream` |
| **lib.rs (chat)** | Added `AsyncToolResult` to `CoreCommand`, `SystemToolDisplay` to `SystemEvent` |
| **scheduler.rs** | `poll_streams_to_next_result()` returns `ToolCompletion` with metadata |
| **completion.rs** | Finalization loop yields `AsyncToolResult` for multi-step tools |
| **session.rs** | Matches `AsyncToolResult` → pushes `SystemToolDisplay` to queue |

**Flow**:
```
completion.rs finalization loop
  → poll_streams_to_next_result() yields ToolCompletion
  → if is_multi_step: yield CoreCommand::AsyncToolResult { call_id, tool_name, result }
  → session.rs matches AsyncToolResult
  → pushes SystemEvent::SystemToolDisplay { tool_name, call_id, result }
```

### Async Event-Driven Refactor (58f70067)

**Purpose**: Make the system purely event-driven with finalization moved to the session layer.

| Change | Description |
|--------|-------------|
| **Purely event-driven** | Refactored async events to be purely event-driven architecture |
| **Finalization to session layer** | Moved finalization logic from completion to session layer |
| **Session layer refactor** | Major refactor to session.rs (246 lines changed) |
| **Cleaner separation** | Better separation between completion and session responsibilities |

### Async Tool Calls & Completion Refactor (5dd2b9cc, 4271efa3)

**Purpose**: Enable async tool calls in the completion module with proper scheduler integration.

| Change | Description |
|--------|-------------|
| **Async spawn for tool calls** | Added async spawn for tool calls to the scheduler's handler |
| **Completion module refactor** | Major refactor of completion.rs (376 lines changed) |
| **CLI async support** | Added async tool calls support to CLI |
| **ChatApp changes** | Updated app.rs for new async flow |

### Forge Executor Multi-Plan Support (ddb953d5)

**Purpose**: Allow the Forge executor to handle multiple plans simultaneously.

| Change | Description |
|--------|-------------|
| **Multi-plan support** | Refactored forge executor to allow multiple plans at the same time |
| **Executor refactor** | Major changes to executor.rs (725 lines, extensive refactor) |
| **Manager module** | Added new manager.rs (72 lines) for plan management |
| **Resources module** | Added resources.rs (36 lines) for resource handling |
| **Tools update** | Updated tools.rs (229 lines) for multi-plan flow |

### CLI System Events Tests (ba8d678f)

**Purpose**: Test coverage for system events in CLI.

| Change | Description |
|--------|-------------|
| **test_app.rs** | Updated test app with system event handling |
| **test_backend.rs** | Backend test updates for system events |
| **printer.rs** | Printer updates for system event display |

### Code Cleanup & Refactoring (e91089e0 → c76fbf31)

**Purpose**: Improve naming clarity and test isolation.

| Change | Description |
|--------|-------------|
| **Renamed fields** | `pending_futures` → `unresolved_calls`, `pending_streams` → `ongoing_streams` |
| **Renamed methods** | `poll_futures_to_streams()` → `resolve_calls_to_streams()`, `take_futures()` → `take_unresolved_calls()`, etc. |
| **SchedulerRuntime enum** | Elegant runtime ownership: `Borrowed(Handle)` vs `Owned(Runtime)` |
| **Removed clients field** | Dead code - `ExternalClients` initialized globally, not stored in scheduler |
| **Test modularization** | `mock_tools.rs` module, `ToolScheduler::new_for_test()` for isolated testing |
| **Rig fallback restored** | completion.rs now falls back to Rig tool registry for non-scheduler tools (MCP) |

**SchedulerRuntime design**:
```rust
enum SchedulerRuntime {
    Borrowed(tokio::runtime::Handle),  // Use existing runtime
    Owned(tokio::runtime::Runtime),    // We own the runtime (tests, no existing runtime)
}

impl SchedulerRuntime {
    fn new() -> Result<Self>           // Borrow if available, else create owned
    fn new_for_test() -> Result<Self>  // Always create owned for test isolation
    fn handle(&self) -> &Handle        // Unified access
}
```

---

## Files Modified This Sprint

### Chat Crate
| File | Description |
|------|-------------|
| `crates/chat/src/lib.rs` | SystemEvent enum + SystemEventQueue (184+ lines changed) |
| `crates/chat/src/app.rs` | CoreAppBuilder accepts SystemEventQueue (29 lines changed) |
| `crates/chat/src/completion.rs` | Major refactor for async tool calls (376 lines changed) |
| `crates/chat/src/connections.rs` | Uses SystemEventQueue for connection status |

### Backend Crate
| File | Description |
|------|-------------|
| `crates/backend/src/session.rs` | Major refactor: event-driven, finalization moved here (246 lines changed) |
| `crates/backend/src/background.rs` | Background task updates (30 lines changed) |
| `crates/backend/src/types.rs` | Type updates (22 lines changed) |
| `crates/backend/tests/test_session.rs` | Test updates for new API |
| `crates/backend/tests/test_wallet_events.rs` | Wallet event tests |
| `crates/backend/tests/utils.rs` | Test utility updates (42 lines changed) |

### Tools Crate
| File | Description |
|------|-------------|
| `crates/tools/src/streams.rs` | Tool result streams (121 lines changed) |
| `crates/tools/src/scheduler.rs` | ToolScheduler with async spawn (172 lines changed) |
| `crates/tools/src/tests/test_scheduler.rs` | Scheduler tests |
| `crates/tools/src/tests/utils.rs` | Test utilities |

### Scripts Crate (Forge Executor)
| File | Description |
|------|-------------|
| `crates/scripts/src/forge_executor/executor.rs` | Multi-plan support (725 lines, major refactor) |
| `crates/scripts/src/forge_executor/manager.rs` | **NEW** — Plan management (72 lines) |
| `crates/scripts/src/forge_executor/resources.rs` | **NEW** — Resource handling (36 lines) |
| `crates/scripts/src/forge_executor/tools.rs` | Tool updates for multi-plan (229 lines changed) |
| `crates/scripts/src/forge_executor/mod.rs` | Module exports |

### CLI
| File | Description |
|------|-------------|
| `bin/cli/src/main.rs` | Async tool call support |
| `bin/cli/src/printer.rs` | System event display |
| `bin/cli/src/test_app.rs` | Test app updates (31 lines changed) |
| `bin/cli/src/test_backend.rs` | Test backend updates |

### Backend Binary
| File | Description |
|------|-------------|
| `bin/backend/src/endpoint/system.rs` | System endpoint updates |

### Other Crates
| File | Description |
|------|-------------|
| `crates/eval/src/eval_app.rs` | Implements system_events() |
| `crates/eval/src/eval_state.rs` | Eval state updates |
| `crates/forge/src/app.rs` | Forge app updates |
| `crates/l2beat/src/app.rs` | Implements system_events() |

### Specs
| File | Description |
|------|-------------|
| `specs/SYSTEM-BUS-PLAN.md` | Design document for system event architecture (37 lines added) |
| `specs/DOMAIN.md` | Domain documentation |
| `specs/METADATA.md` | Project metadata |

---

## Pending Tasks

### Immediate Priority

1. **Wire wallet flow through system events**:
   - `WalletTxRequest` → system buffer → UI pending flag
   - `WalletTxResponse` → UI + optional LLM injection

2. **Test system event flow end-to-end**:
   - Verify multi-step tool results appear as system events
   - Test wallet request/response cycle
   - Test connection status updates

### Short-Term

3. **Update frontend integration**:
   - Handle system events in sync_state response
   - Render system notices separately from chat
   - Show multi-step tool progress as notifications

### Completed (this sprint)

4. **Forge Executor multi-plan support** ✓:
   - Refactored executor for multiple concurrent plans (ddb953d5)
   - Added manager.rs and resources.rs modules
   - Updated tools for multi-plan flow

5. **Async event-driven architecture** ✓:
   - Purely event-driven refactor (58f70067)
   - Finalization moved to session layer
   - Async spawn for tool calls in scheduler (5dd2b9cc)

6. **CLI system events support** ✓:
   - Added async tool calls to CLI (4271efa3)
   - Added system events tests (ba8d678f)

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Wallet flow not fully wired | Pending | Uses old CoreCommand approach |
| Frontend doesn't consume system_events | Pending | sync_state returns them, UI needs update |

---

## Multi-Step Flow State

Current Position: Migration Phase (Steps 1-8 done, Step 9 pending)

| Step | Description | Status |
|------|-------------|--------|
| 1 | Define SystemEvent + SystemEventQueue | ✓ Done |
| 2 | Inject queue into ChatApp/SessionState constructors | ✓ Done |
| 3 | Update stream_completion to route system signals | ✓ Done |
| 4 | Update SessionState::update_state to drain queue | ✓ Done |
| 5 | Refactor ToolScheduler for multi-step tool results | ✓ Done |
| 5a | Separate tool_stream.rs module (ToolResultFuture/Stream) | ✓ Done |
| 5b | Fix premature null results for single-result tools | ✓ Done |
| 5c | Implement into_shared_streams() with fanout for multi-step | ✓ Done |
| 5d | All scheduler tests passing | ✓ Done |
| 6 | Route multi-step results to SystemEventQueue | ✓ Done |
| 6a | Add `ToolCompletion` type (call_id, tool_name, is_multi_step, result) | ✓ Done |
| 6b | Add metadata fields to `ToolStreamream` (tool_name, is_multi_step) | ✓ Done |
| 6c | Add `AsyncToolResult` to CoreCommand, `SystemToolDisplay` to SystemEvent | ✓ Done |
| 6d | `poll_streams_to_next_result()` returns `ToolCompletion` | ✓ Done |
| 6e | Finalization loop yields `AsyncToolResult` for multi-step tools | ✓ Done |
| 6f | session.rs matches `AsyncToolResult` → pushes `SystemToolDisplay` | ✓ Done |
| 7 | Wire wallet flow through system events | Pending |
| 8 | Update sync_state() to return system events | ✓ Done |
| 9 | Frontend integration | Pending |
| **NEW** | Async event-driven refactor (finalization to session layer) | ✓ Done |
| **NEW** | Async spawn for tool calls in scheduler | ✓ Done |
| **NEW** | Forge executor multi-plan support | ✓ Done |
| **NEW** | CLI system events tests | ✓ Done |

---

## Notes for Next Agent

### Critical Context

1. **Architecture goal**
   - Separate system events from LLM chat stream
   - Two buffers: `CoreCommand` for chat, `SystemEventQueue` for system
   - UI can consume both independently
   - Agent only sees system events explicitly injected
   - Multi-step tool results flow to system events (async notifications)
   - **Purely event-driven architecture** with finalization in session layer

2. **Current state**
   - `SystemEvent` enum and `SystemEventQueue` implemented (chat/src/lib.rs)
   - `CoreCommand` cleaned up - no longer has system variants
   - `ChatApp` and `SessionState` hold queue references
   - `processed_system_event_idx` tracks consumption (session.rs)
   - `sync_state()` returns `system_events` alongside messages
   - **ToolScheduler refactored** for multi-step support with async spawn (scheduler.rs)
   - **Finalization moved to session layer** (58f70067)
   - **Forge executor supports multiple concurrent plans** (ddb953d5)

3. **ToolScheduler Architecture** (Updated)
   - **streams.rs**: Tool result streams (121 lines)
   - **Async spawn**: Tool calls spawned asynchronously to scheduler's handler
   - **Two-phase conversion**: `unresolved_calls` → `resolve_calls_to_streams()` → `ongoing_streams` → `poll_streams_to_next_result()` → `ToolCompletion`
   - **Multi-step fanout**: Spawns task to fan out first chunk to both streams, forwards rest to ongoing
   - **Single-result**: Uses `Shared<BoxFuture>` so both streams get same value
   - **SchedulerRuntime enum**: `Borrowed(Handle)` | `Owned(Runtime)` for clean runtime ownership
   - Lock-free design: receiver owned exclusively

4. **What's missing**
   - Wallet transaction flow needs to use system events
   - Frontend needs to consume system_events from sync_state response

5. **Design references**
   - `specs/SYSTEM-BUS-PLAN.md` — System event architecture
   - `specs/EXECUTOR-PLAN.md` — Forge Executor implementation plan

### Key Files
```
aomi/crates/tools/src/streams.rs         # Tool result streams
aomi/crates/tools/src/scheduler.rs       # ToolScheduler with async spawn
aomi/crates/chat/src/lib.rs              # SystemEvent + SystemEventQueue
aomi/crates/chat/src/completion.rs       # Stream completion loop (major refactor)
aomi/crates/backend/src/session.rs       # SessionState with event-driven finalization
aomi/crates/scripts/src/forge_executor/  # Forge executor with multi-plan support
specs/SYSTEM-BUS-PLAN.md                 # System event design document
```

### Message Flow Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              MESSAGE FLOW                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  UI                                                                          │
│   │                                                                          │
│   ├─▶ send_user_input() ───▶ [sender_to_llm] ───────────────-──────────────┐ │
│   │                                                                        │ │
│   └─▶ send_ui_event() ───▶ SystemEventQueue.push()                         │ │
│                                    │                                       │ │
│                                    ▼                                       │ │
│                            [append-only log]                               │ │
│                             events: Vec<E>                                 │ │
│                             frontend_cnt: N                                │ │
│                             llm_cnt: M                                     │ │
│                                    │                                       │ │
│  ┌─────────────────────────────────┼───────────────────────────────────────┤ │
│  │ Background LLM Task             ▼                                       │ │
│  │  receiver_from_ui.recv() ◀──────┘                                       │ │
│  │         │                                                               │ │
│  │         ▼                                                               │ │
│  │  AomiBackend.process_message()                                          │ │
│  │         │                                                               │ │
│  │         ▼                                                               │ │
│  │  CompletionRunner.stream()                                              │ │
│  │         │                                                               │ │
│  │         ├─▶ consume_stream_item() ─▶ CoreCommand::StreamingText         │ │
│  │         │                                      │                         │ │
│  │         └─▶ consume_tool_call() ──────────────┐│                        │ │
│  │                                               ││                        │ │
│  │  ToolHandler                               ││                        │ │
│  │         │                                     ││                        │ │
│  │         ▼ request()                           ││                        │ │
│  │         ▼ resolve_last_call() ──▶ ui_stream   ││                        │ │
│  │                │                      │       ││                        │ │
│  │                ▼ bg_stream            │       ││                        │ │
│  │    [ongoing_streams]                  │       ││                        │ │
│  └────────────────│──────────────────────┼───────┼┼────────────────────────┤ │
│                   │                      │       ││                        │ │
│  ┌────────────────┼──────────────────────┼───────┼┼────────────────────────┤ │
│  │ Background Poller Task                │       ││                        │ │
│  │         │                             │       ││                        │ │
│  │         ▼ poll_streams_once()         │       ││                        │ │
│  │         ▼ take_completed_calls()      │       ││                        │ │
│  │         │                             │       ││                        │ │
│  │         ▼                             │       ││                        │ │
│  │  SystemEventQueue.push_tool_update()  │       ││                        │ │
│  └─────────────────│─────────────────────┼───────┼┼────────────────────────┤ │
│                    │                     │       ││                        │ │
│                    ▼                     │       ▼▼                        │ │
│             [append-only log]            │  [command_sender]                 │ │
│                    │                     │       │                         │ │
│                    │                     │       ▼                         │ │
│  SessionState      │                     │  sync_state()                   │ │
│         │          │                     │       │                         │ │
│         │          │                     │       ├─▶ update messages[]     │ │
│         │          │                     │       │                         │ │
│         │          │                     └───────┼─▶ poll_ui_streams()   │ │
│         │          │                             │                         │ │
│         │          └─────────────────────────────┼─▶ sync_system_events()  │ │
│         │                                        │        │                │ │
│         │  advance_frontend_events() ◀───────────┘        ▼                │ │
│         │          │                             send_events_to_history()  │ │
│         ▼          ▼                                      │                │ │
│       API Response (ChatState)                            ▼                │ │
│                                                   agent_history.push()     │ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Method Naming Conventions**:

| Layer | Pattern | Methods |
|-------|---------|---------|
| SessionState | `send_*`, `sync_*` | `send_user_input`, `send_ui_event`, `send_system_prompt`, `sync_state`, `send_events_to_history` |
| AomiBackend | `process_*` | `process_message` |
| CompletionRunner | `consume_*` | `consume_stream_item`, `consume_tool_call` |
| ToolHandler | `request`, `resolve_*`, `poll_*` | `request`, `resolve_last_call`, `poll_streams_once` |

### Quick Start Commands
```bash
# Check compilation
cargo check --all

# Run clippy
cargo clippy --all

# Run tests
cargo test --all

# Run scheduler tests specifically
cargo test --package aomi-tools -- scheduler

# Run forge executor tests
cargo test --package aomi-scripts -- forge_executor
```

### Implementation Next Steps

**Wallet flow wiring** — Update wallet transaction handling to use SystemEvent:
1. Replace CoreCommand wallet variants with SystemEvent equivalents
2. Push `WalletTxRequest` to system queue when transaction initiated
3. Handle `WalletTxResponse` from queue and optionally inject into agent history

**Frontend integration** — Update UI to consume system_events:
1. Parse `system_events` from sync_state response
2. Render system notices separately from chat
3. Show multi-step tool progress as notifications
4. Handle wallet transaction UI state from system events

**Testing patterns**:
- Use `ToolScheduler::new_for_test()` for isolated test instances
- CLI tests in `bin/cli/src/test_app.rs` and `test_backend.rs`
- Forge executor fixtures in `crates/scripts/src/forge_executor/tests/`
