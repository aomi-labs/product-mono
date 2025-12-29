# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**Event Manager Refactor** — Centralize tool completion routing through SystemEventQueue with per-consumer counters, enabling sync/async tool result delivery to both UI and LLM paths without blocking.

---

## Branch Status

Current branch: `system-event-manager` (base: `main`)

**Recent Commits** (last 10):
```
aea08240 simplified session manager
4638ab77 fn finalization(system_events: SystemEventQueue, ...)
270b1cf3 scheduler in SessionState
b9ce9ab9 completed_calls, resolve_calls
5d97d3c9 new_with_default_tools
486f4b4b update + some notes
22703da8 support SystemEvent in cli + MultiStepApiTool for registration
c8a18a43 shuffling files
44cb792d merged main
efc2742b backend/types.rs
```

---

## Recently Completed Work

### Event Manager Architecture (aea08240)

**Purpose**: Non-blocking tool completion delivery via SystemEventQueue with sync/async distinction.

```
Architecture:

┌─────────────────────┐     poll_streams_once()     ┌─────────────────────┐
│   ToolApiHandler    │ ─────────────────────────▶  │   completed_calls   │
│  (Arc<Mutex<...>>)  │                             │  Vec<ToolCompletion>│
│  - ongoing_streams  │                             │  - sync: bool       │
│  - completed_calls  │                             └─────────────────────┘
└─────────────────────┘                                       │
         ▲                                                    │ take_completed_calls()
         │                                                    ▼
         │                                        ┌─────────────────────┐
   Background Poller                              │  SystemEventQueue   │
   (spawned per session)                          │  - SyncUpdate       │
         │                                        │  - AsyncUpdate      │
         └────── push_tool_update() ─────────────▶│  - counters per     │
                                                  │    consumer         │
                                                  └─────────────────────┘
                                                     │            │
                              advance_llm_events()   │            │  advance_frontend_events()
                                                     ▼            ▼
                                              ┌──────────┐  ┌──────────┐
                                              │   LLM    │  │    UI    │
                                              │  (sync + │  │  (all    │
                                              │  async)  │  │  events) │
                                              └──────────┘  └──────────┘
```

| Change | Description |
|--------|-------------|
| **ToolCompletion.sync** | Renamed from `is_multi_step`; `sync=true` for single-step + first chunk, `sync=false` for follow-ups |
| **ToolResultStream.first_chunk_sent** | Tracks whether first chunk has been emitted for multi-step streams |
| **is_multi_step()** | Method inspects channel type (`StreamInner::Multi`) instead of boolean field |
| **poll_streams_once()** | Non-blocking single-pass; keeps multi-step streams alive, removes single-step |
| **Background poller** | Spawned in `SessionState::new`, 50ms backoff, pushes to SystemEventQueue |
| **Handler in Arc<Mutex>** | Shared between LLM path and background poller |

### SystemEventQueue Counters

| Change | Description |
|--------|-------------|
| **SystemEventQueueInner** | Internal struct with `events`, `frontend_event_cnt`, `llm_event_cnt` |
| **advance_frontend_events()** | Returns all new events, advances counter |
| **advance_llm_events()** | Returns only `SyncUpdate`, `AsyncUpdate`, `SystemError`; advances counter |
| **push_tool_update()** | Emits `SyncUpdate` or `AsyncUpdate` based on `completion.sync` |

### SystemEvent Types

| Variant | Routing | Description |
|---------|---------|-------------|
| `InlineDisplay(Value)` | UI only | Sync JSON events (wallet_tx_request) |
| `SystemNotice(String)` | UI only | Notices like title updates |
| `SystemError(String)` | UI + LLM | Errors both need to know |
| `SyncUpdate(Value)` | UI + LLM | Sync tool results (single-step or first chunk) |
| `AsyncUpdate(Value)` | UI + LLM | Async tool results (follow-up chunks) |

### stream_completion Finalization

| Change | Description |
|--------|-------------|
| **finalization()** | Loops on `advance_llm_events()` after LLM finishes |
| **SyncUpdate handling** | Calls `finalize_sync_tool()` to add tool result to history |
| **AsyncUpdate handling** | Calls `finalize_async_completion()` with system hint |
| **MAX_IDLE_LOOPS** | Breaks after 10 iterations with no events |

---

## Files Modified This Sprint

### Tools Crate
| File | Description |
|------|-------------|
| `crates/tools/src/streams.rs` | `ToolCompletion.sync`, `ToolResultStream.first_chunk_sent`, `is_multi_step()` method |
| `crates/tools/src/scheduler.rs` | `poll_streams_once()` sync/async logic, `completed_calls`, `take_completed_calls()` |

### Chat Crate
| File | Description |
|------|-------------|
| `crates/chat/src/lib.rs` | `SyncUpdate`/`AsyncUpdate` variants, counter-based queue, `push_tool_update()` |
| `crates/chat/src/completion.rs` | `finalization()` function, handler as `Arc<Mutex>` |

### Backend Crate
| File | Description |
|------|-------------|
| `crates/backend/src/session.rs` | Background poller spawn, `sync_system_events()` uses `advance_frontend_events()` |
| `crates/backend/src/types.rs` | `AomiBackend` trait updated with handler parameter |

### Specs
| File | Description |
|------|-------------|
| `specs/EVENT-MANAGER.md` | Updated plan with sync/async distinction |

---

## Pending Tasks

### Immediate (Compile Fixes)

1. **Fix parameter order in test implementations**:
   - `test_backend.rs` has `handler` and `system_events` swapped vs trait
   - Other `AomiBackend` implementations may need updates

2. **Add sleep in finalization loop**:
   - Currently tight loop when waiting for events
   - Should yield between iterations

### Short-Term

3. **Poller shutdown**:
   - Currently runs forever
   - Should tie to session lifetime (cancellation token)

4. **Test counter semantics**:
   - Verify UI and LLM each see events exactly once
   - Test sync vs async delivery

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Parameter order mismatch | Active | `test_backend.rs` has wrong param order vs trait |
| Tight finalization loop | Active | Missing sleep when no events |
| Poller never stops | Active | No shutdown mechanism |

---

## Notes for Next Agent

### Critical Context

1. **Sync vs Async tool completions**:
   - `sync=true`: Single-step tools OR first chunk of multi-step
   - `sync=false`: Follow-up chunks from multi-step tools
   - Both go to `SystemEventQueue` via `push_tool_update()`
   - LLM sees both via `advance_llm_events()`
   - UI sees all events via `advance_frontend_events()`

2. **Handler sharing**:
   - `ToolApiHandler` wrapped in `Arc<Mutex<...>>`
   - Shared between LLM scheduling (process_tool_call) and background poller
   - Short lock spans, no locks held across awaits

3. **Two consumers, one queue**:
   - `frontend_event_cnt` for UI path
   - `llm_event_cnt` for LLM path
   - Each consumer gets exactly-once delivery
   - Events never removed (append-only)

### Key Files
```
aomi/crates/tools/src/streams.rs         # ToolCompletion, ToolResultStream
aomi/crates/tools/src/scheduler.rs       # poll_streams_once, completed_calls
aomi/crates/chat/src/lib.rs              # SystemEventQueue with counters
aomi/crates/chat/src/completion.rs       # finalization(), stream_completion
aomi/crates/backend/src/session.rs       # Background poller, sync_system_events
specs/EVENT-MANAGER.md                   # Current plan
```

### Quick Start Commands
```bash
# Check compilation
cd aomi && cargo check

# Run tests
cargo test --all

# Run scheduler tests specifically
cargo test --package aomi-tools
```
