use std::{sync::Arc, time::Instant};

use anyhow::Result;
use aomi_baml::baml_client::{
    async_client::B,
    types::{ChatMessage as BamlChatMessage, ConversationSummary},
};
use aomi_core::{prompts::create_summary_content, Message};
use aomi_tools::db::{Session, SessionStore, SessionStoreApi};
use dashmap::DashMap;
use sqlx::{Any, Pool};

use crate::types::{ChatMessage, HistorySession, MessageSender};

/// Marker string used to detect if a session has historical context loaded
pub const HISTORICAL_CONTEXT_MARKER: &str = "Previous session context:";

// Maximum number of historical chat messages to use when generating context
const MAX_HISTORICAL_MESSAGES: i32 = 100;

/// Creates a system message with the conversation summary for LLM context
fn create_summary_system_message(summary: &ConversationSummary) -> ChatMessage {
    let content = create_summary_content(
        HISTORICAL_CONTEXT_MARKER,
        &summary.title,
        &summary.key_details.join(", "),
        &summary.current_state,
        &summary.user_friendly_summary,
    );

    ChatMessage {
        sender: MessageSender::System,
        content,
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

/// Trait for managing user chat history with pluggable storage backends.
///
/// Supports different storage strategies (in-memory, database, no-op) with a persist-on-cleanup
/// model: history is kept in memory during runtime and optionally persisted on session cleanup.
#[derive(Debug, Clone)]
pub struct StoredSession {
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub public_key: Option<String>,
}

#[async_trait::async_trait]
pub trait HistoryBackend: Send + Sync {
    /// Retrieves existing user history from storage for session initialization.
    /// Returns historical messages (if any) to initialize the session state.
    /// The session state will convert these to rig Messages for LLM context.
    /// If creating a new session, the provided title will be persisted.
    async fn get_or_create_history(
        &self,
        pubkey: Option<String>,
        session_id: String,
        title: Option<String>,
    ) -> Result<Option<ChatMessage>>;

    /// Updates the in-memory user history with new messages for a specific session.
    /// Called periodically during runtime. Does NOT persist to storage.
    fn update_history(&self, session_id: &str, messages: &[ChatMessage]);

    /// Persists user history to durable storage during session cleanup.
    /// Saves all messages in the current session to database.
    async fn flush_history(&self, pubkey: Option<String>, session_id: String) -> Result<()>;

    /// Lists sessions for a user to power sidebar navigation.
    async fn get_history_sessions(
        &self,
        public_key: &str,
        limit: usize,
    ) -> Result<Vec<HistorySession>>;

    /// Retrieves a session and its messages directly from persistent storage.
    /// Returns StoredSession if session exists, None otherwise.
    /// Default implementation returns None (no-op for non-persistent backends).
    async fn get_session_from_storage(&self, session_id: &str) -> Result<Option<StoredSession>> {
        let _ = session_id;
        Ok(None)
    }

    /// Deletes a session from persistent storage.
    /// Default implementation is a no-op for non-persistent backends.
    async fn delete_session(&self, session_id: &str) -> Result<()> {
        let _ = session_id;
        Ok(())
    }

    /// Updates whether a session's messages have been persisted.
    /// Default implementation is a no-op for non-persistent backends.
    async fn set_messages_persisted(&self, session_id: &str, persisted: bool) -> Result<()> {
        let _ = (session_id, persisted);
        Ok(())
    }

    /// Persists a session's title change to storage (if supported).
    async fn update_session_title(&self, session_id: &str, title: &str) -> Result<()>;
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

    /// Query a session directly from the database
    pub async fn query_session_from_db(&self, session_id: &str) -> Result<Option<StoredSession>> {
        match self.db.get_session(session_id).await? {
            Some(session) => {
                let mut messages = self.db.get_messages(session_id, None, None).await?;
                messages.sort_by_key(|msg| msg.id);
                let chat_messages = messages
                    .into_iter()
                    .filter_map(db_message_to_baml)
                    .map(|baml_msg| ChatMessage {
                        sender: match baml_msg.role.as_str() {
                            "user" => MessageSender::User,
                            _ => MessageSender::Assistant,
                        },
                        content: baml_msg.content,
                        tool_stream: None,
                        timestamp: chrono::Utc::now().format("%H:%M:%S UTC").to_string(),
                        is_streaming: false,
                    })
                    .collect();

                let title = session.title.unwrap_or_else(|| {
                    // Use `#[id]` marker format for fallback titles
                    format!("#[{}]", &session.id[..6.min(session.id.len())])
                });

                Ok(Some(StoredSession {
                    title,
                    messages: chat_messages,
                    public_key: session.public_key,
                }))
            }
            None => Ok(None),
        }
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
        title: Option<String>,
    ) -> Result<Option<ChatMessage>> {
        // If no pubkey, don't create any db records (anonymous session)
        let Some(pk) = pubkey.as_ref() else {
            return Ok(None);
        };

        // Ensure user exists in database
        let _ = self.db.get_or_create_user(pk).await?;

        match self.db.get_session(&session_id).await? {
            Some(existing) => {
                if existing.public_key.as_ref() != Some(pk) {
                    self.db
                        .update_session_public_key(&session_id, Some(pk.clone()))
                        .await?;
                }
            }
            None => {
                // Creating a new session with the provided title
                self.db
                    .create_session(&Session {
                        id: session_id.clone(),
                        public_key: pubkey.clone(),
                        started_at: chrono::Utc::now().timestamp(),
                        last_active_at: chrono::Utc::now().timestamp(),
                        title,
                        pending_transaction: None,
                    })
                    .await?;
            }
        }

        // Load user's most recent session messages for context
        // The LLM can use this to:
        // 1. Generate a summary of the previous conversation
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

        // Call BAML to generate the conversation summary (native FFI - no HTTP)
        let summary = match B.GenerateConversationSummary.call(&baml_messages).await {
            Ok(s) => Some(create_summary_system_message(&s)),
            Err(_) => None,
        };

        tracing::debug!("Generated conversation summary: {:?}", summary);

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
        tracing::info!("Flushing history for session {}", session_id);
        let pubkey = match pubkey {
            Some(pk) => Some(pk),
            None => self
                .db
                .get_session(&session_id)
                .await?
                .and_then(|session| session.public_key),
        };

        // Only persist if pubkey is available
        let Some(pk) = pubkey else {
            tracing::info!("No pubkey provided, skipping flush");
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

        if self
            .db
            .get_messages_persisted(&session_id)
            .await?
            .unwrap_or(false)
        {
            tracing::info!(
                "Messages already persisted for {}, skipping flush",
                session_id
            );
            return Ok(());
        }

        tracing::info!("Flushing history for session {}", session_id);

        // Get messages to persist from the session's history
        let messages = match self.sessions.get(&session_id) {
            Some(entry) => entry.messages.clone(),
            None => return Ok(()), // No messages to flush for this session
        };

        tracing::debug!("Messages to flush: {:?}", messages);

        // Save all messages to database (they're all new)
        for message in &messages {
            // Skip system messages
            if matches!(message.sender, MessageSender::System) {
                continue;
            }

            tracing::debug!("Saving message to database: {:?}", message);

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

            tracing::debug!("Saving message to database: {:?}", db_msg);

            self.db.save_message(&db_msg).await?;
        }

        println!(
            "💾 Flushed {} messages to database for session {}",
            messages.len(),
            session_id
        );

        self.db.update_messages_persisted(&session_id, true).await?;

        // Remove session from in-memory cache after flushing
        self.sessions.remove(&session_id);

        Ok(())
    }

    async fn get_history_sessions(
        &self,
        public_key: &str,
        limit: usize,
    ) -> Result<Vec<HistorySession>> {
        let db_limit = limit.min(i32::MAX as usize) as i32;
        let sessions: Vec<Session> = self.db.get_user_sessions(public_key, db_limit).await?;

        Ok(sessions
            .into_iter()
            .map(|session| HistorySession {
                session_id: session.id.clone(),
                title: session.title.unwrap_or_else(|| {
                    // Use `#[id]` marker format for fallback titles
                    format!("#[{}]", &session.id[..6.min(session.id.len())])
                }),
            })
            .collect())
    }

    async fn get_session_from_storage(&self, session_id: &str) -> Result<Option<StoredSession>> {
        self.query_session_from_db(session_id).await
    }

    async fn delete_session(&self, session_id: &str) -> Result<()> {
        self.db.delete_session(session_id).await
    }

    async fn set_messages_persisted(&self, session_id: &str, persisted: bool) -> Result<()> {
        self.db
            .update_messages_persisted(session_id, persisted)
            .await
    }

    async fn update_session_title(&self, session_id: &str, title: &str) -> Result<()> {
        // Only update if session exists in database
        if self.db.get_session(session_id).await?.is_none() {
            tracing::info!(
                "Session {} does not exist in database, skipping title update",
                session_id
            );
            return Ok(());
        }

        self.db
            .update_session_title(session_id, title.to_string())
            .await?;
        Ok(())
    }
}
