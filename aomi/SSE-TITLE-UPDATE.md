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

### 2. SessionState Addition (session.rs)

Added message-based tracking for summarization:
```rust
pub struct DefaultSessionState<S: State> {
    // ... existing fields ...
    last_summarized_msg: usize,  // Tracks # of messages when last summarized
}
```

New methods for smart summarization:
```rust
pub fn need_summarize(&self) -> bool {
    // Returns true if:
    // 1. NOT archived
    // 2. NOT currently processing
    // 3. Title is None or looks like default (≤6 chars)
    // 4. messages.len() > last_summarized_msg + 5 (at least 5 new messages)
}

pub fn mark_summarized(&mut self) {
    // Call after successful summarization
    // Updates last_summarized_msg = messages.len()
}
```

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

Simplified using `state.need_summarize()`:
```rust
// Check if session needs summarization
if !state.need_summarize() {
    continue;  // Skip archived, processing, or has custom title
}

// Has at least one non-system message
if state.messages.is_empty() {
    continue;
}

// Call BAML API...
// On success:
state.set_title(result.title.clone());
state.mark_summarized();  // Update last_summarized_msg
manager.system_update_tx.send(SystemUpdate::TitleChanged { ... });
```

**Smart skipping logic:**
- Archived sessions - won't summarize
- Active chats - won't summarize (user still typing)
- Custom titles - won't overwrite
- **Fewer than 5 new messages** - won't summarize (message-based throttling)

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
- [x] Add `last_summarized_msg: usize` field to SessionState
- [x] Add `need_summarize()` method to SessionState
  - Checks archived, processing, custom title, message count
- [x] Add `mark_summarized()` method to SessionState
- [x] Add global `system_update_tx` to SessionManager
- [x] Add `subscribe_to_updates()` getter to SessionManager
- [x] Simplify background job to use `state.need_summarize()`
- [x] Call `state.mark_summarized()` after successful title generation
- [x] Add `/api/updates` endpoint (no session_id param)
- [x] Add route to router
- [x] Compile and verify ✅
- [x] All tests passing ✅

**Frontend**:
- [ ] Connect to `/api/updates` once on app init (not per-session)
- [ ] Handle TitleChanged events and filter by session_id
- [ ] Update UI for specific session when title changes
- [ ] Clean up EventSource on app shutdown

## How It Works: Message-Based Throttling

Instead of time-based checks (e.g., "last message was 5 seconds ago"), we use **message count**:

1. **First message**: User sends first message, `messages=[user_msg]`
   - `last_summarized_msg = 0` initially
   - `messages.len() (1) > 0 + 5 = false` → skip

2. **Five+ messages**: Conversation continues to 5+ messages
   - `messages.len() (5) > 0 + 5 = false` → skip
   - `messages.len() (6) > 0 + 5 = true` → summarize!
   - `mark_summarized()` sets `last_summarized_msg = 6`

3. **More messages**: User adds 3 more messages (total 9)
   - `messages.len() (9) > 6 + 5 = false` → skip

4. **10+ new messages**: Continues to 16 messages
   - `messages.len() (16) > 6 + 5 = true` → summarize again!
   - `mark_summarized()` sets `last_summarized_msg = 16`

**Benefits:**
- Independent of time/wall-clock
- Adapts to conversation pace
- No more "was idle check" needed
- Simple and predictable

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
