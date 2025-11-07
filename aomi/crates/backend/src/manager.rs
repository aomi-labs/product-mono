use aomi_chat::ChatApp;
use aomi_tools::db::{SessionStore, SessionStoreApi};
use dashmap::DashMap;
use sqlx::{any::Any, Pool};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    history::UserHistory,
    session::{ChatBackend, ChatMessage, DefaultSessionState, MessageSender},
};
use aomi_chat::ToolResultStream;

struct SessionData {
    state: Arc<Mutex<DefaultSessionState>>,
    last_activity: Instant,
}

pub struct SessionManager {
    sessions: Arc<DashMap<String, SessionData>>,
    user_history: Arc<DashMap<String, UserHistory>>,
    session_public_keys: Arc<DashMap<String, String>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    chat_backend: Arc<dyn ChatBackend<ToolResultStream>>,
    db_store: Option<Arc<dyn SessionStoreApi>>,
}

impl SessionManager {
    pub fn new(chat_app: Arc<ChatApp>) -> Self {
        Self::with_backend(chat_app)
    }

    pub fn with_backend(chat_backend: Arc<dyn ChatBackend<ToolResultStream>>) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            user_history: Arc::new(DashMap::new()),
            session_public_keys: Arc::new(DashMap::new()),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(1800), // 30 minutes
            chat_backend,
            db_store: None,
        }
    }

    pub fn with_database(
        chat_backend: Arc<dyn ChatBackend<ToolResultStream>>,
        db_pool: Pool<Any>,
    ) -> Self {
        let store = SessionStore::new(db_pool);
        Self {
            sessions: Arc::new(DashMap::new()),
            user_history: Arc::new(DashMap::new()),
            session_public_keys: Arc::new(DashMap::new()),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(1800), // 30 minutes
            chat_backend,
            db_store: Some(Arc::new(store)),
        }
    }

    #[allow(dead_code)]
    pub async fn get_or_create_history(&self, public_key: &Option<String>) -> Option<UserHistory> {
        public_key.as_ref().map(|public_key| {
            self.user_history
                .get(public_key.as_str())
                .map(|entry| entry.clone())
                .unwrap_or_else(|| UserHistory::empty_with_activity(Instant::now()))
        })
    }

    pub async fn set_session_public_key(&self, session_id: &str, public_key: Option<String>) {
        if let Some(pk) = public_key.clone() {
            // Update in-memory mapping
            self.session_public_keys
                .insert(session_id.to_string(), pk.clone());

            // Persist to database
            if let Some(db) = &self.db_store {
                // Ensure user exists in database
                if let Err(e) = db.get_or_create_user(&pk).await {
                    tracing::warn!("Failed to create user in DB: {}", e);
                    return;
                }

                // Update session's public_key
                if let Err(e) = db
                    .update_session_public_key(session_id, Some(pk.clone()))
                    .await
                {
                    tracing::warn!("Failed to update session public_key in DB: {}", e);
                    return;
                }

                println!(
                    "🔑 Associated session {} with user {}",
                    session_id,
                    &pk[..8]
                );
            }
        }
    }

    async fn get_user_history_with_pubkey(&self, session_id: &str) -> Option<Vec<ChatMessage>> {
        let public_key = self.session_public_keys.get(session_id)?;
        let pk = public_key.value().clone();
        drop(public_key);

        // Try to load from database first
        if let Some(db) = &self.db_store {
            match db.get_user_message_history(&pk, 100).await {
                Ok(db_messages) if !db_messages.is_empty() => {
                    println!(
                        "📚 Loading user history from database for user: {}",
                        &pk[..8]
                    );
                    return Some(self.db_messages_to_chat_messages(&db_messages));
                }
                Err(e) => {
                    tracing::warn!("Failed to load user history from DB: {}", e);
                }
                _ => {}
            }
        }

        // Fallback to in-memory user history
        self.user_history
            .get(&pk)
            .map(|h| h.clone().into_messages())
    }

    pub async fn get_or_create_session(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        // Check in-memory sessions first
        match self.sessions.get_mut(session_id) {
            Some(mut session_data) => {
                let state = session_data.state.clone();
                session_data.last_activity = Instant::now();

                // Update activity in DB if available
                if let Some(db) = &self.db_store {
                    let _ = db.update_session_activity(session_id).await;
                }

                // Note: User history syncing is no longer needed because messages are
                // automatically persisted to the database via persist_session_messages
                // which is called during the streaming loop in the endpoint

                Ok(state)
            }
            None => {
                // Try to load from database
                let (initial_messages, db_session_opt) = if let Some(db) = &self.db_store {
                    match db.get_session(session_id).await {
                        Ok(Some(db_session)) => {
                            println!("🔄 Restoring session from database: {}", session_id);

                            // Load chat messages from DB
                            let db_messages = db
                                .get_messages(session_id, Some("chat"), None)
                                .await
                                .unwrap_or_default();

                            // Convert DB messages to ChatMessages
                            let messages = self.db_messages_to_chat_messages(&db_messages);
                            (messages, Some(db_session))
                        }
                        Ok(None) => {
                            // Session not in DB, check user history
                            let messages = self
                                .get_user_history_with_pubkey(session_id)
                                .await
                                .unwrap_or_default();
                            (messages, None)
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load session from DB: {}", e);
                            let messages = self
                                .get_user_history_with_pubkey(session_id)
                                .await
                                .unwrap_or_default();
                            (messages, None)
                        }
                    }
                } else {
                    // No DB, use in-memory user history
                    let messages = self
                        .get_user_history_with_pubkey(session_id)
                        .await
                        .unwrap_or_default();
                    (messages, None)
                };

                // Create new session state
                let session_state =
                    DefaultSessionState::new(Arc::clone(&self.chat_backend), initial_messages)
                        .await?;
                let session_data = SessionData {
                    state: Arc::new(Mutex::new(session_state)),
                    last_activity: Instant::now(),
                };
                let new_session = session_data.state.clone();
                self.sessions.insert(session_id.to_string(), session_data);

                // Restore pending transaction if session was loaded from DB
                if let Some(db_session) = db_session_opt {
                    if let Err(e) = self
                        .restore_pending_transaction(
                            session_id,
                            &db_session,
                            Arc::clone(&new_session),
                        )
                        .await
                    {
                        tracing::warn!("Failed to restore pending transaction: {}", e);
                    }
                } else {
                    // Persist new session to DB if it wasn't loaded from DB
                    if let Some(db) = &self.db_store {
                        let now = chrono::Utc::now().timestamp();
                        let db_session = aomi_tools::db::Session {
                            id: session_id.to_string(),
                            public_key: self
                                .session_public_keys
                                .get(session_id)
                                .map(|r| r.value().clone()),
                            started_at: now,
                            last_active_at: now,
                            title: None,
                            pending_transaction: None,
                        };
                        if let Err(e) = db.create_session(&db_session).await {
                            tracing::warn!("Failed to persist session to DB: {}", e);
                        }
                    }
                }

                println!("📝 Created new session: {}", session_id);
                Ok(new_session)
            }
        }
    }

    fn db_messages_to_chat_messages(
        &self,
        db_messages: &[aomi_tools::db::Message],
    ) -> Vec<ChatMessage> {
        db_messages
            .iter()
            .filter_map(|msg| {
                let sender = match msg.sender.as_str() {
                    "user" => MessageSender::User,
                    "agent" => MessageSender::Assistant,
                    "system" => MessageSender::System,
                    _ => return None,
                };

                let content = msg
                    .content
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let timestamp = chrono::DateTime::from_timestamp(msg.timestamp, 0)
                    .map(|dt| dt.format("%H:%M:%S %Z").to_string())
                    .unwrap_or_default();

                Some(ChatMessage {
                    sender,
                    content,
                    tool_stream: None,
                    timestamp,
                    is_streaming: false,
                })
            })
            .collect()
    }

    #[allow(dead_code)]
    pub async fn remove_session(&self, session_id: &str) {
        if self.sessions.remove(session_id).is_some() {
            println!("🗑️ Manually removed session: {}", session_id);
        }
    }

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

                // Clean up in-memory sessions
                sessions.retain(|session_id, session_data| {
                    let should_keep =
                        now.duration_since(session_data.last_activity) < session_timeout;
                    if !should_keep {
                        println!("🗑️ Cleaning up inactive session: {}", session_id);
                    }
                    should_keep
                });

                // Clean up database sessions
                if let Some(db) = &db_store {
                    let cutoff_timestamp =
                        chrono::Utc::now().timestamp() - session_timeout.as_secs() as i64;

                    match db.delete_old_sessions(cutoff_timestamp).await {
                        Ok(deleted_count) => {
                            if deleted_count > 0 {
                                println!(
                                    "🗑️ Cleaned up {} old sessions from database",
                                    deleted_count
                                );
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

    #[allow(dead_code)]
    pub async fn get_active_session_count(&self) -> usize {
        self.sessions.len()
    }

    pub async fn update_user_history(
        &self,
        _session_id: &str,
        public_key: Option<String>,
        messages: &[ChatMessage],
    ) {
        if let Some(public_key) = public_key {
            // Keep in-memory for backward compatibility / fast access
            self.user_history.insert(
                public_key.clone(),
                UserHistory::from_messages_now(messages.to_vec()),
            );

            // The messages are already being persisted to database via persist_session_messages
            // which is called in the same streaming loop, so we don't need to duplicate that here.
            // The session is already associated with the public_key via set_session_public_key,
            // so we can query user messages across all their sessions using get_user_message_history.
        }
    }

    /// Restore pending transaction from database
    async fn restore_pending_transaction(
        &self,
        session_id: &str,
        session: &aomi_tools::db::Session,
        session_state: Arc<Mutex<DefaultSessionState>>,
    ) -> anyhow::Result<()> {
        if let Some(db) = &self.db_store {
            if let Some(pending_tx) = session.get_pending_transaction()? {
                let now = chrono::Utc::now().timestamp();

                // Check if transaction has expired
                if now > pending_tx.expires_at {
                    println!(
                        "🗑️  Clearing expired pending transaction for session: {}",
                        session_id
                    );

                    // Clear from database
                    db.update_pending_transaction(session_id, None).await?;

                    // Add system message to notify user
                    let mut state = session_state.lock().await;
                    state.add_system_message("Your previous pending transaction has expired");
                } else {
                    println!(
                        "🔄 Restoring pending transaction for session: {}",
                        session_id
                    );

                    // Restore to runtime state
                    let mut state = session_state.lock().await;
                    state.pending_wallet_tx = Some(pending_tx.user_intent.clone());

                    // Notify user about pending transaction
                    state.add_system_message(&format!(
                        "You have a pending transaction: {}. Ready to sign?",
                        pending_tx.user_intent
                    ));
                }
            }
        }
        Ok(())
    }

    /// Set a pending wallet transaction for this session
    pub async fn set_pending_transaction(
        &self,
        session_id: &str,
        chain_id: u32,
        tx_data: serde_json::Value,
        user_intent: String,
    ) -> anyhow::Result<()> {
        if let Some(db) = &self.db_store {
            let now = chrono::Utc::now().timestamp();

            let pending_tx = aomi_tools::db::PendingTransaction {
                created_at: now,
                expires_at: now + 3600, // 1 hour expiry
                chain_id,
                transaction: tx_data,
                user_intent: user_intent.clone(),
                signature: None,
            };

            // Save to database
            db.update_pending_transaction(session_id, Some(pending_tx))
                .await?;

            // Update runtime state
            if let Some(session_data) = self.sessions.get(session_id) {
                let mut state = session_data.state.lock().await;
                state.pending_wallet_tx = Some(user_intent);
            }
        }
        Ok(())
    }

    /// Clear the pending wallet transaction for this session
    pub async fn clear_pending_transaction(&self, session_id: &str) -> anyhow::Result<()> {
        if let Some(db) = &self.db_store {
            db.update_pending_transaction(session_id, None).await?;
        }

        // Update runtime state
        if let Some(session_data) = self.sessions.get(session_id) {
            let mut state = session_data.state.lock().await;
            state.pending_wallet_tx = None;
        }

        Ok(())
    }

    /// Persist non-streaming messages to database
    pub async fn persist_session_messages(
        &self,
        session_id: &str,
        messages: &[ChatMessage],
    ) -> anyhow::Result<()> {
        if let Some(db) = &self.db_store {
            // Get existing message count from database to avoid duplicates
            let existing_messages = db
                .get_messages(session_id, Some("chat"), None)
                .await
                .unwrap_or_default();

            let existing_count = existing_messages.len();

            // Only persist new non-streaming messages
            for message in messages.iter().skip(existing_count) {
                // Skip streaming messages - they're not complete yet
                if message.is_streaming {
                    continue;
                }

                // Convert ChatMessage to DB format
                let sender = match message.sender {
                    MessageSender::User => "user",
                    MessageSender::Assistant => "agent",
                    MessageSender::System => "system",
                };

                let content = serde_json::json!({
                    "text": message.content
                });

                let timestamp = chrono::Utc::now().timestamp();

                let db_message = aomi_tools::db::Message {
                    id: 0, // Will be assigned by database
                    session_id: session_id.to_string(),
                    message_type: "chat".to_string(),
                    sender: sender.to_string(),
                    content,
                    timestamp,
                };

                if let Err(e) = db.save_message(&db_message).await {
                    tracing::warn!("Failed to persist message to DB: {}", e);
                }
            }

            // Update session activity
            let _ = db.update_session_activity(session_id).await;
        }
        Ok(())
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
