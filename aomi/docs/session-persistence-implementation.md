# Session Persistence Implementation Guide

## Overview

This document outlines the implementation plan for persisting user sessions, message history, and agent conversation context to PostgreSQL using SQLx.

### Design Philosophy: Simplicity First

This implementation follows a **maximally simplified approach**:

1. **No metadata columns** - Metadata has been eliminated from both sessions and messages tables
2. **JSONB for content** - All message content stored as JSONB, handling both simple chat and complex agent history
3. **Regenerated display data** - Timestamps and formatting regenerated on load rather than persisted
4. **Ephemeral state stays ephemeral** - Runtime state like `is_streaming`, `is_processing`, and `tool_stream` not persisted
5. **Single pending transaction** - Pending wallet transaction stored as JSONB column on sessions (1:1 relationship)

### What's Persisted vs. What's Ephemeral

**Persisted to Database:**
- ✅ Users (public_key, username, created_at)
- ✅ Sessions (id, public_key, timestamps, title, pending_transaction)
- ✅ Messages (chat and agent_history with JSONB content)

**Kept in Memory Only (Runtime State):**
- ❌ `is_processing`, `is_streaming` - UI state
- ❌ `tool_stream` - Ephemeral streaming state
- ❌ `has_sent_welcome` - Derivable from message count
- ❌ Channels, active tool streams, interrupt senders - Runtime communication

**Regenerated on Load:**
- 🔄 Formatted timestamps (from Unix timestamp)
- 🔄 `has_sent_welcome` (check if messages exist)

---

## Database Schema

### Complete Schema Definition

```sql
-- Users table (public_key as primary identifier)
CREATE TABLE users (
    public_key TEXT PRIMARY KEY,
    username TEXT UNIQUE,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

-- Sessions table with pending transaction support
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,  -- UUID v4 string format
    public_key TEXT REFERENCES users(public_key) ON DELETE SET NULL,
    started_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    last_active_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    title TEXT,
    pending_transaction JSONB  -- Single pending wallet transaction
);

-- Unified messages table (both chat and agent history)
CREATE TABLE messages (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    message_type TEXT NOT NULL DEFAULT 'chat',  -- 'chat' or 'agent_history'
    sender TEXT NOT NULL,  -- 'user', 'agent', 'system' for chat; 'user', 'assistant' for agent_history
    content JSONB NOT NULL,  -- JSONB handles both simple text and complex agent message structures
    timestamp BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

-- Indexes for performance
CREATE INDEX idx_sessions_public_key ON sessions(public_key);
CREATE INDEX idx_sessions_last_active ON sessions(last_active_at DESC);
CREATE INDEX idx_messages_session_type ON messages(session_id, message_type, timestamp ASC);
```

### JSONB Content Structures

**Pending Transaction (sessions.pending_transaction):**
```json
{
  "created_at": 1699564800,
  "expires_at": 1699568400,
  "chain_id": 1,
  "transaction": {
    "from": "0x123...",
    "to": "0x456...",
    "value": "0x16345785d8a0000",
    "data": "0x",
    "gas": "21000"
  },
  "user_intent": "Send 0.1 ETH to alice.eth",
  "signature": null
}
```

**Chat Message Content (messages.content where message_type='chat'):**
```json
{
  "text": "Hello, how can I help you today?"
}
```

**Agent History Content (messages.content where message_type='agent_history'):**
```json
{
  "type": "api_message",
  "content": [
    {"type": "text", "text": "I'll help you with that..."},
    {"type": "tool_use", "id": "123", "name": "read_file", "input": {...}}
  ]
}
```

---

## Rust Data Structures

### Database Models

```rust
// crates/backend/src/models/mod.rs

use sqlx::FromRow;
use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub public_key: String,
    pub username: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct Session {
    pub id: String,
    pub public_key: Option<String>,
    pub started_at: i64,
    pub last_active_at: i64,
    pub title: Option<String>,
    pub pending_transaction: Option<serde_json::Value>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Message {
    pub id: i64,
    pub session_id: String,
    pub message_type: String,  // "chat" or "agent_history"
    pub sender: String,
    pub content: serde_json::Value,  // JSONB for all content
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    #[serde(rename = "chat")]
    Chat,
    #[serde(rename = "agent_history")]
    AgentHistory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransaction {
    pub created_at: i64,
    pub expires_at: i64,
    pub chain_id: u32,
    pub transaction: serde_json::Value,  // Full transaction object
    pub user_intent: String,
    pub signature: Option<String>,
}

impl Session {
    /// Get parsed pending transaction
    pub fn get_pending_transaction(&self) -> Result<Option<PendingTransaction>> {
        match &self.pending_transaction {
            Some(json) => Ok(Some(serde_json::from_value(json.clone())?)),
            None => Ok(None),
        }
    }

    /// Set pending transaction
    pub fn set_pending_transaction(&mut self, tx: Option<PendingTransaction>) -> Result<()> {
        self.pending_transaction = match tx {
            Some(t) => Some(serde_json::to_value(t)?),
            None => None,
        };
        Ok(())
    }
}
```

### Conversion Helpers

```rust
// crates/backend/src/models/conversions.rs

use super::*;
use crate::session::{ChatMessage, MessageSender};
use anyhow::Result;

impl ChatMessage {
    /// Convert ChatMessage to database Message
    pub fn to_db_message(&self, session_id: &str) -> Result<Message> {
        let timestamp = chrono::Utc::now().timestamp();

        // Store content as simple JSON object with text field
        let content = serde_json::json!({
            "text": self.content
        });

        Ok(Message {
            id: 0, // Will be assigned by database
            session_id: session_id.to_string(),
            message_type: "chat".to_string(),
            sender: match self.sender {
                MessageSender::User => "user".to_string(),
                MessageSender::Assistant => "agent".to_string(),
                MessageSender::System => "system".to_string(),
            },
            content,
            timestamp,
        })
    }

    /// Convert database Message to ChatMessage
    pub fn from_db_message(msg: Message) -> Result<Self> {
        let sender = match msg.sender.as_str() {
            "user" => MessageSender::User,
            "agent" => MessageSender::Assistant,
            "system" => MessageSender::System,
            _ => MessageSender::System,
        };

        // Extract text content from JSONB
        let content = msg.content
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Regenerate formatted timestamp from Unix timestamp
        let timestamp = chrono::DateTime::from_timestamp(msg.timestamp, 0)
            .map(|dt| dt.format("%H:%M:%S %Z").to_string())
            .unwrap_or_default();

        Ok(ChatMessage {
            sender,
            content,
            tool_stream: None,  // Ephemeral, not restored from DB
            timestamp,
            is_streaming: false, // Always false when loaded from DB
        })
    }
}

// Agent history conversion helpers
impl Message {
    /// Store agent history message (Anthropic API format)
    pub fn from_agent_message(
        session_id: &str,
        role: &str,
        content: serde_json::Value,  // Full API message content
    ) -> Result<Self> {
        Ok(Message {
            id: 0,
            session_id: session_id.to_string(),
            message_type: "agent_history".to_string(),
            sender: role.to_string(),
            content,  // Store full content structure as JSONB
            timestamp: chrono::Utc::now().timestamp(),
        })
    }
}
```

---

## Implementation Steps

### Phase 1: Database Setup

#### Step 1.1: Create Migration File
- [ ] Create migration file at `crates/backend/migrations/001_initial_schema.sql`
- [ ] Add schema from above
- [ ] Test migration on local PostgreSQL instance

#### Step 1.2: Update Database Configuration
- [ ] Ensure `DATABASE_URL` environment variable is set
- [ ] Update connection pool configuration in main.rs if needed
- [ ] Verify SQLx can connect to database

```rust
// Example: Update in main.rs or db setup
let pool = sqlx::any::AnyPoolOptions::new()
    .max_connections(10)  // Increase from 5 for session persistence
    .connect(&database_url)
    .await?;
```

---

### Phase 2: Data Layer Implementation

#### Step 2.1: Create Models Module
- [ ] Create `crates/backend/src/models/mod.rs`
- [ ] Add User, Session, Message structs
- [ ] Add metadata structs (SessionMetadata, ChatMessageMetadata, etc.)
- [ ] Create `conversions.rs` with conversion helpers

#### Step 2.2: Create Database Store Trait
- [ ] Create `crates/backend/src/db/session_store.rs`
- [ ] Define `SessionStoreApi` trait

```rust
#[async_trait]
pub trait SessionStoreApi: Send + Sync {
    // User operations
    async fn get_or_create_user(&self, public_key: &str) -> Result<User>;
    async fn update_user(&self, user: &User) -> Result<()>;

    // Session operations
    async fn create_session(&self, session: &Session) -> Result<()>;
    async fn get_session(&self, session_id: &str) -> Result<Option<Session>>;
    async fn update_session_activity(&self, session_id: &str) -> Result<()>;
    async fn get_user_sessions(&self, public_key: &str, limit: i32) -> Result<Vec<Session>>;

    // Pending transaction operations
    async fn update_pending_transaction(
        &self,
        session_id: &str,
        tx: Option<PendingTransaction>
    ) -> Result<()>;

    // Message operations
    async fn save_chat_message(&self, session_id: &str, message: &ChatMessage) -> Result<i64>;
    async fn get_chat_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>>;
    async fn save_agent_message(
        &self,
        session_id: &str,
        role: &str,
        content: serde_json::Value
    ) -> Result<i64>;
    async fn get_agent_history(&self, session_id: &str) -> Result<Vec<Message>>;

    // Cleanup operations
    async fn delete_old_sessions(&self, inactive_since: i64) -> Result<u64>;
}
```

#### Step 2.3: Implement SessionStore
- [ ] Create concrete implementation using SQLx
- [ ] Add connection pool to struct
- [ ] Implement all trait methods with proper error handling

```rust
pub struct SessionStore {
    pool: AnyPool,
}

impl SessionStore {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionStoreApi for SessionStore {
    // Implement all methods...
}
```

---

### Phase 3: Session Manager Integration

#### Step 3.1: Update SessionManager Structure
- [ ] Add `db_store: Arc<dyn SessionStoreApi>` to SessionManager
- [ ] Keep existing `DashMap` for runtime state
- [ ] Update constructor to accept database store

```rust
pub struct SessionManager<S> {
    // Runtime state (ephemeral)
    sessions: Arc<DashMap<String, SessionRuntimeState<S>>>,
    session_public_keys: Arc<DashMap<String, String>>,

    // Database persistence
    db_store: Arc<dyn SessionStoreApi>,

    // Configuration
    session_timeout: Duration,
}
```

#### Step 3.2: Implement Session Lifecycle Methods
- [ ] Update `get_or_create_session()` to load from DB
- [ ] Add session restoration logic with pending transaction support
- [ ] Implement lazy loading of messages

```rust
impl<S> SessionManager<S> {
    pub async fn get_or_create_session(&self, session_id: &str) -> Result<&SessionRuntimeState<S>> {
        // Check runtime state
        if let Some(state) = self.sessions.get(session_id) {
            return Ok(state);
        }

        // Load or create in database
        let session = match self.db_store.get_session(session_id).await? {
            Some(s) => s,
            None => {
                let new_session = Session {
                    id: session_id.to_string(),
                    public_key: None,
                    started_at: chrono::Utc::now().timestamp(),
                    last_active_at: chrono::Utc::now().timestamp(),
                    title: None,
                    pending_transaction: None,
                };
                self.db_store.create_session(&new_session).await?;
                new_session
            }
        };

        // Create runtime state
        let runtime_state = SessionRuntimeState::new(session_id);
        self.sessions.insert(session_id.to_string(), runtime_state);

        // Restore pending transaction if exists
        self.restore_pending_transaction(session_id, &session).await?;

        Ok(self.sessions.get(session_id).unwrap())
    }

    async fn restore_pending_transaction(&self, session_id: &str, session: &Session) -> Result<()> {
        if let Some(pending_tx) = session.get_pending_transaction()? {
            let now = chrono::Utc::now().timestamp();

            // Check if expired
            if now > pending_tx.expires_at {
                // Clear expired transaction
                self.db_store.update_pending_transaction(session_id, None).await?;
            } else {
                // Restore to runtime state
                if let Some(mut runtime_session) = self.sessions.get_mut(session_id) {
                    runtime_session.pending_wallet_tx = Some(pending_tx.user_intent.clone());
                }

                // Notify user about pending transaction
                self.add_message(
                    session_id,
                    ChatMessage::system(&format!(
                        "You have a pending transaction: {}. Ready to sign?",
                        pending_tx.user_intent
                    ))
                ).await?;
            }
        }

        Ok(())
    }
}
```

#### Step 3.3: Add Message Persistence Hooks
- [ ] Persist ChatMessage after adding to session
- [ ] Persist agent messages after LLM responses
- [ ] Update session activity timestamp

```rust
impl<S> SessionManager<S> {
    pub async fn add_message(&self, session_id: &str, message: ChatMessage) -> Result<()> {
        // Save to database
        self.db_store.save_chat_message(session_id, &message).await?;

        // Update runtime state
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.messages.push(message);
        }

        // Update activity timestamp
        self.db_store.update_session_activity(session_id).await?;

        Ok(())
    }

    pub async fn persist_agent_history(&self, session_id: &str, history: &[anthropic::Message]) -> Result<()> {
        for msg in history {
            // Convert Anthropic message content to JSON
            let content = serde_json::to_value(&msg.content)?;

            self.db_store.save_agent_message(
                session_id,
                &msg.role,
                content,
            ).await?;
        }
        Ok(())
    }

    // Transaction management methods
    pub async fn set_pending_transaction(
        &self,
        session_id: &str,
        chain_id: u32,
        tx_data: serde_json::Value,
        user_intent: String,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        let pending_tx = PendingTransaction {
            created_at: now,
            expires_at: now + 3600,  // 1 hour expiry
            chain_id,
            transaction: tx_data,
            user_intent: user_intent.clone(),
            signature: None,
        };

        // Save to database
        self.db_store.update_pending_transaction(session_id, Some(pending_tx)).await?;

        // Update runtime state
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.pending_wallet_tx = Some(user_intent);
        }

        Ok(())
    }

    pub async fn clear_pending_transaction(&self, session_id: &str) -> Result<()> {
        self.db_store.update_pending_transaction(session_id, None).await?;

        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.pending_wallet_tx = None;
        }

        Ok(())
    }
}
```

#### Step 3.4: Implement History Loading
- [ ] Load chat messages when session is restored
- [ ] Load agent history for LLM context
- [ ] Handle pagination for large histories

```rust
impl<S> SessionManager<S> {
    pub async fn load_session_history(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        self.db_store.get_chat_messages(session_id).await
    }

    pub async fn load_agent_history(&self, session_id: &str) -> Result<Vec<Message>> {
        self.db_store.get_agent_history(session_id).await
    }

    pub async fn restore_session_state(&self, session_id: &str) -> Result<()> {
        // Load chat messages
        let chat_messages = self.load_session_history(session_id).await?;

        // Load agent history
        let agent_messages = self.load_agent_history(session_id).await?;

        // Update runtime state
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.messages = chat_messages;
            // Reconstruct agent_history from agent_messages
            // This depends on your LLM client structure
        }

        Ok(())
    }
}
```

#### Step 3.5: Implement SessionStore Methods
- [ ] Implement `update_pending_transaction` method
- [ ] Implement message save/load methods with JSONB

```rust
#[async_trait]
impl SessionStoreApi for SessionStore {
    async fn update_pending_transaction(
        &self,
        session_id: &str,
        tx: Option<PendingTransaction>,
    ) -> Result<()> {
        let json_value = match tx {
            Some(t) => Some(serde_json::to_value(t)?),
            None => None,
        };

        sqlx::query(
            "UPDATE sessions
             SET pending_transaction = $1,
                 last_active_at = EXTRACT(EPOCH FROM NOW())::BIGINT
             WHERE id = $2"
        )
        .bind(json_value)
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn save_chat_message(&self, session_id: &str, message: &ChatMessage) -> Result<i64> {
        let db_message = message.to_db_message(session_id)?;

        let row: (i64,) = sqlx::query_as(
            "INSERT INTO messages (session_id, message_type, sender, content, timestamp)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id"
        )
        .bind(&db_message.session_id)
        .bind(&db_message.message_type)
        .bind(&db_message.sender)
        .bind(&db_message.content)
        .bind(db_message.timestamp)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    async fn get_chat_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        let messages: Vec<Message> = sqlx::query_as(
            "SELECT * FROM messages
             WHERE session_id = $1 AND message_type = 'chat'
             ORDER BY timestamp ASC"
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        messages.into_iter()
            .map(ChatMessage::from_db_message)
            .collect()
    }
}
```

---

### Phase 4: User History Integration

#### Step 4.1: Update Public Key Association
- [ ] Modify `set_session_public_key()` to persist to DB
- [ ] Create user record if doesn't exist
- [ ] Associate session with user

```rust
impl<S> SessionManager<S> {
    pub async fn set_session_public_key(&self, session_id: &str, public_key: String) -> Result<()> {
        // Ensure user exists
        self.db_store.get_or_create_user(&public_key).await?;

        // Update session
        if let Some(mut session) = self.db_store.get_session(session_id).await? {
            session.public_key = Some(public_key.clone());
            self.db_store.update_session(&session).await?;
        }

        // Update in-memory mapping
        self.session_public_keys.insert(session_id.to_string(), public_key.clone());

        // Sync history (existing logic)
        self.sync_user_history_to_session(session_id, &public_key).await?;

        Ok(())
    }
}
```

#### Step 4.2: Replace In-Memory UserHistory
- [ ] Remove `user_history: Arc<DashMap<String, UserHistory>>`
- [ ] Query database instead
- [ ] Update `sync_user_history_to_session()` to load from DB

```rust
impl<S> SessionManager<S> {
    async fn sync_user_history_to_session(&self, session_id: &str, public_key: &str) -> Result<()> {
        // Load all messages for this user
        let user_messages = self.db_store.get_user_messages(public_key, 100).await?;

        // Update session state
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.messages = user_messages;
        }

        Ok(())
    }
}
```

---

### Phase 5: Cleanup and Maintenance

#### Step 5.1: Update Cleanup Task
- [ ] Modify cleanup task to remove expired sessions from DB
- [ ] Keep in-memory cleanup for runtime state
- [ ] Add configuration for retention policy

```rust
async fn cleanup_sessions(manager: Arc<SessionManager<S>>) {
    let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes

    loop {
        interval.tick().await;

        let cutoff = chrono::Utc::now().timestamp() - 1800; // 30 minutes ago

        // Database cleanup
        if let Err(e) = manager.db_store.delete_old_sessions(cutoff).await {
            eprintln!("Failed to cleanup old sessions: {}", e);
        }

        // Runtime state cleanup (existing logic)
        // ...
    }
}
```

#### Step 5.2: Add Optional History Trimming
- [ ] Implement message count limits per session
- [ ] Add query to trim old messages
- [ ] Keep last N messages for context

```rust
impl SessionStoreApi for SessionStore {
    async fn trim_session_messages(&self, session_id: &str, keep_count: i32) -> Result<()> {
        sqlx::query(
            "DELETE FROM messages
             WHERE session_id = $1
             AND id NOT IN (
                 SELECT id FROM messages
                 WHERE session_id = $1
                 ORDER BY timestamp DESC
                 LIMIT $2
             )"
        )
        .bind(session_id)
        .bind(keep_count)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
```

---

### Phase 6: Testing and Validation

#### Step 6.1: Unit Tests
- [ ] Test message conversions (ChatMessage ↔ Message)
- [ ] Test metadata serialization/deserialization
- [ ] Test database queries with mock data

#### Step 6.2: Integration Tests
- [ ] Test session creation and restoration
- [ ] Test message persistence and loading
- [ ] Test user history across sessions
- [ ] Test cleanup operations

#### Step 6.3: Migration Testing
- [ ] Test with existing in-memory sessions
- [ ] Verify backward compatibility
- [ ] Test rollback scenarios

---

## Configuration

### Environment Variables

```bash
# Database connection
DATABASE_URL=postgres://aomi@localhost:5432/chatbot

# Session configuration
SESSION_TIMEOUT_SECONDS=1800  # 30 minutes
SESSION_CLEANUP_INTERVAL_SECONDS=300  # 5 minutes
MAX_MESSAGES_PER_SESSION=1000
```

### Feature Flags (Optional)

Consider adding a feature flag for gradual rollout:

```rust
// In config or environment
pub struct Config {
    pub enable_session_persistence: bool,
    pub session_timeout: Duration,
    pub max_messages_per_session: usize,
}

// In SessionManager
if self.config.enable_session_persistence {
    self.db_store.save_chat_message(session_id, &message).await?;
}
```

---

## Migration Strategy

### Gradual Rollout

1. **Phase 1:** Deploy with persistence disabled (feature flag off)
   - Verify database connectivity
   - Test schema creation

2. **Phase 2:** Enable persistence for new sessions only
   - Existing in-memory sessions continue as-is
   - New sessions get persisted

3. **Phase 3:** Enable session restoration
   - Sessions can be restored from database
   - In-memory fallback if DB unavailable

4. **Phase 4:** Full cutover
   - All sessions persisted
   - Remove in-memory fallbacks

### Backward Compatibility

```rust
impl<S> SessionManager<S> {
    async fn get_or_create_session(&self, session_id: &str) -> Result<&SessionRuntimeState<S>> {
        // Try runtime state first (existing behavior)
        if let Some(state) = self.sessions.get(session_id) {
            return Ok(state);
        }

        // Try database (new behavior)
        if self.config.enable_session_persistence {
            if let Ok(Some(session)) = self.db_store.get_session(session_id).await {
                return self.restore_session_state(session).await;
            }
        }

        // Create new session (existing behavior)
        self.create_new_session(session_id)
    }
}
```

---

## Performance Considerations

### Optimization Strategies

1. **Batch Inserts:** Buffer messages and insert in batches during high throughput
2. **Async Persistence:** Don't block user requests on database writes
3. **Caching:** Keep recently accessed sessions in memory
4. **Indexing:** Ensure proper indexes on frequently queried columns
5. **Connection Pooling:** Use appropriate pool size for workload

### Monitoring

Add metrics for:
- Database query latency
- Session restoration time
- Message persistence success rate
- Cache hit/miss ratio
- Session cleanup execution time

---

## Rollback Plan

If issues arise:

1. **Disable persistence feature flag**
2. **Revert to in-memory only mode**
3. **Keep database intact for debugging**
4. **Analyze logs for failure patterns**
5. **Fix issues and re-deploy incrementally**

---

## Schema Summary

### Final Schema (3 Tables, 13 Total Columns)

```sql
-- Users: 3 columns
CREATE TABLE users (
    public_key TEXT PRIMARY KEY,
    username TEXT UNIQUE,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

-- Sessions: 6 columns
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    public_key TEXT REFERENCES users(public_key) ON DELETE SET NULL,
    started_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    last_active_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    title TEXT,
    pending_transaction JSONB
);

-- Messages: 6 columns
CREATE TABLE messages (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    message_type TEXT NOT NULL DEFAULT 'chat',
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    timestamp BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);
```

### Key Benefits of This Design

1. **Simple & Clean** - Minimal columns, no unnecessary metadata
2. **Fast Queries** - Proper indexes, no complex JSONB parsing for common queries
3. **Flexible** - JSONB content handles both simple and complex message structures
4. **Scalable** - Can add analytics tables later without changing core schema
5. **Clear Separation** - Persistent state vs. runtime state is obvious
6. **Easy Migration** - Fewer fields means easier schema evolution
7. **Crash Recovery** - Pending transactions survive server restarts
8. **User Continuity** - Full conversation history persisted across sessions

---

## Success Criteria

- [ ] Sessions persist across server restarts
- [ ] Users can resume conversations after disconnection
- [ ] Agent context (agent_history) maintained across sessions
- [ ] Pending wallet transactions restored after crashes
- [ ] No performance degradation vs. in-memory
- [ ] Database queries complete within 100ms p95
- [ ] Zero data loss for completed messages
- [ ] Graceful degradation if database unavailable
- [ ] Clean separation of persistent vs ephemeral state
