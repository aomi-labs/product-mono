use anyhow::Result;
use dashmap::DashMap;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    history::{HistoryBackend, DEFAULT_TITLE},
    namespace::Namespace,
    types::{AomiBackend, ChatMessage, DefaultSessionState, SessionRecord},
};
use serde_json::Value;

const SESSION_TIMEOUT: u64 = 60; // 1 hour
const SESSION_LIST_LIMIT: usize = i32::MAX as usize;

/// Metadata about a session (managed by SessionManager, not SessionState)
#[derive(Clone)]
pub struct SessionMetadata {
    pub title: String,
    pub title_renewal_stamp: usize,
    pub db_hydrated_cnt: usize,
    pub is_archived: bool,
    pub memory_mode: bool,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            title: DEFAULT_TITLE.to_string(),
            title_renewal_stamp: 0,
            db_hydrated_cnt: 0,
            is_archived: false,
            memory_mode: false,
        }
    }
}

pub(crate) struct SessionData {
    pub(crate) state: Arc<Mutex<DefaultSessionState>>,
    pub(crate) last_activity: Instant,
    pub(crate) namespace: Namespace,
    pub(crate) metadata: SessionMetadata,
}

pub struct SessionManager {
    pub(crate) sessions: Arc<DashMap<String, SessionData>>,
    pub(crate) session_public_keys: Arc<DashMap<String, String>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    backends: Arc<HashMap<Namespace, Arc<AomiBackend>>>,
    pub(crate) history_backend: Arc<dyn HistoryBackend>,
    pub(crate) system_update_tx: broadcast::Sender<(String, Value)>,
}

impl SessionManager {
    pub fn new(
        backends: Arc<HashMap<Namespace, Arc<AomiBackend>>>,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Self {
        let (system_update_tx, _system_update_rx) = broadcast::channel::<(String, Value)>(64);
        Self {
            sessions: Arc::new(DashMap::new()),
            session_public_keys: Arc::new(DashMap::new()),
            cleanup_interval: Duration::from_mins(5),
            session_timeout: Duration::from_mins(SESSION_TIMEOUT),
            backends,
            history_backend,
            system_update_tx,
        }
    }

    pub fn with_backend(
        chat_backend: Arc<AomiBackend>,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Self {
        let mut backends: HashMap<Namespace, Arc<AomiBackend>> = HashMap::new();
        backends.insert(Namespace::Default, chat_backend);
        Self::new(Arc::new(backends), history_backend)
    }

    pub fn build_backend_map(
        default_backend: Arc<AomiBackend>,
        l2b_backend: Option<Arc<AomiBackend>>,
        forge_backend: Option<Arc<AomiBackend>>,
        polymarket_backend: Option<Arc<AomiBackend>>,
    ) -> Arc<HashMap<Namespace, Arc<AomiBackend>>> {
        let mut backends: HashMap<Namespace, Arc<AomiBackend>> = HashMap::new();
        backends.insert(Namespace::Default, default_backend);
        if let Some(l2b_backend) = l2b_backend {
            backends.insert(Namespace::L2b, l2b_backend);
        }
        if let Some(forge_backend) = forge_backend {
            backends.insert(Namespace::Forge, forge_backend);
        }
        if let Some(polymarket_backend) = polymarket_backend {
            backends.insert(Namespace::Polymarket, polymarket_backend);
        }
        Arc::new(backends)
    }

    /// Initialize all backends and create a SessionManager
    pub async fn initialize(
        skip_docs: bool,
        skip_mcp: bool,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Result<Self> {
        tracing::info!("Initializing backends...");

        let selection = aomi_baml::Selection {
            rig: aomi_baml::AomiModel::ClaudeSonnet4,
            baml: aomi_baml::AomiModel::ClaudeOpus4,
        };
        let backends = crate::namespace::build_backends(vec![
            (
                Namespace::Default,
                crate::namespace::BuildOpts {
                    no_docs: skip_docs,
                    skip_mcp,
                    no_tools: false,
                    selection,
                },
            ),
            (
                Namespace::L2b,
                crate::namespace::BuildOpts {
                    no_docs: skip_docs,
                    skip_mcp,
                    no_tools: false,
                    selection,
                },
            ),
            (
                Namespace::Forge,
                crate::namespace::BuildOpts {
                    no_docs: skip_docs,
                    skip_mcp,
                    no_tools: false,
                    selection,
                },
            ),
        ])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize backends: {}", e))?;

        tracing::info!("All backends initialized successfully");

        Ok(Self::new(Arc::new(backends), history_backend))
    }

    pub async fn replace_backend(
        &self,
        requested_backend: Option<Namespace>,
        state: Arc<Mutex<DefaultSessionState>>,
        current_backend: Namespace,
    ) -> Result<Namespace> {
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
            let guard = state.lock().await;
            guard.messages.clone()
        };

        let session_state = DefaultSessionState::new(backend, current_messages).await?;

        {
            let mut guard = state.lock().await;
            *guard = session_state;
        }

        Ok(target_backend)
    }

    /// Sets or unsets the archived flag on a session.
    pub fn set_session_archived(&self, session_id: &str, archived: bool) {
        if let Some(mut session_data) = self.sessions.get_mut(session_id) {
            session_data.metadata.is_archived = archived;
        }
    }

    /// Check if a session is archived
    pub fn is_session_archived(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .map(|s| s.metadata.is_archived)
            .unwrap_or(false)
    }

    /// Deletes a session from memory and clears its public key mapping.
    pub async fn delete_session(&self, session_id: &str) {
        if self.sessions.remove(session_id).is_some() {
            debug!(session_id, "Deleted session");
        }
        self.session_public_keys.remove(session_id);
    }

    /// Subscribe to system-wide updates (title changes, etc.)
    pub fn subscribe_to_updates(&self) -> tokio::sync::broadcast::Receiver<(String, Value)> {
        self.system_update_tx.subscribe()
    }

    /// Updates the title of a session in memory and persists to storage
    pub async fn update_session_title(
        &self,
        session_id: &str,
        title: String,
    ) -> anyhow::Result<()> {
        if let Some(mut session_data) = self.sessions.get_mut(session_id) {
            session_data.metadata.title = title.clone();
            debug!(session_id, title, "Updated session title");
            drop(session_data);

            if self.session_public_keys.get(session_id).is_some() {
                self.history_backend
                    .update_session_title(session_id, &title)
                    .await?;
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found: {}", session_id))
        }
    }

    pub async fn set_session_public_key(&self, session_id: &str, public_key: Option<String>) {
        if let Some(pk) = public_key {
            self.session_public_keys
                .insert(session_id.to_string(), pk.clone());

            debug!(session_id, public_key = %pk, "Set public key for session");

            if let Err(e) = self
                .history_backend
                .get_or_create_history(&pk, session_id)
                .await
            {
                error!(session_id, error = %e, "Failed to create session in DB when associating pubkey");
            }
        }
    }

    pub fn get_public_key(&self, session_id: &str) -> Option<String> {
        self.session_public_keys
            .get(session_id)
            .map(|pk| pk.value().clone())
    }

    /// Get a session only if it exists in memory. Does NOT recreate deleted sessions.
    pub fn get_session_if_exists(
        &self,
        session_id: &str,
    ) -> Option<Arc<Mutex<DefaultSessionState>>> {
        self.sessions
            .get(session_id)
            .map(|entry| entry.state.clone())
    }

    /// Get the title for a session
    pub fn get_session_title(&self, session_id: &str) -> Option<String> {
        self.sessions
            .get(session_id)
            .map(|entry| entry.metadata.title.clone())
    }

    /// Get session metadata
    pub fn get_session_metadata(&self, session_id: &str) -> Option<SessionMetadata> {
        self.sessions
            .get(session_id)
            .map(|entry| entry.metadata.clone())
    }

    /// Create session data and insert into the sessions map
    async fn create_session(
        &self,
        session_id: &str,
        namespace: Namespace,
        messages: Vec<ChatMessage>,
        metadata: SessionMetadata,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        let backend = Arc::clone(
            self.backends
                .get(&namespace)
                .expect("requested backend not configured"),
        );

        let session_state = DefaultSessionState::new(backend, messages).await?;

        let session_data = SessionData {
            state: Arc::new(Mutex::new(session_state)),
            last_activity: Instant::now(),
            namespace,
            metadata,
        };

        let new_session = session_data.state.clone();
        self.sessions.insert(session_id.to_string(), session_data);
        Ok(new_session)
    }

    /// Get or create a session. Checks memory first, then DB, then creates new.
    pub async fn get_or_create_session(
        &self,
        session_id: &str,
        requested_backend: Option<Namespace>,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        // 1. Check if session exists in memory
        if let Some(session_data_ref) = self.sessions.get(session_id) {
            let state = session_data_ref.state.clone();
            let namespace = session_data_ref.namespace;
            drop(session_data_ref);

            // Handle backend switching if requested
            let new_namespace = self
                .replace_backend(requested_backend, state.clone(), namespace)
                .await?;

            if let Some(mut session_data) = self.sessions.get_mut(session_id) {
                session_data.namespace = new_namespace;
                session_data.last_activity = Instant::now();
            }

            return Ok(state);
        }

        // 2. Try to load from DB
        if let Some(stored) = self
            .history_backend
            .get_session(session_id)
            .await?
        {
            // Restore public key mapping if available
            if let Some(pk) = stored.public_key.clone() {
                if self.session_public_keys.get(session_id).is_none() {
                    self.session_public_keys.insert(session_id.to_string(), pk);
                }
            }

            let namespace = requested_backend.unwrap_or(Namespace::Default);
            // Always start with "New Chat" - title will be regenerated from messages
            let metadata = SessionMetadata {
                title: DEFAULT_TITLE.to_string(),
                title_renewal_stamp: 0, // Force title regeneration
                db_hydrated_cnt: stored.messages.len(),
                is_archived: false,
                memory_mode: false,
            };

            let state = self
                .create_session(session_id, namespace, stored.messages, metadata)
                .await?;

            debug!(session_id, "Rehydrated session from storage");
            return Ok(state);
        }

        // 3. Create new session
        let pubkey = self
            .session_public_keys
            .get(session_id)
            .map(|pk| pk.value().clone());

        // Ensure DB session exists when creating a new in-memory session (only if pubkey exists)
        if let Some(pk) = &pubkey {
            let _ = self
                .history_backend
                .get_or_create_history(pk, session_id)
                .await?;
        }

        let namespace = requested_backend.unwrap_or(Namespace::Default);
        let metadata = SessionMetadata::default();

        let new_session = self
            .create_session(session_id, namespace, Vec::new(), metadata)
            .await?;

        debug!(session_id, "Created new session");
        Ok(new_session)
    }

    #[allow(dead_code)]
    pub async fn remove_session(&self, session_id: &str) {
        if self.sessions.remove(session_id).is_some() {
            debug!(session_id, "Manually removed session");
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

                // Step 1: Identify expired sessions (don't remove yet)
                let sessions_to_cleanup: Vec<(String, bool)> = sessions
                    .iter()
                    .filter_map(|entry| {
                        let should_cleanup =
                            now.duration_since(entry.value().last_activity) >= session_timeout;
                        if should_cleanup {
                            Some((entry.key().clone(), entry.value().metadata.memory_mode))
                        } else {
                            None
                        }
                    })
                    .collect();

                // Step 2: Flush history BEFORE removing from memory
                // This prevents race condition where new session loads stale data from DB
                // Track which sessions successfully flushed (or don't need flushing)
                let mut successfully_flushed: Vec<String> = Vec::new();

                for (session_id, memory_mode) in &sessions_to_cleanup {
                    let pubkey = session_public_keys
                        .get(session_id)
                        .map(|pk| pk.value().clone());

                    // Memory-only sessions don't need flushing
                    if *memory_mode {
                        successfully_flushed.push(session_id.clone());
                        continue;
                    }

                    // Anonymous sessions (no pubkey) can't be flushed
                    let Some(pk) = pubkey else {
                        successfully_flushed.push(session_id.clone());
                        continue;
                    };

                    // Try to flush - only mark successful if flush succeeds
                    match history_backend.flush_history(&pk, session_id).await {
                        Ok(()) => {
                            successfully_flushed.push(session_id.clone());
                        }
                        Err(e) => {
                            // Keep session in memory for retry on next cleanup cycle
                            error!(session_id, error = %e, "Failed to flush history, will retry");
                        }
                    }
                }

                // Step 3: Only remove sessions that were successfully flushed
                for session_id in successfully_flushed {
                    sessions.remove(&session_id);
                    debug!(session_id, "Cleaned up inactive session");

                    if session_public_keys.get(&session_id).is_some() {
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

    pub async fn update_user_history(&self, session_id: &str, messages: &[ChatMessage]) {
        let db_hydrated_cnt = self
            .sessions
            .get(session_id)
            .map(|session_data| session_data.metadata.db_hydrated_cnt)
            .unwrap_or(0);

        if messages.len() <= db_hydrated_cnt {
            return;
        }

        let new_messages = messages[db_hydrated_cnt..].to_vec();
        tracing::debug!(
            "Updating user history for session {}: {:?}",
            session_id,
            new_messages
        );

        self.history_backend
            .update_history(session_id, &new_messages);
        let _ = self
            .history_backend
            .set_messages_persisted(session_id, false)
            .await;
    }

    /// Sets memory-only mode for a session.
    pub async fn set_memory_mode(&self, session_id: &str, memory_mode: bool) {
        if let Some(mut session_data) = self.sessions.get_mut(session_id) {
            session_data.metadata.memory_mode = memory_mode;
            debug!(session_id, memory_mode, "Session memory mode changed");
        }
    }

    /// Gets whether a session is in memory-only mode.
    pub async fn get_memory_mode(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .map(|session_data| session_data.metadata.memory_mode)
            .unwrap_or(false)
    }

    pub async fn list_sessions(
        &self,
        public_key: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<SessionRecord>> {
        self.history_backend
            .list_sessions(public_key, limit.min(SESSION_LIST_LIMIT))
            .await
    }

    pub fn get_history_backend(&self) -> Arc<dyn HistoryBackend> {
        Arc::clone(&self.history_backend)
    }

    /// Delete all sessions completely (for testing cleanup)
    pub async fn cleanup_all_sessions(&self) {
        let session_ids: Vec<String> = self
            .sessions
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        for session_id in session_ids.iter() {
            self.sessions.remove(session_id);
        }

        self.session_public_keys.clear();

        for session_id in session_ids {
            let _ = self.history_backend.delete_session(&session_id).await;
        }
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
