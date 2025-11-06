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

    pub fn set_session_public_key(&self, session_id: &str, public_key: Option<String>) {
        if let Some(pk) = public_key {
            self.session_public_keys.insert(session_id.to_string(), pk);
        }
    }

    fn get_user_history_with_pubkey(&self, session_id: &str) -> Option<UserHistory> {
        self.session_public_keys
            .get(session_id)
            .and_then(|pk_ref| self.user_history.get(pk_ref.value()).map(|h| h.clone()))
    }

    pub async fn get_or_create_session(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        // Check in-memory sessions first
        match self.sessions.get_mut(session_id) {
            Some(mut session_data) => {
                let state = session_data.state.clone();
                let last_activity = session_data.last_activity;
                session_data.last_activity = Instant::now();

                // Update activity in DB if available
                if let Some(db) = &self.db_store {
                    let _ = db.update_session_activity(session_id).await;
                }

                if let Some(mut user_history) = self.get_user_history_with_pubkey(session_id) {
                    user_history
                        .sync_message_history(last_activity, state.clone())
                        .await;
                }
                Ok(state)
            }
            None => {
                // Try to load from database
                let initial_messages = if let Some(db) = &self.db_store {
                    match db.get_session(session_id).await {
                        Ok(Some(_db_session)) => {
                            println!("🔄 Restoring session from database: {}", session_id);

                            // Load chat messages from DB
                            let db_messages = db
                                .get_messages(session_id, Some("chat"), None)
                                .await
                                .unwrap_or_default();

                            // Convert DB messages to ChatMessages
                            self.db_messages_to_chat_messages(&db_messages)
                        }
                        Ok(None) => {
                            // Session not in DB, check user history
                            self.get_user_history_with_pubkey(session_id)
                                .map(UserHistory::into_messages)
                                .unwrap_or_default()
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load session from DB: {}", e);
                            self.get_user_history_with_pubkey(session_id)
                                .map(UserHistory::into_messages)
                                .unwrap_or_default()
                        }
                    }
                } else {
                    // No DB, use in-memory user history
                    self.get_user_history_with_pubkey(session_id)
                        .map(UserHistory::into_messages)
                        .unwrap_or_default()
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

                // Persist new session to DB
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
                        tracing::warn!("Failed to persist session to DB: {}", e);
                    }
                }

                println!("📝 Created new session: {}", session_id);
                Ok(new_session)
            }
        }
    }

    fn db_messages_to_chat_messages(&self, db_messages: &[aomi_tools::db::Message]) -> Vec<ChatMessage> {
        db_messages
            .iter()
            .filter_map(|msg| {
                let sender = match msg.sender.as_str() {
                    "user" => MessageSender::User,
                    "agent" => MessageSender::Assistant,
                    "system" => MessageSender::System,
                    _ => return None,
                };

                let content = msg.content
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
        tokio::spawn(async move {
            loop {
                interval.tick().await;
                let now = Instant::now();
                sessions.retain(|session_id, session_data| {
                    let should_keep =
                        now.duration_since(session_data.last_activity) < session_timeout;
                    if !should_keep {
                        println!("🗑️ Cleaning up inactive session: {}", session_id);
                    }
                    should_keep
                });
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
            self.user_history.insert(
                public_key,
                UserHistory::from_messages_now(messages.to_vec()),
            );
        }
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
