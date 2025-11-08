use std::{time::Instant, vec};

use anyhow::Result;
use aomi_chat::Message;
use aomi_tools::db::{Session, SessionStore, SessionStoreApi};
use sqlx::{Any, Pool};

use crate::session::{ChatMessage, MessageSender};

/// Trait for managing user chat history with pluggable storage backends.
///
/// Supports different storage strategies (in-memory, database, no-op) with a persist-on-cleanup
/// model: history is kept in memory during runtime and optionally persisted on session cleanup.
#[async_trait::async_trait]
pub trait HistoryBackend {
    /// Retrieves existing user history from storage for LLM summarization.
    /// Returns historical messages (if any) for the LLM to summarize in welcome message.
    /// Does NOT add them to the current session's message list.
    async fn get_or_create_history(
        &mut self,
        pubkey: Option<String>,
        session_id: String,
    ) -> Result<Vec<Message>>;

    /// Updates the in-memory user history with new messages.
    /// Called periodically during runtime. Does NOT persist to storage.
    fn update_history(&mut self, messages: &[ChatMessage]);

    /// Persists user history to durable storage during session cleanup.
    /// Saves all messages in the current session to database.
    async fn flush_history(&self, pubkey: Option<String>, session_id: String) -> Result<()>;
}

#[derive(Clone)]
pub struct PersistentHistoryBackend {
    db: SessionStore,
    messages: Vec<ChatMessage>,
    last_activity: Instant,
}

impl PersistentHistoryBackend {
    pub async fn new(messages: Vec<ChatMessage>, last_activity: Instant, pool: Pool<Any>) -> Self {
        let db = SessionStore::new(pool);
        Self {
            messages,
            last_activity,
            db,
        }
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn into_messages(self) -> Vec<ChatMessage> {
        self.messages
    }

    pub fn conversation_messages(&self) -> Vec<ChatMessage> {
        filter_system_messages(&self.messages)
    }
}

pub fn filter_system_messages(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    messages
        .iter()
        .filter(|&msg| !matches!(msg.sender, MessageSender::System))
        .cloned()
        .collect()
}

pub fn to_rig_messages(messages: &[ChatMessage]) -> Vec<Message> {
    filter_system_messages(messages)
        .into_iter()
        .map(Message::from)
        .collect()
}

#[async_trait::async_trait]
impl HistoryBackend for PersistentHistoryBackend {
    async fn get_or_create_history(
        &mut self,
        pubkey: Option<String>,
        session_id: String,
    ) -> Result<Vec<Message>> {
        // If no pubkey, don't create any db records (anonymous session)
        let Some(pk) = pubkey.as_ref() else {
            return Ok(vec![]);
        };

        // Ensure user exists in database
        let _ = self.db.get_or_create_user(pk).await?;

        if self.db.get_session(&session_id).await?.is_none() {
            //create session
            self.db
                .create_session(&Session {
                    id: session_id,
                    public_key: pubkey,
                    started_at: chrono::Utc::now().timestamp(),
                    last_active_at: chrono::Utc::now().timestamp(),
                    title: None,
                    pending_transaction: None,
                })
                .await?;
            return Ok(vec![]);
        }

        // Load existing messages from database for this session
        let db_messages = self
            .db
            .get_messages(&session_id, Some("chat"), None)
            .await?;
        if db_messages.is_empty() {
            return Ok(Vec::new());
        }

        // Convert DB messages to ChatMessages
        let chat_messages: Vec<ChatMessage> = db_messages
            .into_iter()
            .map(|db_msg| {
                let sender = match db_msg.sender.as_str() {
                    "user" => MessageSender::User,
                    "agent" => MessageSender::Assistant,
                    _ => MessageSender::System,
                };
                let content = db_msg
                    .content
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                ChatMessage {
                    sender,
                    content,
                    tool_stream: None,
                    timestamp: chrono::DateTime::from_timestamp(db_msg.timestamp, 0)
                        .map(|dt| dt.format("%H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| "00:00:00 UTC".to_string()),
                    is_streaming: false,
                }
            })
            .collect();

        println!(
            "📚 Loaded {} historical messages for session {}",
            chat_messages.len(),
            session_id
        );

        // Convert to rig Messages for LLM
        Ok(to_rig_messages(&chat_messages))
    }

    fn update_history(&mut self, messages: &[ChatMessage]) {
        // Update with new non-streaming messages only
        self.messages = messages
            .iter()
            .filter(|msg| !msg.is_streaming)
            .cloned()
            .collect();
        self.last_activity = Instant::now();
    }

    async fn flush_history(&self, pubkey: Option<String>, session_id: String) -> Result<()> {
        // Only persist if pubkey is provided
        let Some(pk) = pubkey else {
            return Ok(());
        };

        // Ensure user exists
        let _ = self.db.get_or_create_user(&pk).await?;

        // Save all messages in self.messages to database (they're all new)
        for message in &self.messages {
            // Skip system messages
            if matches!(message.sender, MessageSender::System) {
                continue;
            }

            let db_msg = aomi_tools::db::Message {
                id: 0, // Will be auto-assigned by database
                session_id: session_id.clone(),
                message_type: "chat".to_string(),
                sender: match message.sender {
                    MessageSender::User => "user".to_string(),
                    MessageSender::Assistant => "agent".to_string(),
                    MessageSender::System => "system".to_string(),
                },
                content: serde_json::json!({"text": message.content}),
                timestamp: chrono::Utc::now().timestamp(),
            };

            self.db.save_message(&db_msg).await?;
        }

        println!(
            "💾 Flushed {} messages to database for session {}",
            self.messages.len(),
            session_id
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::any::AnyPoolOptions;

    async fn setup_test_db() -> Result<Pool<Any>> {
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        // Create schema
        sqlx::query(
            r#"
            CREATE TABLE users (
                public_key TEXT PRIMARY KEY,
                username TEXT UNIQUE,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                public_key TEXT REFERENCES users(public_key) ON DELETE SET NULL,
                started_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                last_active_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                title TEXT,
                pending_transaction TEXT
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                message_type TEXT NOT NULL DEFAULT 'chat',
                sender TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(pool)
    }

    fn test_message(sender: MessageSender, content: &str) -> ChatMessage {
        ChatMessage {
            sender,
            content: content.to_string(),
            tool_stream: None,
            timestamp: "00:00:00 UTC".to_string(),
            is_streaming: false,
        }
    }

    #[tokio::test]
    async fn test_anonymous_session_returns_empty() -> Result<()> {
        let pool = setup_test_db().await?;
        let mut backend = PersistentHistoryBackend::new(vec![], Instant::now(), pool).await;

        // Call with no pubkey
        let history = backend
            .get_or_create_history(None, "anonymous-session".to_string())
            .await?;

        assert!(history.is_empty(), "Anonymous session should return empty history");
        Ok(())
    }

    #[tokio::test]
    async fn test_new_session_creates_user_and_session() -> Result<()> {
        let pool = setup_test_db().await?;
        let mut backend = PersistentHistoryBackend::new(vec![], Instant::now(), pool.clone()).await;

        let pubkey = "0xTEST123".to_string();
        let session_id = "new-session".to_string();

        // First call should create user and session, return empty
        let history = backend
            .get_or_create_history(Some(pubkey.clone()), session_id.clone())
            .await?;

        assert!(history.is_empty(), "New session should return empty history");

        // Verify user was created
        let db = SessionStore::new(pool.clone());
        let user = db.get_user(&pubkey).await?;
        assert!(user.is_some(), "User should be created");

        // Verify session was created
        let session = db.get_session(&session_id).await?;
        assert!(session.is_some(), "Session should be created");
        assert_eq!(session.unwrap().public_key, Some(pubkey));

        Ok(())
    }

    #[tokio::test]
    async fn test_existing_session_loads_messages() -> Result<()> {
        let pool = setup_test_db().await?;
        let mut backend = PersistentHistoryBackend::new(vec![], Instant::now(), pool.clone()).await;
        let db = SessionStore::new(pool.clone());

        let pubkey = "0xTEST456".to_string();
        let session_id = "existing-session".to_string();

        // Create user
        db.get_or_create_user(&pubkey).await?;

        // Create session
        db.create_session(&Session {
            id: session_id.clone(),
            public_key: Some(pubkey.clone()),
            started_at: chrono::Utc::now().timestamp(),
            last_active_at: chrono::Utc::now().timestamp(),
            title: None,
            pending_transaction: None,
        })
        .await?;

        // Add messages to the session
        db.save_message(&aomi_tools::db::Message {
            id: 0,
            session_id: session_id.clone(),
            message_type: "chat".to_string(),
            sender: "user".to_string(),
            content: serde_json::json!({"text": "Hello"}),
            timestamp: chrono::Utc::now().timestamp(),
        })
        .await?;

        db.save_message(&aomi_tools::db::Message {
            id: 0,
            session_id: session_id.clone(),
            message_type: "chat".to_string(),
            sender: "agent".to_string(),
            content: serde_json::json!({"text": "Hi there!"}),
            timestamp: chrono::Utc::now().timestamp(),
        })
        .await?;

        // Load history
        let history = backend
            .get_or_create_history(Some(pubkey), session_id)
            .await?;

        assert_eq!(history.len(), 2, "Should load 2 messages from history");
        Ok(())
    }

    #[tokio::test]
    async fn test_update_history_filters_streaming() -> Result<()> {
        let pool = setup_test_db().await?;
        let mut backend = PersistentHistoryBackend::new(vec![], Instant::now(), pool).await;

        let messages = vec![
            test_message(MessageSender::User, "Message 1"),
            ChatMessage {
                sender: MessageSender::Assistant,
                content: "Streaming...".to_string(),
                tool_stream: None,
                timestamp: "00:00:01 UTC".to_string(),
                is_streaming: true, // This should be filtered out
            },
            test_message(MessageSender::Assistant, "Complete message"),
        ];

        backend.update_history(&messages);

        assert_eq!(
            backend.messages().len(),
            2,
            "Should only store non-streaming messages"
        );
        assert!(!backend.messages()[0].is_streaming);
        assert!(!backend.messages()[1].is_streaming);

        Ok(())
    }

    #[tokio::test]
    async fn test_flush_history_persists_messages() -> Result<()> {
        let pool = setup_test_db().await?;
        let mut backend = PersistentHistoryBackend::new(vec![], Instant::now(), pool.clone()).await;
        let db = SessionStore::new(pool.clone());

        let pubkey = "0xFLUSH".to_string();
        let session_id = "flush-session".to_string();

        // Create user and session first
        backend
            .get_or_create_history(Some(pubkey.clone()), session_id.clone())
            .await?;

        // Add messages to in-memory state
        let messages = vec![
            test_message(MessageSender::User, "User message"),
            test_message(MessageSender::Assistant, "Agent reply"),
            test_message(MessageSender::System, "System message"), // Should be skipped
        ];

        backend.update_history(&messages);

        // Flush to database
        backend
            .flush_history(Some(pubkey), session_id.clone())
            .await?;

        // Verify messages in database
        let db_messages = db.get_messages(&session_id, Some("chat"), None).await?;

        assert_eq!(
            db_messages.len(),
            2,
            "Should persist 2 messages (system excluded)"
        );
        assert_eq!(db_messages[0].sender, "user");
        assert_eq!(db_messages[1].sender, "agent");

        Ok(())
    }

    #[tokio::test]
    async fn test_flush_history_without_pubkey_does_nothing() -> Result<()> {
        let pool = setup_test_db().await?;
        let mut backend = PersistentHistoryBackend::new(vec![], Instant::now(), pool.clone()).await;
        let db = SessionStore::new(pool.clone());

        let session_id = "no-pubkey-session".to_string();

        // Add messages to in-memory state
        let messages = vec![test_message(MessageSender::User, "Test message")];
        backend.update_history(&messages);

        // Flush without pubkey should succeed but not persist
        backend.flush_history(None, session_id.clone()).await?;

        // Verify no messages in database
        let result = db.get_messages(&session_id, Some("chat"), None).await;

        // Session doesn't exist, so this should return error or empty
        assert!(result.is_err() || result.unwrap().is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_filter_system_messages() {
        let messages = vec![
            test_message(MessageSender::User, "User"),
            test_message(MessageSender::Assistant, "Assistant"),
            test_message(MessageSender::System, "System"),
        ];

        let filtered = filter_system_messages(&messages);

        assert_eq!(filtered.len(), 2, "Should filter out system messages");
        assert!(matches!(filtered[0].sender, MessageSender::User));
        assert!(matches!(filtered[1].sender, MessageSender::Assistant));
    }
}
