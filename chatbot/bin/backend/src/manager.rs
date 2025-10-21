use aomi_agent::ChatApp;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::session::{ChatBackend, ChatMessage, SessionState};

struct SessionData {
    state: Arc<Mutex<SessionState>>,
    last_activity: Instant,
}

// TODO: use this with user's wallet address as key, make it persistent with a database
// for now just use a session id as key
#[derive(Clone)]
pub struct UserHistory {
    pub messages: Vec<ChatMessage>,
    pub last_activity: Instant,
}

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    user_history: Arc<RwLock<HashMap<String, UserHistory>>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    chat_backend: Arc<dyn ChatBackend>,
}

impl SessionManager {
    pub fn new(chat_app: Arc<ChatApp>) -> Self {
        Self::with_backend(chat_app)
    }

    pub fn with_backend(chat_backend: Arc<dyn ChatBackend>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            user_history: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(1800), // 30 minutes
            chat_backend,
        }
    }

    pub async fn get_or_create_history(&self, public_key: &Option<String>) -> Option<UserHistory> {
        match public_key {
            Some(public_key) => {
                let histories = self.user_history.read().await;
                if let Some(user_history) = histories.get(public_key.as_str()) {
                    Some(user_history.clone())
                } else {
                    Some(UserHistory {
                        messages: Vec::new(),
                        last_activity: Instant::now(),
                    })
                }
            }
            None => None,
        }
    }

    pub async fn get_or_create_session(
        &self,
        session_id: &str,
        user_history: Option<UserHistory>,
    ) -> anyhow::Result<Arc<Mutex<SessionState>>> {
        let mut sessions = self.sessions.write().await;

        if let Some(session_data) = sessions.get_mut(session_id) {
            let old_activity = session_data.last_activity;
            session_data.last_activity = Instant::now();

            if let Some(user_history) = user_history {
                let replacement = {
                    let mut state = session_data.state.lock().await;
                    let incoming_conversation =
                        SessionState::filter_system_messages(&user_history.messages);
                    let current_conversation = SessionState::filter_system_messages(&state.messages);
                    let has_more_messages =
                        incoming_conversation.len() > current_conversation.len();
                    let has_different_messages = incoming_conversation != current_conversation;
                    let has_newer_activity = user_history.last_activity >= old_activity;

                    if has_more_messages || has_different_messages || has_newer_activity {
                        let new_messages = user_history.messages.clone();
                        state.messages = new_messages.clone();
                        state.sync_welcome_flag();
                        let agent_handle = state.agent_history_handle();
                        drop(state);
                        Some((agent_handle, new_messages))
                    } else {
                        None
                    }
                };

                if let Some((agent_history, new_messages)) = replacement {
                    let agent_messages = SessionState::to_rig_messages(&new_messages);
                    let mut agent_history_guard = agent_history.write().await;
                    *agent_history_guard = agent_messages;
                }
            }

            Ok(session_data.state.clone())
        } else {
            let session_history = user_history.map(|h| h.messages).unwrap_or_default();
            let session_state =
                SessionState::new(Arc::clone(&self.chat_backend), session_history).await?;
            let session_data = SessionData {
                state: Arc::new(Mutex::new(session_state)),
                last_activity: Instant::now(),
            };
            let new_session = session_data.state.clone();
            sessions.insert(session_id.to_string(), session_data);
            println!("📝 Created new session: {}", session_id);
            Ok(new_session)
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

    #[allow(dead_code)]
    pub async fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_some() {
            println!("🗑️ Manually removed session: {}", session_id);
        }
    }

    pub fn start_cleanup_task(self: Arc<Self>) {
        let cleanup_manager = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_manager.cleanup_interval);
            loop {
                interval.tick().await;
                cleanup_manager.cleanup_inactive_sessions().await;
            }
        });
    }

    #[allow(dead_code)]
    pub async fn get_active_session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    pub async fn update_user_history(
        &self,
        _session_id: &str,
        public_key: Option<String>,
        messages: &[ChatMessage],
    ) {
        if let Some(public_key) = public_key {
            let mut histories = self.user_history.write().await;
            histories.insert(
                public_key,
                UserHistory {
                    messages: messages.to_vec(),
                    last_activity: Instant::now(),
                },
            );
        }
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
