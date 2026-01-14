# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**Architecture Refinement** — Clean up core abstractions, improve type safety with CallMetadata, standardize error handling with eyre, and simplify API interfaces for better maintainability.

---

## Branch Status

Current branch: `cecilia/refine-abstractions` (base: `main`)

**Recent Commits** (last 10):
```
5c39487a CallMetadata
b45c5a3b pass clippy & fmt all
86dc0252 fmt
67f8346e use eyre everywhere
1b145bed move AomiApp definition
5592caea CoreAppBuilder::new
a7239782 SessionManager
5309b7cb SessionState start_processing & start_polling_tools
9b484827 chage process_message everywhere
b236adc9 chage process_message everywhere
```

---

## Recently Completed Work

### CallMetadata Type Introduction (5c39487a)
| Change | Description |
|--------|-------------|
| **CallMetadata type** | Added strongly-typed `CallMetadata` throughout the codebase for better type safety |
| **streams.rs updates** | Updated ToolStream, ToolCompletion to use CallMetadata (67 lines changed) |
| **scheduler.rs** | Updated ToolScheduler to use CallMetadata (4 lines changed) |
| **completion.rs** | Stream completion now uses CallMetadata (29 lines changed) |
| **session.rs** | SessionState processing uses CallMetadata (9 lines changed) |
| **Test updates** | All tests updated for new type (test_scheduler.rs, test_session.rs, test_wallet_events.rs, utils.rs) |

### Error Handling Standardization (67f8346e)
| Change | Description |
|--------|-------------|
| **eyre adoption** | Standardized on `eyre::Result` and `eyre::Report` throughout codebase |
| **Removed anyhow** | Replaced all `anyhow` usage with `eyre` for consistent error handling |
| **Better error context** | Improved error messages and context using eyre's `.wrap_err()` pattern |

### Core Architecture Refactoring (1b145bed → b236adc9)
| Change | Description |
|--------|-------------|
| **CoreCtx introduction** | New `CoreCtx` type to hold core runtime context (40944a84, 9677b008) |
| **SessionManager** | Extracted session management into dedicated `SessionManager` type (a7239782) |
| **CoreAppBuilder::new** | Simplified builder pattern for CoreApp construction (5592caea) |
| **AomiApp definition** | Moved and clarified AomiApp trait definition (1b145bed) |
| **start_processing/start_polling_tools** | Cleaner separation of processing and polling concerns in SessionState (5309b7cb) |
| **process_message refactor** | Unified message processing across different app types (9b484827, b236adc9) |
| **command_sender** | Clarified command sender abstractions (e337f656) |
| **App::new simplification** | Simplified initialization flow (316e9c30, 2fbbdd47, c543e15e) |

### Code Quality Improvements (b45c5a3b, 86dc0252)
| Change | Description |
|--------|-------------|
| **Clippy compliance** | Fixed all clippy warnings across the codebase |
| **Formatting** | Applied consistent formatting (rustfmt) throughout |

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

### Core Abstraction Changes (This Branch)
| File | Description |
|------|-------------|
| `crates/chat/src/lib.rs` | Re-exports CallMetadata, ToolCompletion, ToolStream (5 lines changed) |
| `crates/chat/src/app.rs` | CoreAppBuilder refactor, CoreCtx integration (25 lines changed) |
| `crates/chat/src/completion.rs` | CallMetadata integration in stream completion (29 lines changed) |
| `crates/backend/src/session.rs` | SessionManager, start_processing/start_polling_tools, CallMetadata usage (9 lines changed) |
| `crates/tools/src/lib.rs` | CallMetadata export (2 lines changed) |
| `crates/tools/src/scheduler.rs` | CallMetadata in ToolScheduler (4 lines changed) |
| `crates/tools/src/streams.rs` | CallMetadata in ToolStream, ToolCompletion (67 lines changed) |

### Test Updates
| File | Description |
|------|-------------|
| `crates/backend/tests/test_session.rs` | Updated for CallMetadata (6 lines changed) |
| `crates/backend/tests/test_wallet_events.rs` | Updated for CallMetadata (4 lines changed) |
| `crates/backend/tests/utils.rs` | Updated test utilities (15 lines changed) |
| `crates/tools/src/tests/test_scheduler.rs` | Updated for CallMetadata (8 lines changed) |
| `crates/tools/src/tests/utils.rs` | Updated mock tools (11 lines changed) |
| `bin/cli/src/test_backend.rs` | Updated CLI tests (6 lines changed) |

### Previous Sprint Work (Merged from system-event-buff-v2)
| File | Description |
|------|-------------|
| `crates/chat/src/lib.rs` | SystemEvent enum + SystemEventQueue |
| `crates/chat/src/completion.rs` | Major refactor for async tool calls (376 lines changed) |
| `crates/backend/src/session.rs` | Event-driven architecture, finalization moved to session layer (246 lines changed) |
| `crates/tools/src/streams.rs` | Tool result streams (121 lines changed) |
| `crates/tools/src/scheduler.rs` | ToolScheduler with async spawn (172 lines changed) |
| `crates/apps/forge/` | **NEW** — Forge app restructured into apps/ directory |
| `crates/apps/l2beat/` | **MOVED** — L2Beat app moved to apps/ directory |
| `crates/anvil/` | Anvil lifecycle management, ProviderManager |
| `bin/backend/` | Database migrations, history endpoint, session persistence |
| `bin/cli/` | CLI system events tests, test_app.rs, test_backend.rs |

### Specs & Documentation
| File | Description |
|------|-------------|
| `specs/PROGRESS.md` | Updated for refine-abstractions sprint (679 lines changed) |
| `specs/DOMAIN.md` | Domain documentation updates |
| `specs/METADATA.md` | Project metadata |
| `important-convo.md` | **NEW** — Architecture conversation notes (2286 lines) |
| `specs/stateless-arch.excalidraw` | **NEW** — Architecture diagram (2024 lines) |

---

## Pending Tasks

### Immediate Priority

1. **Merge to main**:
   - Review all changes in refine-abstractions branch
   - Ensure all tests pass
   - Create PR to merge improvements back to main

2. **Continue system event integration**:
   - Wire wallet flow through system events
   - Frontend integration for system event display
   - End-to-end testing of multi-step tool flow

### Completed (this sprint: cecilia/refine-abstractions)

1. **CallMetadata type safety** ✓:
   - Introduced strongly-typed CallMetadata (5c39487a)
   - Updated all tool-related code to use CallMetadata
   - Fixed all tests for new type

2. **Error handling standardization** ✓:
   - Migrated from anyhow to eyre (67f8346e)
   - Consistent error handling throughout codebase

3. **Core architecture cleanup** ✓:
   - CoreCtx for runtime context (40944a84, 9677b008)
   - SessionManager extraction (a7239782)
   - CoreAppBuilder simplification (5592caea)
   - Unified process_message interface (9b484827, b236adc9)
   - Cleaner separation of concerns (5309b7cb)

4. **Code quality** ✓:
   - All clippy warnings fixed (b45c5a3b)
   - Consistent formatting (86dc0252)

### Completed (previous sprint: system-event-buff-v2)

5. **Async event-driven architecture** ✓:
   - Purely event-driven refactor (58f70067)
   - Finalization moved to session layer
   - Async spawn for tool calls in scheduler (5dd2b9cc)

6. **Forge Executor multi-plan support** ✓:
   - Refactored executor for multiple concurrent plans (ddb953d5)
   - Added manager.rs and resources.rs modules

7. **CLI system events support** ✓:
   - Added async tool calls to CLI (4271efa3)
   - Added system events tests (ba8d678f)

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Wallet flow not fully wired | Pending | Uses old CoreCommand approach, needs system event migration |
| Frontend doesn't consume system_events | Pending | sync_state returns them, UI needs update |
| No known blockers in refine-abstractions | Clean | All tests passing, clippy clean |

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

1. **Current Branch: cecilia/refine-abstractions**
   - This branch focuses on improving core abstractions and type safety
   - Based on system-event-buff-v2 work
   - All tests passing, clippy clean, ready for merge review

2. **Recent Changes (This Branch)**
   - **CallMetadata**: Introduced strongly-typed identifier for tool calls throughout codebase
   - **eyre**: Standardized error handling, replaced all anyhow usage
   - **CoreCtx**: New type for core runtime context
   - **SessionManager**: Extracted session management logic
   - **CoreAppBuilder**: Simplified builder pattern
   - **Unified interfaces**: Consistent process_message, start_processing/start_polling_tools APIs

3. **Architecture goal** (from system-event-buff-v2)
   - Separate system events from LLM chat stream
   - Two buffers: `CoreCommand` for chat, `SystemEventQueue` for system
   - UI can consume both independently
   - Agent only sees system events explicitly injected
   - Multi-step tool results flow to system events (async notifications)
   - **Purely event-driven architecture** with finalization in session layer

4. **Current state**
   - `SystemEvent` enum and `SystemEventQueue` implemented (chat/src/lib.rs)
   - `CoreCommand` cleaned up - no longer has system variants
   - `ChatApp` and `SessionState` hold queue references
   - `sync_state()` returns `system_events` alongside messages
   - **ToolScheduler refactored** for multi-step support with async spawn
   - **CallMetadata** type safety throughout tool handling
   - **Finalization moved to session layer**
   - **Forge executor supports multiple concurrent plans**

5. **ToolScheduler Architecture**
   - **streams.rs**: Tool result streams with CallMetadata
   - **Async spawn**: Tool calls spawned asynchronously to scheduler's handler
   - **Two-phase conversion**: `unresolved_calls` → `resolve_calls_to_streams()` → `ongoing_streams` → `poll_streams_to_next_result()` → `ToolCompletion`
   - **Multi-step fanout**: Spawns task to fan out first chunk to both streams
   - **Single-result**: Uses `Shared<BoxFuture>` so both streams get same value
   - **SchedulerRuntime enum**: `Borrowed(Handle)` | `Owned(Runtime)` for clean runtime ownership
   - Lock-free design: receiver owned exclusively

6. **What's missing**
   - Wallet transaction flow needs to use system events
   - Frontend needs to consume system_events from sync_state response

7. **Design references**
   - `specs/SYSTEM-BUS-PLAN.md` — System event architecture
   - `important-convo.md` — Architecture discussion notes
   - `specs/stateless-arch.excalidraw` — Architecture diagram

### Key Files (Updated This Branch)
```
aomi/crates/tools/src/streams.rs         # CallMetadata in ToolStream, ToolCompletion
aomi/crates/tools/src/scheduler.rs       # CallMetadata in ToolScheduler
aomi/crates/chat/src/lib.rs              # CallMetadata re-export, SystemEvent + SystemEventQueue
aomi/crates/chat/src/app.rs              # CoreAppBuilder, CoreCtx integration
aomi/crates/chat/src/completion.rs       # CallMetadata in stream completion
aomi/crates/backend/src/session.rs       # SessionManager, start_processing/start_polling_tools
```

### Key Files (From Previous Sprint)
```
aomi/crates/tools/src/streams.rs         # Tool result streams
aomi/crates/tools/src/scheduler.rs       # ToolScheduler with async spawn
aomi/crates/chat/src/lib.rs              # SystemEvent + SystemEventQueue
aomi/crates/chat/src/completion.rs       # Stream completion loop (major refactor)
aomi/crates/backend/src/session.rs       # SessionState with event-driven finalization
aomi/crates/apps/forge/                  # Forge app restructured
aomi/crates/apps/l2beat/                 # L2Beat app moved
specs/SYSTEM-BUS-PLAN.md                 # System event design document
important-convo.md                       # Architecture conversation notes
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

**This Branch (refine-abstractions)**:
1. **Merge preparation**:
   - Run full test suite: `cargo test --all`
   - Run clippy: `cargo clippy --all`
   - Create PR for review
   - Merge to main

**Next Sprint**:
1. **Wallet flow wiring** — Update wallet transaction handling to use SystemEvent:
   - Replace CoreCommand wallet variants with SystemEvent equivalents
   - Push `WalletTxRequest` to system queue when transaction initiated
   - Handle `WalletTxResponse` from queue and optionally inject into agent history

2. **Frontend integration** — Update UI to consume system_events:
   - Parse `system_events` from sync_state response
   - Render system notices separately from chat
   - Show multi-step tool progress as notifications
   - Handle wallet transaction UI state from system events

**Testing patterns**:
- Use `ToolScheduler::new_for_test()` for isolated test instances
- CLI tests in `bin/cli/src/test_app.rs` and `test_backend.rs`
- All tool-related code uses CallMetadata for type safety
- Error handling uses eyre::Result and eyre::Report
