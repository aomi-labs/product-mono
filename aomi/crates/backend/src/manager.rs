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
    history::HistoryBackend,
    session::{BackendwithTool, ChatMessage, DefaultSessionState},
};

const SESSION_TIMEOUT: u64 = 3600; // 1 hour

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BackendType {
    Default,
    L2b,
}

struct SessionData {
    state: Arc<Mutex<DefaultSessionState>>,
    last_activity: Instant,
    backend_kind: BackendType,
    memory_mode: bool,
}

pub struct SessionManager {
    sessions: Arc<DashMap<String, SessionData>>,
    session_public_keys: Arc<DashMap<String, String>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    backends: Arc<HashMap<BackendType, Arc<BackendwithTool>>>,
    history_backend: Arc<dyn HistoryBackend>,
}

impl SessionManager {
    pub fn new(
        backends: Arc<HashMap<BackendType, Arc<BackendwithTool>>>,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Self {
        Self::with_backends(backends, history_backend)
    }

    pub fn with_backend(
        chat_backend: Arc<BackendwithTool>,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Self {
        let mut backends: HashMap<BackendType, Arc<BackendwithTool>> = HashMap::new();
        backends.insert(BackendType::Default, chat_backend);
        Self::with_backends(Arc::new(backends), history_backend)
    }

    pub fn build_backend_map(
        default_backend: Arc<BackendwithTool>,
        l2b_backend: Option<Arc<BackendwithTool>>,
    ) -> Arc<HashMap<BackendType, Arc<BackendwithTool>>> {
        let mut backends: HashMap<BackendType, Arc<BackendwithTool>> = HashMap::new();
        backends.insert(BackendType::Default, default_backend);
        if let Some(l2b_backend) = l2b_backend {
            backends.insert(BackendType::L2b, l2b_backend);
        }
        Arc::new(backends)
    }

    fn with_backends(
        backends: Arc<HashMap<BackendType, Arc<BackendwithTool>>>,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            session_public_keys: Arc::new(DashMap::new()),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(SESSION_TIMEOUT),
            backends,
            history_backend,
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
            self.backends
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

    pub async fn set_session_public_key(&self, session_id: &str, public_key: Option<String>) {
        if let Some(pk) = public_key {
            self.session_public_keys
                .insert(session_id.to_string(), pk.clone());

            // Create session in database when pubkey is first associated and load historical messages
            // This handles the case where session was created without a pubkey
            match self
                .history_backend
                .get_or_create_history(Some(pk), session_id.to_string())
                .await
            {
                Ok(historical_summary) => {
                    if let Some(session_data) = self.sessions.get(session_id) {
                        let session = session_data.state.lock().await;

                        if let Some(summary) = historical_summary {
                            tracing::info!(
                                "Historical context loaded for session {}, triggering greeting",
                                session_id
                            );

                            // Trigger the auto-greeting with historical context
                            if let Err(e) = session.sender_to_llm.send(summary.content).await {
                                tracing::error!("Failed to send auto-greeting: {}", e);
                            }
                        } 
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create session in DB when associating pubkey: {}",
                        e
                    );
                }
            }
        }
    }

    pub async fn get_or_create_session(
        &self,
        session_id: &str,
        requested_backend: Option<BackendType>,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        // Check if session exists
        match self.sessions.get_mut(session_id) {
            Some(mut session_data) => {
                // Handle backend switching if requested
                let new_backend_kind = self
                    .replace_backend(
                        requested_backend,
                        session_data.state.clone(),
                        session_data.backend_kind,
                    )
                    .await?;
                session_data.backend_kind = new_backend_kind;
                session_data.last_activity = Instant::now();

                Ok(session_data.state.clone())
            }
            None => {
                // Get pubkey for this session if available
                let pubkey = self
                    .session_public_keys
                    .get(session_id)
                    .map(|pk| pk.value().clone());

                let mut historical_messages = Vec::new();

                // Load historical messages and ensure DB session exists (if pubkey is present)
                if let Some(msg ) =  self
                    .history_backend
                    .get_or_create_history(pubkey, session_id.to_string())
                    .await? {
                        historical_messages.push(msg);
                }

                let backend_kind = requested_backend.unwrap_or(BackendType::Default);
                tracing::info!("using {:?} backend", backend_kind);

                let backend = Arc::clone(
                    self.backends
                        .get(&backend_kind)
                        .expect("requested backend not configured"),
                );

                // Create new session state with historical messages for LLM context
                let session_state = DefaultSessionState::new(backend, historical_messages).await?;

                let session_data = SessionData {
                    state: Arc::new(Mutex::new(session_state)),
                    last_activity: Instant::now(),
                    backend_kind,
                    memory_mode: false,
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
        let session_public_keys = cleanup_manager.session_public_keys.clone();
        let history_backend = Arc::clone(&cleanup_manager.history_backend);

        tokio::spawn(async move {
            loop {
                interval.tick().await;
                let now = Instant::now();

                // Collect sessions to remove (with memory_mode flag)
                let mut sessions_to_cleanup: Vec<(String, bool)> = Vec::new();

                // Clean up in-memory sessions
                sessions.retain(|session_id, session_data| {
                    let should_keep =
                        now.duration_since(session_data.last_activity) < session_timeout;
                    if !should_keep {
                        sessions_to_cleanup.push((session_id.clone(), session_data.memory_mode));
                    }
                    should_keep
                });

                // Flush history for cleaned up sessions (unless in memory-only mode)
                for (session_id, memory_mode) in sessions_to_cleanup {
                    let pubkey = session_public_keys
                        .get(&session_id)
                        .map(|pk| pk.value().clone());

                    // Only persist to database if not in memory-only mode
                    if !memory_mode {
                        if let Err(e) = history_backend
                            .flush_history(pubkey.clone(), session_id.clone())
                            .await
                        {
                            eprintln!(
                                "❌ Failed to flush history for session {}: {}",
                                session_id, e
                            );
                        } else {
                            println!("🗑️ Cleaned up inactive session: {}", session_id);
                        }
                    } else {
                        println!(
                            "🗑️ Cleaned up inactive session (memory-only): {}",
                            session_id
                        );
                    }

                    // Clean up public key mapping
                    if pubkey.is_some() {
                        session_public_keys.remove(&session_id);
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
        session_id: &str,
        _public_key: Option<String>,
        messages: &[ChatMessage],
    ) {
        // Update in-memory history (called periodically from SSE stream)
        self.history_backend.update_history(session_id, messages);
    }

    /// Sets memory-only mode for a session.
    /// When enabled, the session's history will not be persisted to database on cleanup.
    pub async fn set_memory_mode(&self, session_id: &str, memory_mode: bool) {
        if let Some(mut session_data) = self.sessions.get_mut(session_id) {
            session_data.memory_mode = memory_mode;
            println!(
                "🔄 Session {} memory mode: {}",
                session_id,
                if memory_mode { "enabled" } else { "disabled" }
            );
        }
    }

    /// Gets whether a session is in memory-only mode.
    pub async fn get_memory_mode(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .map(|session_data| session_data.memory_mode)
            .unwrap_or(false)
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
