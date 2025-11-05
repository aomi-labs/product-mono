use anyhow::Result;
use dashmap::DashMap;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    history::UserHistory,
    session::{ChatBackend, ChatMessage, DefaultSessionState},
};
use aomi_chat::ToolResultStream;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BackendType {
    Default,
    L2b,
}

struct SessionData {
    state: Arc<Mutex<DefaultSessionState>>,
    last_activity: Instant,
    backend_kind: BackendType,
}

pub struct SessionManager {
    sessions: Arc<DashMap<String, SessionData>>,
    user_history: Arc<DashMap<String, UserHistory>>,
    session_public_keys: Arc<DashMap<String, String>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    backends: Arc<HashMap<BackendType, Arc<dyn ChatBackend<ToolResultStream>>>>,
}

impl SessionManager {
    pub fn new(
        backends: Arc<HashMap<BackendType, Arc<dyn ChatBackend<ToolResultStream>>>>,
    ) -> Self {
        Self::with_backends(backends)
    }

    pub fn with_backend(chat_backend: Arc<dyn ChatBackend<ToolResultStream>>) -> Self {
        let mut backends: HashMap<BackendType, Arc<dyn ChatBackend<ToolResultStream>>> =
            HashMap::new();
        backends.insert(BackendType::Default, chat_backend);
        Self::with_backends(Arc::new(backends))
    }

    pub fn build_backend_map(
        default_backend: Arc<dyn ChatBackend<ToolResultStream>>,
        l2b_backend: Option<Arc<dyn ChatBackend<ToolResultStream>>>,
    ) -> Arc<HashMap<BackendType, Arc<dyn ChatBackend<ToolResultStream>>>> {
        let mut backends: HashMap<BackendType, Arc<dyn ChatBackend<ToolResultStream>>> =
            HashMap::new();
        backends.insert(BackendType::Default, default_backend);
        if let Some(l2b_backend) = l2b_backend {
            backends.insert(BackendType::L2b, l2b_backend);
        }
        Arc::new(backends)
    }

    fn with_backends(
        backends: Arc<HashMap<BackendType, Arc<dyn ChatBackend<ToolResultStream>>>>,
    ) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            user_history: Arc::new(DashMap::new()),
            session_public_keys: Arc::new(DashMap::new()),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(1800), // 30 minutes
            backends,
        }
    }

    pub async fn replace_backend(
        &self,
        requested_backend: Option<BackendType>,
        state: Arc<Mutex<DefaultSessionState>>,
        current_backend: BackendType,
    ) -> Result<BackendType> {

        let target_backend = requested_backend.unwrap_or(current_backend);
        if target_backend == current_backend {
            return Ok(current_backend);
        }
    
        let backend = Arc::clone(
            self
                .backends
                .get(&target_backend)
                .expect("requested backend not configured"),
        );
    
        let current_messages = {
            let mut guard = state.lock().await;
            guard.get_messages_mut().clone()
        };
    
        let session_state = DefaultSessionState::new(backend, current_messages).await?;
    
        {
            let mut guard = state.lock().await;
            *guard = session_state;
        }
    
        Ok(target_backend)
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
        requested_backend: Option<BackendType>,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        match self.sessions.get_mut(session_id) {
            Some(mut session_data) => {
                let last_activity = session_data.last_activity;
                let new_backend_kind = self.replace_backend(
                    requested_backend,
                    session_data.state.clone(),
                    session_data.backend_kind,
                )
                .await?;
                session_data.backend_kind = new_backend_kind;

                session_data.last_activity = Instant::now();
                let state = session_data.state.clone();
                if let Some(mut user_history) = self.get_user_history_with_pubkey(session_id) {
                    user_history
                        .sync_message_history(last_activity, state.clone())
                        .await;
                }
                Ok(state)
            }
            None => {
                let initial_messages = self
                    .get_user_history_with_pubkey(session_id)
                    .map(UserHistory::into_messages)
                    .unwrap_or_default();
                
                let backend_kind = requested_backend.unwrap_or(BackendType::Default);
                tracing::info!("using {:?} backend", backend_kind);

                let backend = Arc::clone(
                    self.backends
                        .get(&backend_kind)
                        .expect("requested backend not configured"),
                );
                let session_state =
                    DefaultSessionState::new(backend, initial_messages)
                        .await?;
                let session_data = SessionData {
                    state: Arc::new(Mutex::new(session_state)),
                    last_activity: Instant::now(),
                    backend_kind,
                };
                let new_session = session_data.state.clone();
                self.sessions.insert(session_id.to_string(), session_data);
                println!("📝 Created new session: {}", session_id);
                Ok(new_session)
            }
        }
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
                sessions.retain(|session_id, session_status| {
                    let should_keep =
                        now.duration_since(session_status.last_activity) < session_timeout;
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
