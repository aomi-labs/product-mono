# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**System Event Buffer** — Separate system-side signals from the LLM/user chat stream so wallet, backend, and external events can flow without polluting chat history.

---

## Branch Status

Current branch: `system-event-buff` (base: `main`)

**Recent Commits** (last 5):
```
c8f43fc move the initialization of system_events to where history started
a905490 processed_system_event_idx
a945e7e system_event_queue
28bca42 claude cmd
5c84cec add specs
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
   - Verify events reach UI correctly
   - Test wallet request/response cycle
   - Test connection status updates

### Short-Term

3. **Update frontend integration**:
   - Handle system events in sync_state response
   - Render system notices separately from chat

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

Current Position: Migration Phase (Steps 1-4 done, Step 5 in progress)

| Step | Description | Status |
|------|-------------|--------|
| 1 | Define SystemEvent + SystemEventQueue | ✓ Done |
| 2 | Inject queue into ChatApp/SessionState constructors | ✓ Done |
| 3 | Update stream_completion to route system signals | ✓ Done |
| 4 | Update SessionState::update_state to drain queue | ✓ Done |
| 5 | Wire wallet flow through system events | In Progress |
| 6 | Update sync_state() to return system events | ✓ Done |
| 7 | Frontend integration | Pending |

---

## Notes for Next Agent

### Critical Context

1. **Architecture goal**
   - Separate system events from LLM chat stream
   - Two buffers: `ChatCommand` for chat, `SystemEventQueue` for system
   - UI can consume both independently
   - Agent only sees system events explicitly injected

2. **Current state**
   - `SystemEvent` enum and `SystemEventQueue` implemented (chat/src/lib.rs)
   - `ChatCommand` cleaned up - no longer has system variants
   - `ChatApp` and `SessionState` hold queue references
   - `processed_system_event_idx` tracks consumption (session.rs:72)
   - `slice_from()` efficiently retrieves new events (session.rs:309)
   - `sync_state()` returns `system_events` alongside messages (session.rs:466)
   - System events initialization moved to where history starts

3. **What's missing**
   - Wallet transaction flow needs to use system events
   - Frontend needs to consume system_events from sync_state response

4. **Design references**
   - `specs/SYSTEM-BUS-PLAN.md` — System event architecture
   - `specs/EXECUTOR-PLAN.md` — Forge Executor implementation plan

### Key Files
```
aomi/crates/chat/src/lib.rs              # SystemEvent + SystemEventQueue
aomi/crates/chat/src/app.rs              # ChatApp with queue injection
aomi/crates/backend/src/session.rs       # SessionState with processed_system_event_idx
specs/SYSTEM-BUS-PLAN.md                 # System event design document
specs/EXECUTOR-PLAN.md                   # Forge Executor design document
```

### Quick Start Commands
```bash
# Check compilation
cargo check --all

# Run clippy
cargo clippy --all

# Run tests
cargo test --all
```

### Implementation Next Steps

**Wallet flow wiring** — Update wallet transaction handling to use SystemEvent:
1. Replace ChatCommand wallet variants with SystemEvent equivalents
2. Push `WalletTxRequest` to system queue when transaction initiated
3. Handle `WalletTxResponse` from queue and optionally inject into agent history

**Frontend integration** — Update UI to consume system_events:
1. Parse `system_events` from sync_state response
2. Render system notices in separate UI area
3. Handle wallet transaction UI state from system events
