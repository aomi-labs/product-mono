# Session Implementation Plan

## Overview
Transform the single-user backend and frontend to support multiple concurrent users through session-based state management.

## Backend Implementation

### 1. Core Structs to Modify/Add

#### A. Add SessionManager (NEW)
```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
}

struct SessionData {
    state: Arc<Mutex<WebChatState>>,
    last_activity: Instant,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(1800), // 30 minutes
        }
    }

    pub async fn get_or_create_session(&self, session_id: &str, skip_docs: bool) -> Result<Arc<Mutex<WebChatState>>, anyhow::Error> {
        let mut sessions = self.sessions.write().await;

        if let Some(session_data) = sessions.get_mut(session_id) {
            // Update last activity and return existing session
            session_data.last_activity = Instant::now();
            Ok(session_data.state.clone())
        } else {
            // Create new session
            let web_chat_state = WebChatState::new(skip_docs).await?;
            let session_data = SessionData {
                state: Arc::new(Mutex::new(web_chat_state)),
                last_activity: Instant::now(),
            };
            let state_clone = session_data.state.clone();
            sessions.insert(session_id.to_string(), session_data);
            Ok(state_clone)
        }
    }

    pub async fn cleanup_inactive_sessions(&self) {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();

        sessions.retain(|session_id, session_data| {
            let should_keep = now.duration_since(session_data.last_activity) < self.session_timeout;
            if !should_keep {
                println!("🗑️ Cleaning up inactive session: {}", session_id);
            }
            should_keep
        });
    }

    pub async fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
    }

    pub async fn start_cleanup_task(self: Arc<Self>) {
        let cleanup_manager = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_manager.cleanup_interval);
            loop {
                interval.tick().await;
                cleanup_manager.cleanup_inactive_sessions().await;
            }
        });
    }
}
```

#### B. Modify Request Structs (EXISTING)

**Location**: `chatbot/bin/backend/src/main.rs:288-302`

```rust
// BEFORE
#[derive(Deserialize)]
struct ChatRequest {
    message: String,
}

#[derive(Deserialize)]
struct SystemMessageRequest {
    message: String,
}

#[derive(Deserialize)]
struct McpCommandRequest {
    command: String,
    args: serde_json::Value,
}

// AFTER
#[derive(Deserialize)]
struct ChatRequest {
    message: String,
    session_id: Option<String>, // Optional for backward compatibility
}

#[derive(Deserialize)]
struct SystemMessageRequest {
    message: String,
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct McpCommandRequest {
    command: String,
    args: serde_json::Value,
    session_id: Option<String>,
}
```

#### C. Replace SharedChatState Type Alias (EXISTING)

**Location**: `chatbot/bin/backend/src/main.rs:321`

```rust
// BEFORE
type SharedChatState = Arc<Mutex<WebChatState>>;

// AFTER
type SharedSessionManager = Arc<SessionManager>;
```

### 2. Endpoint Functions to Modify

#### A. chat_endpoint (EXISTING)
**Location**: `chatbot/bin/backend/src/main.rs:337-348`

```rust
// BEFORE
async fn chat_endpoint(
    State(chat_state): State<SharedChatState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let mut state = chat_state.lock().await;

    if let Err(_) = state.send_message(request.message).await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

// AFTER
async fn chat_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    if let Err(_) = state.send_message(request.message).await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}
```

#### B. state_endpoint (EXISTING)
**Location**: `chatbot/bin/backend/src/main.rs:350-356`

```rust
// BEFORE
async fn state_endpoint(
    State(chat_state): State<SharedChatState>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let mut state = chat_state.lock().await;
    state.update_state().await;
    Ok(Json(state.get_state()))
}

// AFTER
async fn state_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = params.get("session_id").cloned().unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    state.update_state().await;
    Ok(Json(state.get_state()))
}
```

#### C. chat_stream (EXISTING) - Most Complex Change
**Location**: `chatbot/bin/backend/src/main.rs:358-378`

```rust
// BEFORE
async fn chat_stream(
    State(chat_state): State<SharedChatState>,
) -> Sse<impl StreamExt<Item = Result<axum::response::sse::Event, Infallible>>> {
    let stream = IntervalStream::new(interval(Duration::from_millis(100)))
        .map(move |_| {
            let chat_state = Arc::clone(&chat_state);
            async move {
                let mut state = chat_state.lock().await;
                state.update_state().await;
                let response = state.get_state();

                axum::response::sse::Event::default()
                    .json_data(&response)
                    .map_err(|_| ())
            }
        })
        .then(|f| f)
        .map(|result| result.map_err(|_| unreachable!()));

    Sse::new(stream)
}

// AFTER
async fn chat_stream(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Sse<impl StreamExt<Item = Result<axum::response::sse::Event, Infallible>>> {
    let session_id = params.get("session_id").cloned().unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => {
            // Return error event
            let stream = futures::stream::once(async {
                axum::response::sse::Event::default()
                    .event("error")
                    .data("Failed to create session")
                    .map_err(|_| unreachable!())
            });
            return Sse::new(stream);
        }
    };

    let stream = IntervalStream::new(interval(Duration::from_millis(100)))
        .map(move |_| {
            let session_state = Arc::clone(&session_state);
            async move {
                let mut state = session_state.lock().await;
                state.update_state().await;
                let response = state.get_state();

                axum::response::sse::Event::default()
                    .json_data(&response)
                    .map_err(|_| ())
            }
        })
        .then(|f| f)
        .map(|result| result.map_err(|_| unreachable!()));

    Sse::new(stream)
}
```

#### D. interrupt_endpoint (EXISTING)
**Location**: `chatbot/bin/backend/src/main.rs:380-389`

```rust
// BEFORE
async fn interrupt_endpoint(
    State(chat_state): State<SharedChatState>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let mut state = chat_state.lock().await;
    if let Err(_) = state.interrupt_processing().await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

// AFTER
async fn interrupt_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<InterruptRequest>, // New request struct needed
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    if let Err(_) = state.interrupt_processing().await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

// NEW struct needed
#[derive(Deserialize)]
struct InterruptRequest {
    session_id: Option<String>,
}
```

#### E. system_message_endpoint & mcp_command_endpoint (EXISTING)
Similar pattern - add session manager parameter and session resolution.

### 3. Main Function Changes (EXISTING)
**Location**: `chatbot/bin/backend/src/main.rs:455-493`

```rust
// BEFORE
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize chat state
    let chat_state = Arc::new(Mutex::new(
        WebChatState::new(cli.no_docs).await?
    ));

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/chat/stream", get(chat_stream))
        .route("/api/interrupt", post(interrupt_endpoint))
        .route("/api/system", post(system_message_endpoint))
        .route("/api/mcp-command", post(mcp_command_endpoint))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        )
        .with_state(chat_state);
    // ... rest
}

// AFTER
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize session manager
    let session_manager = Arc::new(SessionManager::new());

    // Start cleanup task
    let cleanup_manager = Arc::clone(&session_manager);
    cleanup_manager.start_cleanup_task().await;

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/chat/stream", get(chat_stream))
        .route("/api/interrupt", post(interrupt_endpoint))
        .route("/api/system", post(system_message_endpoint))
        .route("/api/mcp-command", post(mcp_command_endpoint))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        )
        .with_state(session_manager);
    // ... rest
}

// NEW helper function
fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
```

### 4. Additional Dependencies
**Location**: `chatbot/bin/backend/Cargo.toml`

Add these dependencies:
```toml
uuid = { version = "1.0", features = ["v4", "serde"] }
```

## Frontend Implementation

### 1. ChatManager Class Modifications

#### A. Add Session Management (ChatManager.ts:16-40)

```typescript
// BEFORE
export class ChatManager {
  private config: ChatManagerConfig;
  private onMessage: (messages: Message[]) => void;
  // ... other private fields

  constructor(config: Partial<ChatManagerConfig> = {}, eventHandlers: Partial<ChatManagerEventHandlers> = {}) {
    this.config = { ... };
    // ... existing setup
  }

// AFTER
export class ChatManager {
  private config: ChatManagerConfig;
  private sessionId: string; // NEW
  private onMessage: (messages: Message[]) => void;
  // ... other private fields

  constructor(config: Partial<ChatManagerConfig> = {}, eventHandlers: Partial<ChatManagerEventHandlers> = {}) {
    this.config = { ... };
    this.sessionId = this.generateSessionId(); // NEW
    // ... existing setup
  }

  // NEW method
  private generateSessionId(): string {
    return crypto.randomUUID ? crypto.randomUUID() :
           'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
             const r = Math.random() * 16 | 0;
             const v = c == 'x' ? r : (r & 0x3 | 0x8);
             return v.toString(16);
           });
  }

  // NEW method to get session ID
  public getSessionId(): string {
    return this.sessionId;
  }

  // NEW method to set session ID (for reconnection scenarios)
  public setSessionId(sessionId: string): void {
    this.sessionId = sessionId;
  }
```

#### B. Update SSE Connection (ChatManager.ts:42-77)

```typescript
// BEFORE
connect(): void {
    this.setConnectionStatus(ConnectionStatus.CONNECTING);
    this.disconnect();

    try {
      this.eventSource = new EventSource(`${this.config.backendUrl}/api/chat/stream`);
      // ... rest of connection logic
    }
}

// AFTER
connect(): void {
    this.setConnectionStatus(ConnectionStatus.CONNECTING);
    this.disconnect();

    try {
      this.eventSource = new EventSource(`${this.config.backendUrl}/api/chat/stream?session_id=${this.sessionId}`);
      // ... rest of connection logic
    }
}
```

#### C. Update API Calls (ChatManager.ts:87-118)

```typescript
// BEFORE
async sendMessage(message: string): Promise<void> {
    // ... validation logic

    try {
      const response = await fetch(`${this.config.backendUrl}/api/chat`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ message }),
      });
      // ... rest
    }
}

// AFTER
async sendMessage(message: string): Promise<void> {
    // ... validation logic

    try {
      const response = await fetch(`${this.config.backendUrl}/api/chat`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          message,
          session_id: this.sessionId
        }),
      });
      // ... rest
    }
}
```

#### D. Update interrupt() (ChatManager.ts:120-137)

```typescript
// BEFORE
async interrupt(): Promise<void> {
    try {
      const response = await fetch(`${this.config.backendUrl}/api/interrupt`, {
        method: 'POST',
      });
      // ... rest
    }
}

// AFTER
async interrupt(): Promise<void> {
    try {
      const response = await fetch(`${this.config.backendUrl}/api/interrupt`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          session_id: this.sessionId
        }),
      });
      // ... rest
    }
}
```

#### E. Update sendNetworkSwitchRequest() & sendTransactionResult() (ChatManager.ts:139-199)

```typescript
// Add session_id to both method's request bodies
body: JSON.stringify({
  message: systemMessage,
  session_id: this.sessionId
}),
```

### 2. Type Updates

#### A. Add to ChatManagerConfig (types.ts:16-21)
```typescript
export interface ChatManagerConfig {
  backendUrl: string;
  maxMessageLength: number;
  reconnectAttempts: number;
  reconnectDelay: number;
  sessionId?: string; // NEW - optional for external session management
}
```

## Implementation Order

### Phase 1: Backend Core (Low Risk)
1. Add SessionManager struct and helper function
2. Add session_id fields to request structs
3. Update main() to use SessionManager
4. Add uuid dependency

### Phase 2: Backend Endpoints (Medium Risk)
1. Update chat_endpoint
2. Update state_endpoint
3. Update interrupt_endpoint (add InterruptRequest struct)
4. Update system_message_endpoint
5. Update mcp_command_endpoint

### Phase 3: Backend SSE (High Risk)
1. Update chat_stream endpoint (most complex change)

### Phase 4: Frontend Core (Low Risk)
1. Add session ID generation to ChatManager
2. Update constructor to initialize sessionId

### Phase 5: Frontend API Integration (Medium Risk)
1. Update sendMessage() API call
2. Update interrupt() API call
3. Update sendNetworkSwitchRequest()
4. Update sendTransactionResult()

### Phase 6: Frontend SSE (High Risk)
1. Update connect() method with session parameter

## Testing Strategy

### Backend Testing
```bash
# Test session isolation
curl -X POST http://localhost:8080/api/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"Hello from session 1","session_id":"session-1"}'

curl -X POST http://localhost:8080/api/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"Hello from session 2","session_id":"session-2"}'

# Verify SSE streams are isolated
curl "http://localhost:8080/api/chat/stream?session_id=session-1" &
curl "http://localhost:8080/api/chat/stream?session_id=session-2" &
```

### Frontend Testing
```javascript
// Create two ChatManager instances with different sessions
const manager1 = new ChatManager();
const manager2 = new ChatManager();

// Verify they have different session IDs
console.log(manager1.getSessionId(), manager2.getSessionId());

// Test concurrent usage
manager1.connect();
manager2.connect();
manager1.sendMessage("Hello from manager 1");
manager2.sendMessage("Hello from manager 2");
```

## Risk Mitigation

### Backward Compatibility
- All session_id fields are optional
- Default session generation for missing session_id
- Existing clients continue to work (single session)

### Memory Management
- Session cleanup every 5 minutes
- 30-minute session timeout
- Proper cleanup in SessionManager drop

### Error Handling
- Graceful fallback for session creation errors
- SSE error events for session failures
- Logging for debugging session issues

## File Summary

### Files to Modify
1. `chatbot/bin/backend/src/main.rs` - Major changes (~150 lines modified)
2. `chatbot/bin/backend/Cargo.toml` - Add uuid dependency
3. `frontend/src/lib/chat-manager.ts` - Medium changes (~20 lines modified)
4. `frontend/src/lib/types.ts` - Minor changes (1 optional field)

### Files Not Modified
- WebChatState implementation remains unchanged
- Agent setup remains unchanged
- Frontend UI components remain unchanged
- Frontend types mostly unchanged