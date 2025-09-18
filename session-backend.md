# Session-Based Backend Architecture

## Overview
Transform the current single-user backend to support multiple concurrent users through session-based state management. Each user gets their own isolated chat state and agent process.

## Current Problems ❌
- **Single Global State**: One `WebChatState` shared across all users
- **Shared Agent Process**: One `setup_agent_and_handle_messages` task for all users
- **Message Contamination**: User A sees User B's messages
- **State Conflicts**: Processing/loading flags interfere between users
- **Transaction Conflicts**: Wallet transactions get mixed up

## Proposed Solution ✅

### 1. SessionManager Structure
```rust
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<Mutex<WebChatState>>>>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
}

impl SessionManager {
    pub async fn get_or_create_session(&self, session_id: &str) -> Arc<Mutex<WebChatState>>
    pub async fn cleanup_inactive_sessions(&self)
    pub async fn remove_session(&self, session_id: &str)
}
```

### 2. Session Lifecycle
1. **Creation**: First API call with session_id creates new WebChatState + agent task
2. **Isolation**: Each session has independent:
   - Message history
   - Agent process (`setup_agent_and_handle_messages`)
   - Channel pairs (`sender_to_llm`, `receiver_from_llm`)
   - State flags (`is_processing`, `pending_wallet_tx`)
3. **Cleanup**: Inactive sessions auto-removed after timeout

### 3. API Changes
```rust
// Before
async fn chat_endpoint(
    State(chat_state): State<SharedChatState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<WebStateResponse>, StatusCode>

// After
async fn chat_endpoint(
    State(session_manager): State<Arc<SessionManager>>,
    Json(request): Json<ChatRequest>, // now has session_id
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(|| generate_session_id());
    let session_state = session_manager.get_or_create_session(&session_id).await;
    let mut state = session_state.lock().await;
    state.send_message(request.message).await?;
    Ok(Json(state.get_state()))
}
```

### 4. SSE Stream Per Session
```rust
async fn chat_stream(
    State(session_manager): State<Arc<SessionManager>>,
    Query(params): Query<HashMap<String, String>>, // session_id in query param
) -> Sse<impl StreamExt<Item = Result<axum::response::sse::Event, Infallible>>> {
    let session_id = params.get("session_id").cloned().unwrap_or_else(|| generate_session_id());
    let session_state = session_manager.get_or_create_session(&session_id).await;

    let stream = IntervalStream::new(interval(Duration::from_millis(100)))
        .map(move |_| {
            let session_state = Arc::clone(&session_state);
            async move {
                let mut state = session_state.lock().await;
                state.update_state().await;
                let response = state.get_state();
                axum::response::sse::Event::default().json_data(&response).map_err(|_| ())
            }
        });
    Sse::new(stream)
}
```

## Implementation Steps

### Phase 1: Core Session Support
1. ✅ Add `session_id` to all request structs
2. Create `SessionManager` struct
3. Replace `type SharedChatState = Arc<Mutex<WebChatState>>` with `Arc<SessionManager>`
4. Update main() to initialize SessionManager instead of single WebChatState

### Phase 2: Endpoint Updates
1. Update all endpoints to extract session_id and get session state
2. Update SSE endpoint for per-session streams
3. Add session cleanup background task

### Phase 3: Frontend Integration
1. Generate session IDs in frontend ChatManager
2. Include session_id in all API calls
3. Update SSE connection with session parameter

### Phase 4: Testing & Optimization
1. Test concurrent users don't interfere
2. Add session timeout configuration
3. Add session metrics/monitoring

## File Changes Required

### Backend (`chatbot/bin/backend/src/main.rs`)
- Add SessionManager struct (~50 lines)
- Update main() function (~10 lines)
- Update all 6 endpoint functions (~30 lines)
- Add cleanup task (~20 lines)

### Frontend (`frontend/src/lib/chat-manager.ts`)
- Add session ID generation (~5 lines)
- Update all API calls to include session_id (~15 lines)
- Update SSE connection (~5 lines)

## Benefits
- ✅ **True Multi-User**: Isolated state per user
- ✅ **Scalable**: Cleanup prevents memory leaks
- ✅ **Backward Compatible**: Default session_id for existing clients
- ✅ **Simple**: Minimal changes to existing logic
- ✅ **Secure**: No cross-user data leakage

## Considerations
- **Memory Usage**: Each session = 1 agent process + channels + message history
- **Resource Limits**: Consider max concurrent sessions
- **Session Storage**: Currently in-memory, could extend to Redis/DB later
- **Agent Startup Time**: New sessions have brief initialization delay