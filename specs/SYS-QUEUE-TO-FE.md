# System Event Queue to Frontend

> Plan for unifying system events and routing them to frontend via SSE

---

## Goal

Unify `SystemUpdate` and `SystemEvent` into a single event system that:
1. Separates inline events (sync with chat stream) from async events (background notifications)
2. Routes async events + global updates through a single SSE endpoint
3. Provides a GET endpoint for frontend to pull full async event data

---

## Current State

### Two Separate Event Types

**SystemUpdate** (session.rs:18-23) - global updates:
```rust
pub enum SystemUpdate {
    TitleChanged { session_id, new_title }
}
```

**SystemEvent** (chat/lib.rs:23-46) - session events:
```rust
pub enum SystemEvent {
    SystemBroadcast(String),
    SystemNotice(String),
    SystemError(String),
    SystemToolDisplay { tool_name, call_id, result },
    WalletTxRequest { payload },
    WalletTxResponse { status, tx_hash, detail },
    UserRequest { kind, payload },
    UserResponse { kind, payload },
}
```

### Current Flow

```
system_event_queue: [A, B, C, D, E, ...]
                     │  │  │  │  │
                     ▼  ▼  ▼  ▼  ▼
              handle_system_event()
                     │  │  │  │  │
                     ▼  ▼  ▼  ▼  ▼
   active_system_events: [A, B, C, D, E]  ← everything goes here (no filtering)
```

**Problems:**
- No differentiation between inline vs async events
- `broadcasted_system_event_idx` defined but unused
- `SystemUpdate` and `SystemEvent` are separate systems
- SSE endpoint only streams global `SystemUpdate`, not session async events

---

## Target State

### Unified SystemEvent Enum

```rust
// chat/lib.rs
pub enum SystemEvent {
    // Inline - flows with chat stream (path 1)
    InlineNotification(Value),  // {"type": "tool_display"|"wallet_request"|..., ...}
    SystemNotice(String),
    SystemError(String),

    // Async - background updates (path 2)
    AsyncUpdate(Value),         // {"type": "title_changed"|"task_complete"|..., "session_id": "...", ...}
}
```

### Target Flow

```
system_event_queue: [A, B, C, D, E, ...]
                     │  │  │  │  │
                     ▼  ▼  ▼  ▼  ▼
              handle_system_event() decides by variant:
                     │  │  │  │  │
    InlineNotification/Notice/Error?
                    yes no yes yes no
                     │  │  │  │  │
                     ▼     ▼  ▼
   active_system_events: [A, C, D]     (path 1 - via ChatState.system_events)
                        │     │
                        ▼     ▼
     pending_async_events: [B, E]      (path 2 - via SSE notification + GET)
```

### SSE Stream Content

```
updates_endpoint SSE:
  ├── Global updates (from background task)
  │   └── {"type": "title_changed", "session_id": "abc", "new_title": "..."}
  │
  └── Session async notifications (from background task polling)
      └── {"type": "tool_complete", "session_id": "abc", "tool_name": "wallet", "call_id": "xyz"}
```

---

## Implementation Plan

### Phase 1: Simplify SystemEvent Enum

**File: `crates/chat/src/lib.rs`**

1. Replace current variants with:
```rust
pub enum SystemEvent {
    // Inline (path 1)
    InlineNotification(Value),
    SystemNotice(String),
    SystemError(String),

    // Async (path 2)
    AsyncUpdate(Value),
}
```

2. Remove old variants:
   - `SystemBroadcast` → becomes `AsyncUpdate`
   - `SystemToolDisplay` → becomes `InlineNotification({"type": "tool_display", ...})`
   - `WalletTxRequest` → becomes `InlineNotification({"type": "wallet_tx_request", ...})`
   - `WalletTxResponse` → becomes `InlineNotification({"type": "wallet_tx_response", ...})`
   - `UserRequest` → becomes `InlineNotification({"type": "user_request", ...})`
   - `UserResponse` → becomes `InlineNotification({"type": "user_response", ...})`

### Phase 2: Update SessionState

**File: `crates/backend/src/session.rs`**

1. Add new field:
```rust
pub struct SessionState<S> {
    // ... existing ...
    pub system_event_queue: SystemEventQueue,
    pub active_system_events: Vec<SystemEvent>,    // inline (path 1)
    pending_async_events: Vec<Value>,              // NEW: async (path 2)
}
```

2. Update `handle_system_event()`:
```rust
async fn handle_system_event(&mut self, event: SystemEvent) {
    match event {
        // Inline events → active_system_events
        SystemEvent::InlineNotification(v) => {
            self.active_system_events.push(SystemEvent::InlineNotification(v));
        }
        SystemEvent::SystemNotice(msg) => {
            self.active_system_events.push(SystemEvent::SystemNotice(msg));
        }
        SystemEvent::SystemError(err) => {
            self.active_system_events.push(SystemEvent::SystemError(err));
        }

        // Async events → pending_async_events
        SystemEvent::AsyncUpdate(v) => {
            self.pending_async_events.push(v);
        }
    }
}
```

3. Add methods:
```rust
/// Get pending async notifications (metadata only, for SSE)
pub fn get_pending_async_notifications(&self) -> Vec<Value> {
    self.pending_async_events.clone()
}

/// Consume async events (for GET endpoint)
pub fn take_async_events(&mut self) -> Vec<Value> {
    std::mem::take(&mut self.pending_async_events)
}
```

### Phase 3: Remove SystemUpdate, Unify Broadcast Channel

**File: `crates/backend/src/session.rs`**

1. Delete `SystemUpdate` enum (line 16-23)

**File: `crates/backend/src/manager.rs`**

2. Change broadcast channel type:
```rust
pub struct SessionManager {
    // ... existing ...
    pub(crate) system_update_tx: broadcast::Sender<Value>,  // was: broadcast::Sender<SystemUpdate>
}
```

**File: `crates/backend/src/title.rs`**

3. Update title generation to send `Value`:
```rust
let _ = self.system_update_tx.send(serde_json::json!({
    "type": "title_changed",
    "session_id": session_id,
    "new_title": title
}));
```

### Phase 4: Rename title.rs → background.rs

**File: `crates/backend/src/background.rs`** (renamed from title.rs)

1. Rename `start_title_generation_task` → `start_background_tasks`

2. Add async notification processing:
```rust
impl SessionManager {
    pub fn start_background_tasks(self: Arc<Self>) {
        let manager = Arc::clone(&self);
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        tokio::spawn(async move {
            loop {
                interval.tick().await;
                manager.process_title_generation().await;
                manager.broadcast_async_notifications().await;
            }
        });
    }

    /// Collect pending async events from all sessions and broadcast them
    async fn broadcast_async_notifications(&self) {
        for entry in self.sessions.iter() {
            let session_id = entry.key().clone();
            let session_data = entry.value();

            // Try to get pending notifications without blocking
            if let Ok(state) = session_data.state.try_lock() {
                for notification in state.get_pending_async_notifications() {
                    // notification already has session_id baked in
                    let _ = self.system_update_tx.send(notification);
                }
            }
        }
    }
}
```

3. Update lib.rs:
```rust
pub mod history;
pub mod manager;
pub mod session;
mod background;  // was: mod title
```

### Phase 5: Update SSE Endpoint

**File: `bin/backend/src/endpoint/system.rs`**

1. Update stream to handle `Value` instead of `SystemUpdate`:
```rust
async fn updates_endpoint(
    State(session_manager): State<SharedSessionManager>,
) -> Result<Sse<impl StreamExt<Item = Result<Event, Infallible>>>, StatusCode> {
    let rx = session_manager.subscribe_to_updates();

    let stream = BroadcastStream::new(rx)
        .filter_map(|result| match result {
            Ok(update) => Event::default().json_data(&update).ok(),  // update is now Value
            Err(_) => None,
        })
        .map(Ok::<_, Infallible>);

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
```

### Phase 6: Add GET Endpoint for Async Events

**File: `bin/backend/src/endpoint/system.rs`**

1. Add new endpoint:
```rust
async fn get_async_events_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<Value>>, StatusCode> {
    let session_id = params.get("session_id").ok_or(StatusCode::BAD_REQUEST)?;

    let session_state = session_manager
        .get_session_if_exists(session_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let events = {
        let mut state = session_state.lock().await;
        state.take_async_events()
    };

    Ok(Json(events))
}
```

2. Register route:
```rust
pub fn create_system_router() -> Router<SharedSessionManager> {
    Router::new()
        .route("/updates", get(updates_endpoint))
        .route("/events", get(get_async_events_endpoint))  // NEW
        .route("/system", post(system_message_endpoint))
        .route("/memory-mode", post(memory_mode_endpoint))
}
```

### Phase 7: Update All Event Push Sites

**Files to update:**

1. `crates/backend/src/session.rs`:
   - Line 148: `SystemNotice("Backend connected")` - keep as is
   - Line 197: `SystemError(...)` - keep as is
   - Line 236: `SystemError(...)` - keep as is
   - Line 240: `UserRequest {...}` → `InlineNotification(json!({...}))`
   - Line 319: `SystemError(...)` - keep as is
   - Line 446: `SystemToolDisplay {...}` → `InlineNotification(json!({...}))`

2. `crates/chat/src/completion.rs`:
   - Any `SystemEvent` pushes need to use new variants

3. Other crates that push to `SystemEventQueue`

---

## Migration Checklist

- [ ] Phase 1: Simplify SystemEvent enum in chat/lib.rs
- [ ] Phase 2: Update SessionState with pending_async_events
- [ ] Phase 3: Remove SystemUpdate, change broadcast to Value
- [ ] Phase 4: Rename title.rs → background.rs, add broadcast_async_notifications
- [ ] Phase 5: Update SSE endpoint for Value type
- [ ] Phase 6: Add GET /events endpoint
- [ ] Phase 7: Update all event push sites
- [ ] Run cargo check --all
- [ ] Run cargo test --all
- [ ] Manual test: verify inline events appear in ChatState
- [ ] Manual test: verify async events appear in SSE stream
- [ ] Manual test: verify GET /events returns and consumes async events

---

## JSON Payload Formats

### Inline Notifications

```json
// Tool display
{"type": "tool_display", "tool_name": "wallet", "call_id": "abc123", "result": {...}}

// Wallet request
{"type": "wallet_tx_request", "payload": {...}}

// Wallet response
{"type": "wallet_tx_response", "status": "confirmed", "tx_hash": "0x...", "detail": "..."}

// User request
{"type": "user_request", "kind": "interruption", "payload": {...}}
```

### Async Updates

```json
// Title changed (global)
{"type": "title_changed", "session_id": "abc", "new_title": "New Chat Title"}

// Background task complete (session-specific)
{"type": "task_complete", "session_id": "abc", "task_id": "xyz", "result": {...}}
```

---

## Notes

- All JSON payloads include a `type` field for frontend routing
- Async updates include `session_id` so SSE stream can be session-aware
- Frontend subscribes to SSE once, filters by session_id client-side
- GET /events is called per-session to retrieve full event data
- Background task interval: 5 seconds (same as title generation)
