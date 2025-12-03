# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**System Event Buffer** — Separate system-side signals from the LLM/user chat stream so wallet, backend, and external events can flow without polluting chat history.

---

## Branch Status

Current branch: `system-event-buff` (base: `main`)

**Recent Commits** (last 3):
```
a945e7e system_event_queue
28bca42 claude cmd
5c84cec add specs
```

---

## Recently Completed Work

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

1. **Complete system event drain in SessionState**:
   - Add second loop in `update_state()` to drain `system_event_queue`
   - Process UI-only events (append `MessageSender::System`)
   - Update state flags (connection status, pending wallet tx)
   - Optionally inject into agent history where needed

2. **Implement sync_state() system event handling**:
   - Return both chat messages and system events
   - Allow UI to render chat and system notices independently

3. **Wire wallet flow through system events**:
   - `WalletTxRequest` → system buffer → UI pending flag
   - `WalletTxResponse` → UI + optional LLM injection

### Short-Term

4. **Test system event flow end-to-end**:
   - Verify events reach UI correctly
   - Test wallet request/response cycle
   - Test connection status updates

5. **Update frontend integration**:
   - Handle system events in sync_state response
   - Render system notices separately from chat

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| System event drain not yet implemented | In progress | `update_state()` needs second loop |
| sync_state() doesn't return system events | Pending | UI can't see system events yet |
| Wallet flow not fully wired | Pending | Uses old ChatCommand approach |

---

## Multi-Step Flow State

Current Position: Migration Phase (Steps 1-3 done, Step 4 in progress)

| Step | Description | Status |
|------|-------------|--------|
| 1 | Define SystemEvent + SystemEventQueue | ✓ Done |
| 2 | Inject queue into ChatApp/SessionState constructors | ✓ Done |
| 3 | Update stream_completion to route system signals | ✓ Done |
| 4 | Update SessionState::update_state to drain queue | In Progress |
| 5 | Wire wallet flow through system events | Pending |
| 6 | Update sync_state() to return system events | Pending |
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
   - `SystemEvent` enum and `SystemEventQueue` are implemented (chat/src/lib.rs)
   - `ChatCommand` cleaned up - no longer has system variants
   - `ChatApp` and `SessionState` hold queue references
   - `AomiBackend` trait requires `system_events()` method
   - Events are being pushed to queue, but not yet drained/processed

3. **What's missing**
   - `SessionState::update_state()` needs to drain the queue after processing ChatCommands
   - `sync_state()` needs to return system events alongside messages
   - Wallet transaction flow needs to use system events

4. **Design reference**
   - See `specs/SYSTEM-BUS-PLAN.md` for full architecture
   - Routing rules, buffer patterns, and integration points documented there

### Key Files
```
aomi/crates/chat/src/lib.rs              # SystemEvent + SystemEventQueue
aomi/crates/chat/src/app.rs              # ChatApp with queue injection
aomi/crates/backend/src/session.rs       # SessionState with queue (needs drain)
specs/SYSTEM-BUS-PLAN.md                 # Design document
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

In `session.rs`:
```rust
// After the ChatCommand while loop in update_state():
// Drain system events
for event in self.system_event_queue.drain() {
    match event {
        SystemEvent::BackendConnected => {
            if !self.has_sent_welcome {
                self.add_assistant_message(ASSISTANT_WELCOME);
                self.has_sent_welcome = true;
            }
        }
        SystemEvent::SystemNotice(msg) | SystemEvent::SystemError(msg) => {
            self.add_system_message(&msg);
        }
        SystemEvent::WalletTxRequest { payload } => {
            self.pending_wallet_tx = Some(payload.to_string());
            self.add_system_message("Transaction request sent...");
        }
        // ... handle other variants
        _ => {}
    }
    self.system_events.push(event);
}
```
