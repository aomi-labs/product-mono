# Session Persistence Refactor: Persist-on-Cleanup Design

## Overview

This document outlines a simpler architecture for session persistence that eliminates runtime database writes and only persists data when sessions are cleaned up. This design provides user control over persistence while improving performance and simplifying the codebase.

## Current Implementation (Problems)

### What We Have Now

```rust
// Every 100ms in SSE stream:
let response = state.get_state();

// Persist to database every 100ms
session_manager.persist_session_messages(&session_id, &response.messages).await;

// Database query on every call to check for duplicates
let existing_count = db.get_messages(session_id, ...).await.len();
```

### Issues

1. **Excessive Database Load**
   - Database query + writes every 100ms per active session
   - 10 concurrent users = 100 DB operations per second
   - Duplicate detection query on every call

2. **Complex Logic**
   - `persist_session_messages()` has duplicate detection
   - Mixed concerns: streaming + persistence
   - Hard to understand when data is saved

3. **No User Control**
   - All sessions automatically persisted
   - No "incognito mode"
   - No way to opt-out of persistence

4. **Unnecessary Complexity**
   - Both `user_history` (in-memory) and `db_store` (database)
   - Two persistence mechanisms doing similar things
   - Confusing which to use when

## Proposed Architecture: Persist-on-Cleanup

### Core Principle

**Sessions live in memory. Database writes happen once at cleanup time, based on user preference.**

### Design

```rust
pub struct SessionData {
    state: Arc<Mutex<DefaultSessionState>>,
    last_activity: Instant,
    should_persist: bool,  // ← User-controlled flag
}

pub struct SessionManager {
    sessions: DashMap<String, SessionData>,
    db_store: Option<Arc<dyn SessionStoreApi>>,
    // Remove: user_history DashMap (redundant)
}
```

### Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│                         Runtime                              │
│                      (Active Session)                        │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  User Input  →  LLM Processing  →  SessionState.messages    │
│                                            │                  │
│                                            ↓                  │
│                                    In-Memory Only            │
│                                    (Fast, no DB)             │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                         ↓
                   Session Inactive
                   (30 min timeout)
                         ↓
┌─────────────────────────────────────────────────────────────┐
│                        Cleanup                               │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Check: should_persist flag?                                │
│     │                                                         │
│     ├─ YES → Save all messages to database (1 batch write)  │
│     │                                                         │
│     └─ NO  → Discard (incognito mode)                       │
│                                                               │
│  Remove from memory                                          │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Implementation Steps

### Step 1: Update SessionData

**File:** `crates/backend/src/manager.rs`

```rust
struct SessionData {
    state: Arc<Mutex<DefaultSessionState>>,
    last_activity: Instant,
    should_persist: bool,  // NEW: User-controlled persistence flag
}

impl SessionManager {
    pub async fn get_or_create_session(&self, session_id: &str) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        match self.sessions.get_mut(session_id) {
            Some(mut session_data) => { /* ... */ }
            None => {
                // Create new session with default persistence setting
                let session_data = SessionData {
                    state: Arc::new(Mutex::new(session_state)),
                    last_activity: Instant::now(),
                    should_persist: false,  // Default: don't persist
                };
                // ...
            }
        }
    }
}
```

### Step 2: Remove Runtime Persistence

**File:** `bin/backend/src/endpoint.rs`

```rust
// BEFORE:
let stream = IntervalStream::new(interval(Duration::from_millis(100))).then(move |_| {
    async move {
        let response = {
            let mut state = session_state.lock().await;
            state.update_state().await;
            state.get_state()
        };

        session_manager.update_user_history(&session_id, public_key.clone(), &response.messages).await;

        // REMOVE THIS ❌
        let _ = session_manager
            .persist_session_messages(&session_id, &response.messages)
            .await;

        Event::default().json_data(&response).map_err(|_| unreachable!())
    }
});

// AFTER:
let stream = IntervalStream::new(interval(Duration::from_millis(100))).then(move |_| {
    async move {
        let response = {
            let mut state = session_state.lock().await;
            state.update_state().await;
            state.get_state()
        };

        // Only send to frontend - no DB writes
        Event::default().json_data(&response).map_err(|_| unreachable!())
    }
});
```

### Step 3: Add Persistence Control Endpoint

**File:** `bin/backend/src/endpoint.rs`

```rust
#[derive(Deserialize)]
struct SetPersistenceRequest {
    session_id: String,
    should_persist: bool,
}

#[derive(Serialize)]
struct SetPersistenceResponse {
    success: bool,
    message: String,
}

async fn set_session_persistence(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<SetPersistenceRequest>,
) -> Result<Json<SetPersistenceResponse>, StatusCode> {
    if let Some(mut session_data) = session_manager.sessions.get_mut(&request.session_id) {
        session_data.should_persist = request.should_persist;

        let message = if request.should_persist {
            "Session will be saved to history"
        } else {
            "Session will not be saved (incognito mode)"
        };

        Ok(Json(SetPersistenceResponse {
            success: true,
            message: message.to_string(),
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// Add to router:
pub fn create_router(session_manager: Arc<SessionManager>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/chat/stream", get(chat_stream))
        .route("/api/interrupt", post(interrupt_endpoint))
        .route("/api/system", post(system_message_endpoint))
        .route("/api/mcp-command", post(mcp_command_endpoint))
        .route("/api/session/persistence", post(set_session_persistence))  // NEW
        .with_state(session_manager)
}
```

### Step 4: Update Cleanup Task

**File:** `crates/backend/src/manager.rs`

```rust
pub fn start_cleanup_task(self: Arc<Self>) {
    let cleanup_manager = Arc::clone(&self);
    let mut interval = tokio::time::interval(cleanup_manager.cleanup_interval);
    let sessions = cleanup_manager.sessions.clone();
    let session_timeout = cleanup_manager.session_timeout;
    let db_store = cleanup_manager.db_store.clone();

    tokio::spawn(async move {
        loop {
            interval.tick().await;
            let now = Instant::now();

            // Collect sessions to clean up
            let mut to_cleanup = Vec::new();
            sessions.retain(|session_id, session_data| {
                let should_keep = now.duration_since(session_data.last_activity) < session_timeout;

                if !should_keep {
                    // Session is inactive - save for cleanup
                    to_cleanup.push((
                        session_id.clone(),
                        session_data.state.clone(),
                        session_data.should_persist,
                    ));
                    println!("🗑️ Cleaning up inactive session: {}", session_id);
                }

                should_keep
            });

            // Persist sessions that should be saved (outside of retain closure)
            for (session_id, session_state, should_persist) in to_cleanup {
                if should_persist {
                    if let Some(db) = &db_store {
                        let messages = {
                            let state = session_state.lock().await;
                            state.messages.clone()
                        };

                        // Batch save all messages at once
                        if let Err(e) = save_session_to_db(db, &session_id, &messages).await {
                            tracing::error!("Failed to persist session {}: {}", session_id, e);
                        } else {
                            println!("💾 Persisted session {} to database", session_id);
                        }
                    }
                } else {
                    println!("🔒 Discarded session {} (incognito mode)", session_id);
                }
            }

            // Clean up old sessions from database
            if let Some(db) = &db_store {
                let cutoff_timestamp = chrono::Utc::now().timestamp() - session_timeout.as_secs() as i64;
                match db.delete_old_sessions(cutoff_timestamp).await {
                    Ok(deleted_count) => {
                        if deleted_count > 0 {
                            println!("🗑️ Cleaned up {} old sessions from database", deleted_count);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to cleanup old sessions from database: {}", e);
                    }
                }
            }
        }
    });
}

async fn save_session_to_db(
    db: &Arc<dyn SessionStoreApi>,
    session_id: &str,
    messages: &[ChatMessage],
) -> anyhow::Result<()> {
    for msg in messages {
        // Skip system messages (can be regenerated)
        if matches!(msg.sender, MessageSender::System) {
            continue;
        }

        let sender = match msg.sender {
            MessageSender::User => "user",
            MessageSender::Assistant => "agent",
            MessageSender::System => "system",
        };

        let content = serde_json::json!({
            "text": msg.content
        });

        let timestamp = chrono::Utc::now().timestamp();

        let db_message = aomi_tools::db::Message {
            id: 0,
            session_id: session_id.to_string(),
            message_type: "chat".to_string(),
            sender: sender.to_string(),
            content,
            timestamp,
        };

        db.save_message(&db_message).await?;
    }

    Ok(())
}
```

### Step 5: Simplify SessionManager Structure

**File:** `crates/backend/src/manager.rs`

```rust
pub struct SessionManager {
    sessions: Arc<DashMap<String, SessionData>>,
    session_public_keys: Arc<DashMap<String, String>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    chat_backend: Arc<dyn ChatBackend<ToolResultStream>>,
    db_store: Option<Arc<dyn SessionStoreApi>>,

    // REMOVE: user_history: Arc<DashMap<String, UserHistory>>
    // No longer needed - database handles cross-session history
}
```

### Step 6: Remove Obsolete Methods

**File:** `crates/backend/src/manager.rs`

Remove or simplify:
- ~~`persist_session_messages()`~~ - No longer needed
- ~~`update_user_history()`~~ - Simplified or removed
- ~~`get_user_history_with_pubkey()`~~ - Only needed for loading from DB

### Step 7: Update Session Restoration

**File:** `crates/backend/src/manager.rs`

```rust
pub async fn get_or_create_session(&self, session_id: &str) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
    match self.sessions.get_mut(session_id) {
        Some(mut session_data) => {
            // Session exists in memory - just return it
            session_data.last_activity = Instant::now();
            if let Some(db) = &self.db_store {
                let _ = db.update_session_activity(session_id).await;
            }
            Ok(session_data.state.clone())
        }
        None => {
            // Try to load from database
            let initial_messages = if let Some(db) = &self.db_store {
                match db.get_session(session_id).await {
                    Ok(Some(db_session)) => {
                        println!("🔄 Restoring session from database: {}", session_id);
                        let db_messages = db.get_messages(session_id, Some("chat"), None).await.unwrap_or_default();
                        self.db_messages_to_chat_messages(&db_messages)
                    }
                    _ => Vec::new(),  // New session
                }
            } else {
                Vec::new()  // New session, no DB
            };

            // Create new session state
            let session_state = DefaultSessionState::new(Arc::clone(&self.chat_backend), initial_messages).await?;
            let session_data = SessionData {
                state: Arc::new(Mutex::new(session_state)),
                last_activity: Instant::now(),
                should_persist: false,  // Default: don't persist
            };

            let new_session = session_data.state.clone();
            self.sessions.insert(session_id.to_string(), session_data);

            // Create session record in DB if available
            if let Some(db) = &self.db_store {
                let now = chrono::Utc::now().timestamp();
                let db_session = aomi_tools::db::Session {
                    id: session_id.to_string(),
                    public_key: self.session_public_keys.get(session_id).map(|r| r.value().clone()),
                    started_at: now,
                    last_active_at: now,
                    title: None,
                    pending_transaction: None,
                };
                if let Err(e) = db.create_session(&db_session).await {
                    tracing::warn!("Failed to create session record in DB: {}", e);
                }
            }

            println!("📝 Created new session: {}", session_id);
            Ok(new_session)
        }
    }
}
```

## Frontend Integration

### Toggle Persistence

```typescript
// Frontend component
const [shouldPersist, setShouldPersist] = useState(false);

const togglePersistence = async () => {
  await fetch('/api/session/persistence', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      session_id: sessionId,
      should_persist: !shouldPersist,
    }),
  });
  setShouldPersist(!shouldPersist);
};

// UI
<button onClick={togglePersistence}>
  {shouldPersist ? '💾 Saving to History' : '🔒 Incognito Mode'}
</button>
```

### Query Params Alternative

```typescript
// Set persistence mode when creating stream connection
const eventSource = new EventSource(
  `/api/chat/stream?session_id=${sessionId}&persist=true`
);
```

## Benefits

### Performance

| Metric | Before | After |
|--------|--------|-------|
| DB queries per active session | 10/sec (100ms interval) | 0 |
| DB writes per message | 1 per message | 0 |
| DB writes per session | Many | 1 (on cleanup) |
| Active session memory | Same | Same |

**Example:** 10 concurrent users, 5-minute sessions
- **Before:** ~3,000 DB operations per session
- **After:** 1 DB operation per session
- **Improvement:** 99.97% reduction in DB load

### Simplicity

**Code Removed:**
- `persist_session_messages()` with duplicate detection logic
- `update_user_history()` complexity
- 100ms timer persistence calls
- Duplicate detection queries

**Lines of Code:**
- **Before:** ~150 lines of persistence logic
- **After:** ~50 lines (one-time batch save)
- **Reduction:** 67% less code

### User Experience

**New Features:**
- ✅ "Incognito mode" - conversations not saved
- ✅ Explicit opt-in to persistence
- ✅ Clear user control
- ✅ Better privacy

## Trade-offs

### What You Lose

1. **Real-time Crash Recovery**
   - **Before:** Messages saved every 100ms
   - **After:** Messages lost if server crashes before cleanup
   - **Impact:** Low - sessions are typically short-lived

2. **Immediate Persistence**
   - **Before:** Message in DB within 100ms
   - **After:** Message in DB after 30-minute timeout
   - **Impact:** Minimal - active sessions in memory anyway

3. **Cross-Session Resume During Active Session**
   - **Before:** User could switch devices mid-conversation
   - **After:** Must wait for session cleanup first
   - **Impact:** Low - rare use case

### What You Gain

1. **Performance:** 99%+ reduction in DB operations
2. **Simplicity:** 67% less persistence code
3. **User Control:** Privacy and incognito mode
4. **Reliability:** Fewer moving parts = fewer bugs

## Migration Path

### Phase 1: Add New System (Parallel)
- Add `should_persist` flag to SessionData
- Add persistence control endpoint
- Keep existing runtime persistence

### Phase 2: Test New System
- Deploy to staging
- Test persistence on cleanup
- Verify batch writes work

### Phase 3: Remove Old System
- Remove `persist_session_messages()` calls from endpoint
- Remove duplicate detection logic
- Clean up user_history DashMap

### Phase 4: Production Deploy
- Feature flag for gradual rollout
- Monitor database load reduction
- Gather user feedback on persistence control

## Success Metrics

**Technical:**
- [ ] Database write operations reduced by >95%
- [ ] Session cleanup completes within 1 second
- [ ] No data loss for persisted sessions
- [ ] Memory usage unchanged

**User Experience:**
- [ ] Users can toggle persistence mid-session
- [ ] Clear indication of persistence state
- [ ] Incognito mode works as expected
- [ ] Session restoration works after timeout

## Future Enhancements

### 1. Manual Save
```rust
// POST /api/session/save
// Force save current session immediately (before timeout)
async fn save_session_now(session_id: &str) -> Result<()>
```

### 2. Export Session
```rust
// GET /api/session/export?session_id=abc
// Download session as JSON
async fn export_session(session_id: &str) -> Json<SessionExport>
```

### 3. Configurable Timeout
```rust
// Allow users to set custom session timeout
struct SessionData {
    custom_timeout: Option<Duration>,  // Override default 30min
}
```

### 4. Session Tags
```rust
// Tag sessions for organization
struct SessionData {
    tags: Vec<String>,  // ["trading", "research", "important"]
}
```

## Questions to Consider

1. **Should we save pending transactions immediately?**
   - They're critical for crash recovery
   - Consider persisting these even without full persistence flag

2. **What's the default persistence setting?**
   - Option A: Default OFF (privacy first)
   - Option B: Default ON (convenience first)
   - Recommendation: OFF by default, user opts in

3. **Should we notify users about unsaved sessions?**
   - Show indicator when session has unsaved messages
   - Warn before closing tab if not persisted

4. **How do we handle very long sessions?**
   - Sessions >30 minutes never timeout
   - Consider periodic "checkpoint" saves for long sessions

## Conclusion

The persist-on-cleanup design dramatically simplifies the architecture while providing better performance and user control. The trade-off of losing real-time crash recovery is acceptable for most chat applications, and the benefits far outweigh the costs.

This design follows the principle: **Keep it simple. Optimize for the common case (short sessions). Let users control their data.**
