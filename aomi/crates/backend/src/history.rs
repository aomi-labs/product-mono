use std::{sync::Arc, time::Instant, vec};

use anyhow::Result;
use aomi_chat::Message;
use aomi_tools::db::{Session, SessionStore, SessionStoreApi};
use baml_client::{
    apis::{configuration::Configuration, default_api},
    models::{ChatMessage as BamlChatMessage, ConversationSummary, SummarizeConversationRequest},
};
use dashmap::DashMap;
use sqlx::{Any, Pool};

use crate::session::{ChatMessage, MessageSender};

/// Marker string used to detect if a session has historical context loaded
pub const HISTORICAL_CONTEXT_MARKER: &str = "Previous session context:";

// Maximum number of historical chat messages to use when generating context
const MAX_HISTORICAL_MESSAGES: i32 = 100;

/// Creates a system message with the conversation summary for LLM context
fn create_summary_system_message(summary: &ConversationSummary) -> ChatMessage {
    ChatMessage {
        sender: MessageSender::System,
        content: format!(
            "{}
             Topic: {}
             Details: {}
             Where they left off: {}

             Instructions:
             1. Greet the user with this specific summary: \"{}\"
             2. Ask if they'd like to continue that conversation or start fresh
             3. If they want to start fresh (e.g., 'new conversation', 'start over', 'fresh start'), \
             acknowledge it and don't reference the previous context anymore",
            HISTORICAL_CONTEXT_MARKER,
            summary.main_topic,
            summary.key_details.join(", "),
            summary.current_state,
            summary.user_friendly_summary
        ),
        tool_stream: None,
        timestamp: chrono::Utc::now().format("%H:%M:%S UTC").to_string(),
        is_streaming: false,
    }
}

/// Converts a database message to BAML format for conversation summarization
fn db_message_to_baml(db_msg: aomi_tools::db::Message) -> Option<BamlChatMessage> {
    let role = match db_msg.sender.as_str() {
        "user" => "user",
        "agent" => "assistant",
        _ => return None, // Skip system messages
    };

    let content = db_msg
        .content
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some(BamlChatMessage {
        role: role.to_string(),
        content,
    })
}

/// Creates a fallback summary when BAML summarization fails
fn create_fallback_summary() -> ConversationSummary {
    ConversationSummary {
        main_topic: "previous conversation".to_string(),
        key_details: vec![],
        current_state: "unknown".to_string(),
        user_friendly_summary: "I see you have some previous activity.".to_string(),
    }
}

/// Creates BAML configuration from environment
fn get_baml_config() -> Configuration {
    let baml_url =
        std::env::var("BAML_SERVER_URL").unwrap_or_else(|_| "http://localhost:2024".to_string());
    Configuration {
        base_path: baml_url,
        ..Configuration::default()
    }
}

/// Trait for managing user chat history with pluggable storage backends.
///
/// Supports different storage strategies (in-memory, database, no-op) with a persist-on-cleanup
/// model: history is kept in memory during runtime and optionally persisted on session cleanup.
#[async_trait::async_trait]
pub trait HistoryBackend: Send + Sync {
    /// Retrieves existing user history from storage for session initialization.
    /// Returns historical messages (if any) to initialize the session state.
    /// The session state will convert these to rig Messages for LLM context.
    async fn get_or_create_history(
        &self,
        pubkey: Option<String>,
        session_id: String,
    ) -> Result<Option<ChatMessage>>;

    /// Updates the in-memory user history with new messages for a specific session.
    /// Called periodically during runtime. Does NOT persist to storage.
    fn update_history(&self, session_id: &str, messages: &[ChatMessage]);

    /// Persists user history to durable storage during session cleanup.
    /// Saves all messages in the current session to database.
    async fn flush_history(&self, pubkey: Option<String>, session_id: String) -> Result<()>;
}

struct SessionHistory {
    messages: Vec<ChatMessage>,
    #[allow(dead_code)]
    last_activity: Instant,
}

pub struct PersistentHistoryBackend {
    db: SessionStore,
    sessions: Arc<DashMap<String, SessionHistory>>,
}

impl PersistentHistoryBackend {
    pub async fn new(pool: Pool<Any>) -> Self {
        let db = SessionStore::new(pool);
        Self {
            sessions: Arc::new(DashMap::new()),
            db,
        }
    }

    pub fn get_session_messages(&self, session_id: &str) -> Option<Vec<ChatMessage>> {
        self.sessions
            .get(session_id)
            .map(|entry| entry.messages.clone())
    }

    pub fn get_session_conversation_messages(&self, session_id: &str) -> Option<Vec<ChatMessage>> {
        self.sessions
            .get(session_id)
            .map(|entry| filter_system_messages(&entry.messages))
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
        &self,
        pubkey: Option<String>,
        session_id: String,
    ) -> Result<Option<ChatMessage>> {
        // If no pubkey, don't create any db records (anonymous session)
        let Some(pk) = pubkey.as_ref() else {
            return Ok(None);
        };

        // Ensure user exists in database
        let _ = self.db.get_or_create_user(pk).await?;

        if self.db.get_session(&session_id).await?.is_none() {
            // Creating a new session
            self.db
                .create_session(&Session {
                    id: session_id.clone(),
                    public_key: pubkey.clone(),
                    started_at: chrono::Utc::now().timestamp(),
                    last_active_at: chrono::Utc::now().timestamp(),
                    title: None,
                    pending_transaction: None,
                })
                .await?;
        } 

        // Load user's most recent session messages for context
        // The LLM can use this to:
        // 1. Summarize the previous conversation
        // 2. Ask if user wants to continue or start fresh
        // 3. Clear context if user says "start fresh", "new conversation", etc.
        let recent_messages = self
            .db
            .get_user_message_history(pk, MAX_HISTORICAL_MESSAGES)
            .await?;

        tracing::info!(
            "Loaded {} historical messages for user {} in new session {}",
            recent_messages.len(),
            pk,
            session_id
        );

        if recent_messages.is_empty() {
            tracing::info!("No historical messages found, starting fresh session");
            return Ok(None);
        }

        // Convert DB messages to BAML format for summarization
        let baml_messages: Vec<BamlChatMessage> = recent_messages
            .into_iter()
            .rev() // Reverse because get_user_message_history returns DESC order
            .filter_map(db_message_to_baml)
            .collect();

        // Call BAML to summarize the conversation
        let config = get_baml_config();
        let request = SummarizeConversationRequest::new(baml_messages);
        let summary = match default_api::summarize_conversation(&config, request).await {
            Ok(s) => Some(create_summary_system_message(&s)),
            Err(_) => {
                None
            }
        };

        tracing::info!("Generated conversation summary: {:?}", summary);
        
        return Ok(summary);   
    }

    fn update_history(&self, session_id: &str, messages: &[ChatMessage]) {
        // Update with new non-streaming messages only
        let filtered: Vec<ChatMessage> = messages
            .iter()
            .filter(|msg| !msg.is_streaming)
            .cloned()
            .collect();

        // Update or insert session history
        self.sessions.insert(
            session_id.to_string(),
            SessionHistory {
                messages: filtered,
                last_activity: Instant::now(),
            },
        );
    }

    async fn flush_history(&self, pubkey: Option<String>, session_id: String) -> Result<()> {
        // Only persist if pubkey is provided
        let Some(pk) = pubkey else {
            return Ok(());
        };

        // Ensure user exists
        let _ = self.db.get_or_create_user(&pk).await?;

        // Verify session exists in database before attempting to save messages
        if self.db.get_session(&session_id).await?.is_none() {
            tracing::warn!(
                "Session {} does not exist in database, skipping flush",
                session_id
            );
            return Ok(());
        }

        // Get messages to persist from the session's history
        let messages = match self.sessions.get(&session_id) {
            Some(entry) => entry.messages.clone(),
            None => return Ok(()), // No messages to flush for this session
        };

        // Save all messages to database (they're all new)
        for message in &messages {
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
            messages.len(),
            session_id
        );

        // Remove session from in-memory cache after flushing
        self.sessions.remove(&session_id);

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
        let backend = PersistentHistoryBackend::new(pool).await;

        // Call with no pubkey
        let history = backend
            .get_or_create_history(None, "anonymous-session".to_string())
            .await?;

        assert!(
            history.is_none(),
            "Anonymous session should return empty history"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_new_session_creates_user_and_session() -> Result<()> {
        let pool = setup_test_db().await?;
        let backend = PersistentHistoryBackend::new(pool.clone()).await;

        let pubkey = "0xTEST123".to_string();
        let session_id = "new-session".to_string();

        // First call should create user and session, return empty
        let history = backend
            .get_or_create_history(Some(pubkey.clone()), session_id.clone())
            .await?;

        assert!(
            history.is_none(),
            "New session should return empty history"
        );

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
    async fn test_update_history_filters_streaming() -> Result<()> {
        let pool = setup_test_db().await?;
        let backend = PersistentHistoryBackend::new(pool).await;
        let session_id = "test-session";

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

        backend.update_history(session_id, &messages);

        let stored_messages = backend.get_session_messages(session_id).unwrap();
        assert_eq!(
            stored_messages.len(),
            2,
            "Should only store non-streaming messages"
        );
        assert!(!stored_messages[0].is_streaming);
        assert!(!stored_messages[1].is_streaming);

        Ok(())
    }

    #[tokio::test]
    async fn test_flush_history_persists_messages() -> Result<()> {
        let pool = setup_test_db().await?;
        let backend = PersistentHistoryBackend::new(pool.clone()).await;
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

        backend.update_history(&session_id, &messages);

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
        let backend = PersistentHistoryBackend::new(pool.clone()).await;
        let db = SessionStore::new(pool.clone());

        let session_id = "no-pubkey-session".to_string();

        // Add messages to in-memory state
        let messages = vec![test_message(MessageSender::User, "Test message")];
        backend.update_history(&session_id, &messages);

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
