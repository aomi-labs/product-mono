use aomi_chat::ChatApp;
use aomi_tools::db::{SessionStore, SessionStoreApi};
use dashmap::DashMap;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::session::{ChatBackend, ChatMessage, DefaultSessionState, MessageSender};
use aomi_chat::ToolResultStream;

struct SessionData {
    state: Arc<Mutex<DefaultSessionState>>,
    last_activity: Instant,
    memory_mode: bool,
}

pub struct SessionManager {
    sessions: Arc<DashMap<String, SessionData>>,
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
            session_public_keys: Arc::new(DashMap::new()),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(1800), // 30 minutes
            chat_backend,
            db_store: None,
        }
    }

    pub async fn set_session_public_key(&self, session_id: &str, public_key: Option<String>) {
        if let Some(pk) = public_key {
            self.session_public_keys.insert(session_id.to_string(), pk);
        }
    }

    async fn get_user_history_with_pubkey(&self, session_id: &str) -> Option<Vec<ChatMessage>> {
        // self.session_public_keys
        // .get(session_id)
        // .and_then(|pk_ref| self.user_history.get(pk_ref.value()).map(|h| h.clone()))
        None
    }

    pub async fn get_or_create_session(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        // // Check in-memory sessions first
        // match self.sessions.get_mut(session_id) {
        //     Some(mut session_data) => {
        //         let state = session_data.state.clone();
        //         session_data.last_activity = Instant::now();

        //         // if let Some(mut user_history) = self.get_user_history_with_pubkey(session_id) {
        //         //     user_history
        //         //         .sync_message_history(last_activity, state.clone())
        //         //         .await;
        //         // }

        //         Ok(state)
        //     }
        //     None => {
        //         let initial_messages = self
        //             .get_user_history_with_pubkey(session_id)
        //             .map(UserHistory::into_messages)
        //             .unwrap_or_default();

        //         // Create new session state
        //         let session_state =
        //             DefaultSessionState::new(Arc::clone(&self.chat_backend), initial_messages)
        //                 .await?;
        //         let session_data = SessionData {
        //             state: Arc::new(Mutex::new(session_state)),
        //             last_activity: Instant::now(),
        //             memory_mode: false,
        //         };
        //         let new_session = session_data.state.clone();
        //         self.sessions.insert(session_id.to_string(), session_data);

        //         println!("📝 Created new session: {}", session_id);
        //         Ok(new_session)
        //     }
        // }

        let session_state =
            DefaultSessionState::new(Arc::clone(&self.chat_backend), vec![]).await?;
        Ok(Arc::new(Mutex::new(session_state)))
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

                // Clean up in-memory sessions
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
        // if let Some(public_key) = public_key {
        //     // Keep in-memory for backward compatibility / fast access
        //     self.user_history.insert(
        //         public_key.clone(),
        //         UserHistory::from_messages_now(messages.to_vec()),
        //     );

        //     // The messages are already being persisted to database via persist_session_messages
        //     // which is called in the same streaming loop, so we don't need to duplicate that here.
        //     // The session is already associated with the public_key via set_session_public_key,
        //     // so we can query user messages across all their sessions using get_user_message_history.
        // }
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
