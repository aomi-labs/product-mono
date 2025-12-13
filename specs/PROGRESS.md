# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**System Event Buffer** — Separate system-side signals from the LLM/user chat stream so wallet, backend, and external events can flow without polluting chat history.

---

## Branch Status

Current branch: `system-event-buff` (base: `main`)

**Recent Commits** (last 10):
```
2b66a45 all tests pass
a836cde all test pass
f4fc3c2 fix future -> stream -> result, split_first_chunk_and_rest
e7b97b8 refactor ToolResultFuture, clean separation of UI stream type and internal channels
403f0ec Fixed premature null results: single-result futures stay pending until oneshot completes
b33ea15 file separation, shorter code
6d3f1e7 file separation, shorter code
ddbfdff resolve first chunk problem
b311837 fixed panic of oneshot future stream
7fadf3c ToolResultSender enum (Oneshot/MultiStep)
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
| **ChatCommand cleanup** | Removed system variants from ChatCommand, now only contains: StreamingText, ToolCall, Complete, Error, Interrupted (lib.rs:75-93) |

### ChatApp Integration
| Change | Description |
|--------|-------------|
| **ChatAppBuilder changes** | Now accepts `SystemEventQueue` in constructor and passes through build process (app.rs) |
| **System event routing** | `MissingApiKey` and system notices now pushed to queue instead of ChatCommand channel |
| **`new()` and `new_with_retries()`** | Updated signatures to accept `SystemEventQueue` parameter |

### SessionState Changes (session.rs)
| Change | Description |
|--------|-------------|
| **system_event_queue field** | Added `SystemEventQueue` to SessionState struct |
| **system_events field** | Added `Vec<SystemEvent>` for drained events |
| **AomiBackend trait** | Added `fn system_events(&self) -> SystemEventQueue` method |
| **Removed ChatCommand variants** | Deleted handlers for `WalletTransactionRequest`, `System`, `BackendConnected`, `BackendConnecting`, `MissingApiKey` |
| **BackendConnected routing** | Now pushes `SystemEvent::BackendConnected` to queue instead of sending ChatCommand |

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

### ToolScheduler Refactor for Multi-Step Results (ddbfdff → 2b66a45)

**Purpose**: Enable multi-step tool calls to route subsequent results to system event buffer, allowing async tool progress to appear as UI notifications without polluting LLM chat history.

```
Architecture (final):

pending_results: Vec<ToolResultFuture>     pending_streams: Vec<ToolResultStream>
         │                                            │
         │ poll_futures_to_streams()                  │ poll_streams_to_next_result()
         │ converts futures → streams                 │ polls streams → ToolCompletion
         ▼                                            ▼
┌─────────────────────┐                    ┌─────────────────────┐
│  ToolResultFuture   │ ──────────────────▶│  ToolResultStream   │
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
| **tool_stream.rs (NEW)** | Separated `ToolResultFuture` and `ToolResultStream` into dedicated module |
| **ToolResultFuture** | Internal type holding raw channel receivers (`single_rx`, `multi_step_rx`) |
| **ToolResultStream** | UI-facing stream with metadata fields (`tool_name`, `is_multi_step`) |
| **ToolCompletion** | Return type from `poll_streams_to_next_result()` with full metadata |
| **into_shared_streams()** | Converts future → two streams: one for pending polling, one for UI ACK |
| **split_first_chunk_and_rest** | Multi-step spawns task to fan out first chunk to both streams |
| **Lock-free design** | Receiver owned exclusively by `ToolResultFuture`, no `Arc<Mutex>` needed |
| **poll_futures_to_streams()** | Polls `pending_futures`, converts ready futures to `pending_streams` |
| **poll_streams_to_next_result()** | Polls `pending_streams`, returns `ToolCompletion` |

**Key Files**:
- `crates/tools/src/tool_stream.rs` — `ToolResultFuture`, `ToolResultStream`, `ToolCompletion`, `ToolResultSender`
- `crates/tools/src/scheduler.rs` — `ToolScheduler`, `ToolApiHandler`
- `crates/tools/src/types.rs` — `AomiApiTool` trait with `MultiStepResults` associated type
- `crates/tools/src/test.rs` — Comprehensive test suite

### Multi-Step to SystemEventQueue (Phase 6)

**Purpose**: Route multi-step tool results to `SystemEventQueue` so frontend receives async tool progress as system events.

| Change | Description |
|--------|-------------|
| **types.rs** | Added `MultiStepResults` associated type to `AomiApiTool`, `validate_multi_step_result` method |
| **tool_stream.rs** | Added `ToolCompletion` struct, metadata fields on `ToolResultStream` |
| **lib.rs (chat)** | Added `AsyncToolResult` to `ChatCommand`, `SystemToolDisplay` to `SystemEvent` |
| **scheduler.rs** | `poll_streams_to_next_result()` returns `ToolCompletion` with metadata |
| **completion.rs** | Finalization loop yields `AsyncToolResult` for multi-step tools |
| **session.rs** | Matches `AsyncToolResult` → pushes `SystemToolDisplay` to queue |

**Flow**:
```
completion.rs finalization loop
  → poll_streams_to_next_result() yields ToolCompletion
  → if is_multi_step: yield ChatCommand::AsyncToolResult { call_id, tool_name, result }
  → session.rs matches AsyncToolResult
  → pushes SystemEvent::SystemToolDisplay { tool_name, call_id, result }
```

---

## Files Modified This Sprint

### Chat Crate
| File | Description |
|------|-------------|
| `crates/chat/src/lib.rs` | SystemEvent enum + SystemEventQueue implementation |
| `crates/chat/src/app.rs` | ChatAppBuilder accepts SystemEventQueue |
| `crates/chat/src/completion.rs` | Routes system events to queue |
| `crates/chat/src/connections.rs` | Uses SystemEventQueue for connection status |

### Backend Crate
| File | Description |
|------|-------------|
| `crates/backend/src/session.rs` | SessionState holds queue, AomiBackend trait updated |
| `crates/backend/tests/session_tests.rs` | Test updates for new API |
| `crates/backend/tests/utils.rs` | Test utility updates |

### Tools Crate
| File | Description |
|------|-------------|
| `crates/tools/src/tool_stream.rs` | **NEW** — ToolResultFuture, ToolResultStream, ToolResultSender, SchedulerRequest |
| `crates/tools/src/scheduler.rs` | ToolScheduler, ToolApiHandler (refactored, types moved to tool_stream.rs) |
| `crates/tools/src/types.rs` | AnyApiTool trait with multi_steps() and call_with_sender() |
| `crates/tools/src/test.rs` | **NEW** — Comprehensive test suite (moved from scheduler.rs) |
| `crates/tools/src/lib.rs` | Re-exports for scheduler types |

### Other Crates
| File | Description |
|------|-------------|
| `crates/eval/src/eval_app.rs` | Implements system_events() |
| `crates/eval/src/harness.rs` | Updated for new ChatApp API |
| `crates/l2beat/src/app.rs` | Implements system_events() |

### Specs
| File | Description |
|------|-------------|
| `specs/SYSTEM-BUS-PLAN.md` | Design document for system event architecture |
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

4. **Forge Executor implementation** (see EXECUTOR-PLAN.md):
   - BAML client setup for phase1/phase2
   - ForgeExecutor with plan-driven execution
   - Contract source fetching

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Wallet flow not fully wired | Pending | Uses old ChatCommand approach |
| Frontend doesn't consume system_events | Pending | sync_state returns them, UI needs update |

---

## Multi-Step Flow State

Current Position: Migration Phase (Steps 1-7 done, Step 8-9 pending)

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
| 6b | Add metadata fields to `ToolResultStream` (tool_name, is_multi_step) | ✓ Done |
| 6c | Add `AsyncToolResult` to ChatCommand, `SystemToolDisplay` to SystemEvent | ✓ Done |
| 6d | `poll_streams_to_next_result()` returns `ToolCompletion` | ✓ Done |
| 6e | Finalization loop yields `AsyncToolResult` for multi-step tools | ✓ Done |
| 6f | session.rs matches `AsyncToolResult` → pushes `SystemToolDisplay` | ✓ Done |
| 7 | Wire wallet flow through system events | Pending |
| 8 | Update sync_state() to return system events | ✓ Done |
| 9 | Frontend integration | Pending |

---

## Notes for Next Agent

### Critical Context

1. **Architecture goal**
   - Separate system events from LLM chat stream
   - Two buffers: `ChatCommand` for chat, `SystemEventQueue` for system
   - UI can consume both independently
   - Agent only sees system events explicitly injected
   - Multi-step tool results flow to system events (async notifications)

2. **Current state**
   - `SystemEvent` enum and `SystemEventQueue` implemented (chat/src/lib.rs)
   - `ChatCommand` cleaned up - no longer has system variants
   - `ChatApp` and `SessionState` hold queue references
   - `processed_system_event_idx` tracks consumption (session.rs:72)
   - `sync_state()` returns `system_events` alongside messages
   - **ToolScheduler refactored** for multi-step support (scheduler.rs)

3. **ToolScheduler Architecture** (Updated)
   - **tool_stream.rs**: `ToolResultFuture` (internal channels) and `ToolResultStream` (UI-facing)
   - **Two-phase conversion**: `pending_results` → `poll_results_to_streams()` → `pending_streams` → `finalize_tool_results()` → chat_history
   - **into_shared_streams()**: Converts future to two streams (pending + UI ACK)
   - **Multi-step fanout**: Spawns task to send first chunk to both streams, forwards rest to pending
   - **Single-result**: Uses `Shared<BoxFuture>` so both streams get same value
   - Lock-free design: receiver owned exclusively by future, no `Arc<Mutex>`

4. **What's missing**
   - Wallet transaction flow needs to use system events
   - Frontend needs to consume system_events from sync_state response
   - Rig tools fallback commented out in completion.rs — restore or remove

5. **Design references**
   - `specs/SYSTEM-BUS-PLAN.md` — System event architecture
   - `specs/EXECUTOR-PLAN.md` — Forge Executor implementation plan

### Key Files
```
aomi/crates/tools/src/tool_stream.rs     # ToolResultFuture, ToolResultStream, ToolResultSender (NEW)
aomi/crates/tools/src/scheduler.rs       # ToolScheduler, ToolApiHandler
aomi/crates/tools/src/test.rs            # Comprehensive test suite (NEW)
aomi/crates/tools/src/types.rs           # AnyApiTool trait with multi_steps(), call_with_sender()
aomi/crates/chat/src/lib.rs              # SystemEvent + SystemEventQueue
aomi/crates/chat/src/completion.rs       # Stream completion loop (TODO: multi-step → system event routing)
aomi/crates/backend/src/session.rs       # SessionState with system event handling
specs/SYSTEM-BUS-PLAN.md                 # System event design document
```

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
```

### Implementation Next Steps

**Restore or remove Rig tools fallback** — completion.rs:
- Currently commented out, returns `ToolNotFoundError` for non-scheduler tools
- Either restore for MCP/Rig tools or remove dead code

**Wallet flow wiring** — Update wallet transaction handling to use SystemEvent:
1. Replace ChatCommand wallet variants with SystemEvent equivalents
2. Push `WalletTxRequest` to system queue when transaction initiated
3. Handle `WalletTxResponse` from queue and optionally inject into agent history

**Frontend integration** — Update UI to consume system_events:
1. Parse `system_events` from sync_state response
2. Render system notices separately from chat
3. Show multi-step tool progress as notifications
4. Handle wallet transaction UI state from system events
