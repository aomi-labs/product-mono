# System Updates via SSE - `/api/updates/...`

## Problem
Backend title auto-generation (5s background job) has no mechanism to notify frontend of changes.

## Solution
Generalized system update channel for title updates + future system-wide events.

## Design

### 1. SystemUpdate Enum (session.rs)
```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum SystemUpdate {
    TitleChanged {
        session_id: String,
        new_title: String,
    },
    // Future: ProcessingStateChanged, MemoryModeChanged, etc.
}
```

### 2. SessionState (No Changes)
SessionState no longer has per-session broadcast channel - uses global manager channel instead.

### 3. SessionManager Addition (manager.rs)
```rust
pub struct SessionManager {
    // ... existing fields ...
    system_update_tx: broadcast::Sender<SystemUpdate>,
}
```

Getter method:
```rust
pub fn subscribe_to_updates(&self) -> broadcast::Receiver<SystemUpdate> {
    self.system_update_tx.subscribe()
}
```

### 4. Manager Background Job (manager.rs)
**5-second title summarization loop:**
- Skips sessions that are still processing (`is_processing == true`)
- Only processes sessions with messages and default/no title
- Calls BAML API to generate meaningful title
- Broadcasts update to global channel:
```rust
let _ = manager.system_update_tx.send(SystemUpdate::TitleChanged {
    session_id: session_id.clone(),
    new_title: result.title.clone(),
});
```

### 5. New Endpoint: `/api/updates` (endpoint.rs)

**GET /api/updates** - Returns global SSE stream for all title changes
```rust
async fn updates_endpoint(
    State(session_manager): State<SharedSessionManager>,
) -> Result<Sse<impl StreamExt<Item = Result<Event, Infallible>>>, StatusCode> {
    let rx = session_manager.subscribe_to_updates();

    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(update) => Event::default()
                .json_data(&update)
                .ok(),
            Err(_) => None,
        }
    }).map(|event| Ok::<_, Infallible>(event));

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
```

Add to router:
```rust
.route("/api/updates", get(updates_endpoint))
```

## Frontend Usage

**Subscribe once on app init (not per-session):**
```javascript
// Single global connection for ALL sessions
const eventSource = new EventSource(`/api/updates`);

eventSource.addEventListener('message', (event) => {
    const update = JSON.parse(event.data);

    if (update.type === 'TitleChanged') {
        // update.data.session_id tells you which session was updated
        const sessionId = update.data.session_id;
        const newTitle = update.data.new_title;

        // Update the UI for this specific session
        updateSessionTitle(sessionId, newTitle);
    }
    // Future events handled here
});
```

**Why one connection instead of N?**
- 1 SSE connection handles updates for ALL 10+ sessions
- Lower memory footprint
- Simpler connection management
- All updates flow through one channel, filtered by session_id

## Implementation Checklist

**Backend** ✅:
- [x] Add `SystemUpdate` enum to session.rs
- [x] Add global `system_update_tx` to SessionManager
- [x] Add `subscribe_to_updates()` getter to SessionManager
- [x] Removed per-session broadcast channel (SessionState no longer needs it)
- [x] Broadcast to global channel only in background job
- [x] Skip sessions that are still processing (optimization)
- [x] Add `/api/updates` endpoint (no session_id param)
- [x] Add route to router
- [x] Compile and verify ✅

**Frontend**:
- [ ] Connect to `/api/updates` once on app init (not per-session)
- [ ] Handle TitleChanged events and filter by session_id
- [ ] Update UI for specific session when title changes
- [ ] Clean up EventSource on app shutdown

## Future Extensions

Easily add more system updates:
```rust
pub enum SystemUpdate {
    TitleChanged {
        session_id: String,
        new_title: String,
    },
    ProcessingStarted { session_id: String },
    ProcessingFinished { session_id: String },
    MemoryModeChanged { session_id: String, enabled: bool },
    ToolExecutionStatus { session_id: String, tool_name: String, status: String },
}
```

Same endpoint handles all - frontend filters by `type` field. Each event includes `session_id` for routing/validation.
