use anyhow::Result;
use aomi_chat::CoreApp;
use aomi_forge::ForgeApp;
use aomi_l2beat::L2BeatApp;
use dashmap::DashMap;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::{
    history::HistoryBackend,
    types::{AomiBackend, ChatMessage, DefaultSessionState, HistorySession},
};
use serde_json::Value;

const SESSION_TIMEOUT: u64 = 60; // 1 hour
const SESSION_LIST_LIMIT: usize = i32::MAX as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Namespace {
    Default,
    L2b,
    Forge,
    Test,
}
// Han does (api_key -> [L2b, Forge])
// DB -> schema [api_key, allowed_namespaces, company_name, allowed_users]

/// Metadata about a session (managed by SessionManager, not SessionState)
#[derive(Clone)]
pub struct SessionMetadata {
    pub title: Option<String>,
    pub is_archived: bool,
    pub is_user_title: bool,
    pub last_gen_title_msg: usize,
    pub history_sessions: Vec<HistorySession>,
}

pub(crate) struct SessionData {
    pub(crate) state: Arc<Mutex<DefaultSessionState>>,
    pub(crate) last_activity: Instant,
    pub(crate) backend_kind: Namespace,
    pub(crate) memory_mode: bool,
    pub(crate) persisted_message_count: usize,
    // Metadata fields (not chat-stream related)
    pub(crate) title: Option<String>,
    pub(crate) is_user_title: bool,
    pub(crate) history_sessions: Vec<HistorySession>,
    pub(crate) is_archived: bool,
    pub(crate) last_gen_title_msg: usize,
}

struct SessionInsertMetadata {
    title: Option<String>,
    history_sessions: Vec<HistorySession>,
    is_user_title: bool,
    persisted_message_count: usize,
    last_gen_title_msg: usize,
}

pub struct SessionManager {
    pub(crate) sessions: Arc<DashMap<String, SessionData>>,
    pub(crate) session_public_keys: Arc<DashMap<String, String>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    backends: Arc<HashMap<Namespace, Arc<AomiBackend>>>,
    pub(crate) history_backend: Arc<dyn HistoryBackend>,
    pub(crate) system_update_tx: broadcast::Sender<Value>,
}

impl SessionManager {
    pub fn new(
        backends: Arc<HashMap<Namespace, Arc<AomiBackend>>>,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Self {
        let (system_update_tx, _system_update_rx) = broadcast::channel::<Value>(64);
        // NOTE: _system_update_rx is intentionally dropped here.
        // The broadcast channel works with only senders - receivers are created via subscribe().
        // Watch for:
        // - If buffer fills (64 messages) with no subscribers, oldest messages are dropped (expected)
        // - If send() is called with no subscribers, it returns Err (we ignore with `let _ = ...`)
        // Memory leaks are not a concern since the channel is bounded.
        Self {
            sessions: Arc::new(DashMap::new()),
            session_public_keys: Arc::new(DashMap::new()),
            cleanup_interval: Duration::from_secs(30), // 5 minutes
            session_timeout: Duration::from_secs(SESSION_TIMEOUT),
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
    ) -> Arc<HashMap<Namespace, Arc<AomiBackend>>> {
        let mut backends: HashMap<Namespace, Arc<AomiBackend>> = HashMap::new();
        backends.insert(Namespace::Default, default_backend);
        if let Some(l2b_backend) = l2b_backend {
            backends.insert(Namespace::L2b, l2b_backend);
        }
        if let Some(forge_backend) = forge_backend {
            backends.insert(Namespace::Forge, forge_backend);
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

        // Initialize ChatApp
        tracing::info!("Initializing ChatApp...");
        let chat_app = Arc::new(
            CoreApp::new_with_options(skip_docs, skip_mcp)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize ChatApp: {}", e))?,
        );
        let chat_backend: Arc<AomiBackend> = chat_app;

        // Initialize L2BeatApp
        tracing::info!("Initializing L2BeatApp...");
        let l2b_app = Arc::new(
            L2BeatApp::new(skip_docs, skip_mcp)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize L2BeatApp: {}", e))?,
        );
        let l2b_backend: Arc<AomiBackend> = l2b_app;

        // Initialize ForgeApp
        tracing::info!("Initializing ForgeApp...");
        let forge_app = Arc::new(
            ForgeApp::new(skip_docs, skip_mcp)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize ForgeApp: {}", e))?,
        );
        let forge_backend: Arc<AomiBackend> = forge_app;

        // Build backend map
        let backends =
            Self::build_backend_map(chat_backend, Some(l2b_backend), Some(forge_backend));

        tracing::info!("All backends initialized successfully");

        Ok(Self::new(backends, history_backend))
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

        // Only need messages from SessionState now - metadata is in SessionData
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
            session_data.is_archived = archived;
        }
    }

    /// Deletes a session from memory and clears its public key mapping.
    /// Persistent history is still flushed via the cleanup task when needed.
    pub async fn delete_session(&self, session_id: &str) {
        if self.sessions.remove(session_id).is_some() {
            println!("🗑️ Deleted session: {}", session_id);
        }
        // Clean up public key mapping if present
        self.session_public_keys.remove(session_id);
    }

    /// Subscribe to system-wide updates (title changes, etc.)
    pub fn subscribe_to_updates(&self) -> tokio::sync::broadcast::Receiver<Value> {
        self.system_update_tx.subscribe()
    }

    /// Updates the title of a session in memory and persists to storage
    /// This is called when a user manually renames a session, so it sets is_user_title = true
    pub async fn update_session_title(
        &self,
        session_id: &str,
        title: String,
    ) -> anyhow::Result<()> {
        if let Some(mut session_data) = self.sessions.get_mut(session_id) {
            session_data.title = Some(title.clone());
            session_data.is_user_title = true; // User manually set this title
            tracing::info!("Updated title for session {} - {}", session_id, title);
            drop(session_data);

            // Persist title to database (only for sessions with pubkey)
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

            tracing::info!("Set public key for session {}: {}", session_id, pk);

            // Ensure the session/user exists in persistent storage when a pubkey is attached
            // (session might have been created before the wallet connected)
            let current_title = self.get_session_title(session_id);
            if let Err(e) = self
                .history_backend
                .get_or_create_history(Some(pk), session_id.to_string(), current_title)
                .await
            {
                tracing::error!(
                    "Failed to create session in DB when associating pubkey: {}",
                    e
                );
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
            .and_then(|entry| entry.title.clone())
    }

    /// Get session metadata (title, is_archived, last_gen_title_msg, history_sessions)
    pub fn get_session_metadata(&self, session_id: &str) -> Option<SessionMetadata> {
        self.sessions.get(session_id).map(|entry| SessionMetadata {
            title: entry.title.clone(),
            is_archived: entry.is_archived,
            is_user_title: entry.is_user_title,
            last_gen_title_msg: entry.last_gen_title_msg,
            history_sessions: entry.history_sessions.clone(),
        })
    }

    async fn load_history_sessions(&self, pubkey: Option<String>) -> Vec<HistorySession> {
        let Some(pk) = pubkey else {
            return Vec::new();
        };

        match self
            .history_backend
            .get_history_sessions(&pk, SESSION_LIST_LIMIT)
            .await
        {
            Ok(sessions) => sessions,
            Err(e) => {
                tracing::error!("Failed to load history sessions for {}: {}", pk, e);
                Vec::new()
            }
        }
    }

    fn is_user_title(title: &Option<String>) -> bool {
        title
            .as_ref()
            .map(|t| !t.starts_with("#["))
            .unwrap_or(false)
    }

    async fn insert_session_data(
        &self,
        session_id: &str,
        backend_kind: Namespace,
        messages: Vec<ChatMessage>,
        metadata: SessionInsertMetadata,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        let backend = Arc::clone(
            self.backends
                .get(&backend_kind)
                .expect("requested backend not configured"),
        );

        let session_state = DefaultSessionState::new(backend, messages).await?;

        let session_data = SessionData {
            state: Arc::new(Mutex::new(session_state)),
            last_activity: Instant::now(),
            backend_kind,
            memory_mode: false,
            persisted_message_count: metadata.persisted_message_count,
            title: metadata.title,
            is_user_title: metadata.is_user_title,
            history_sessions: metadata.history_sessions,
            is_archived: false,
            last_gen_title_msg: metadata.last_gen_title_msg,
        };

        let new_session = session_data.state.clone();
        self.sessions.insert(session_id.to_string(), session_data);
        Ok(new_session)
    }

    pub async fn get_or_create_session(
        &self,
        session_id: &str,
        requested_backend: Option<Namespace>,
        initial_title: Option<String>,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        let pubkey = self
            .session_public_keys
            .get(session_id)
            .map(|pk| pk.value().clone());

        let history_sessions = self.load_history_sessions(pubkey.clone()).await;

        // Check if session exists
        match self.sessions.get(session_id) {
            Some(session_data_ref) => {
                let state = session_data_ref.state.clone();
                let backend_kind = session_data_ref.backend_kind;
                drop(session_data_ref);

                // Handle backend switching if requested
                let new_backend_kind = self
                    .replace_backend(requested_backend, state.clone(), backend_kind)
                    .await?;

                if let Some(mut session_data) = self.sessions.get_mut(session_id) {
                    session_data.backend_kind = new_backend_kind;
                    session_data.last_activity = Instant::now();
                    session_data.history_sessions = history_sessions;
                    Ok(session_data.state.clone())
                } else {
                    Ok(state)
                }
            }
            None => {
                // Get pubkey for this session if available
                let historical_messages = Vec::new();

                // Ensure DB session exists when creating a new in-memory session (if pubkey is present)
                // Pass initial_title to persist when creating new session in DB
                let _ = self
                    .history_backend
                    .get_or_create_history(
                        pubkey.clone(),
                        session_id.to_string(),
                        initial_title.clone(),
                    )
                    .await?;

                let backend_kind = requested_backend.unwrap_or(Namespace::Default);
                tracing::info!("using {:?} backend", backend_kind);

                let is_user_title = Self::is_user_title(&initial_title);
                let metadata = SessionInsertMetadata {
                    title: initial_title,
                    history_sessions,
                    is_user_title,
                    persisted_message_count: 0,
                    last_gen_title_msg: 0,
                };
                let new_session = self
                    .insert_session_data(session_id, backend_kind, historical_messages, metadata)
                    .await?;

                println!("📝 Created new session: {}", session_id);
                Ok(new_session)
            }
        }
    }

    pub async fn get_or_rehydrate_session(
        &self,
        session_id: &str,
        requested_backend: Option<Namespace>,
    ) -> anyhow::Result<(Option<Arc<Mutex<DefaultSessionState>>>, bool)> {
        if self.get_session_if_exists(session_id).is_some() {
            let state = self
                .get_or_create_session(session_id, requested_backend, None)
                .await?;
            return Ok((Some(state), false));
        }

        let stored = self
            .history_backend
            .get_session_from_storage(session_id)
            .await?;
        let Some(stored) = stored else {
            return Ok((None, false));
        };

        if self.get_session_if_exists(session_id).is_some() {
            let state = self
                .get_or_create_session(session_id, requested_backend, None)
                .await?;
            return Ok((Some(state), false));
        }

        if let Some(pk) = stored.public_key.clone() {
            if self.session_public_keys.get(session_id).is_none() {
                self.session_public_keys.insert(session_id.to_string(), pk);
            }
        }

        let history_sessions = self
            .load_history_sessions(self.get_public_key(session_id))
            .await;
        let backend_kind = requested_backend.unwrap_or(Namespace::Default);
        let title = Some(stored.title);
        let is_user_title = Self::is_user_title(&title);
        let last_gen_title_msg = stored.messages.len();
        let metadata = SessionInsertMetadata {
            title,
            history_sessions,
            is_user_title,
            persisted_message_count: stored.messages.len(),
            last_gen_title_msg,
        };

        let state = self
            .insert_session_data(session_id, backend_kind, stored.messages, metadata)
            .await?;

        println!("♻️ Rehydrated session: {}", session_id);
        Ok((Some(state), true))
    }

    #[allow(dead_code)]
    pub async fn remove_session(&self, session_id: &str) {
        if self.sessions.remove(session_id).is_some() {
            println!("🗑️ Manually removed session: {}", session_id);
        }
    }

    // NOTE: start_background_tasks() is in background.rs
    // It combines title generation + async notification broadcasting

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

    pub async fn update_user_history(&self, session_id: &str, messages: &[ChatMessage]) {
        let persisted_message_count = self
            .sessions
            .get(session_id)
            .map(|session_data| session_data.persisted_message_count)
            .unwrap_or(0);

        if messages.len() <= persisted_message_count {
            return;
        }

        let new_messages = messages[persisted_message_count..].to_vec();
        tracing::info!(
            "Updating user history for session {}: {:?}",
            session_id,
            new_messages
        );

        // Update in-memory history with only messages not persisted yet.
        self.history_backend
            .update_history(session_id, &new_messages);
        let _ = self
            .history_backend
            .set_messages_persisted(session_id, false)
            .await;
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

    pub async fn get_history_sessions(
        &self,
        public_key: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<HistorySession>> {
        self.history_backend
            .get_history_sessions(public_key, limit.min(SESSION_LIST_LIMIT))
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

        // Delete all sessions from memory
        for session_id in session_ids.iter() {
            self.sessions.remove(session_id);
        }

        // Clear all public key mappings
        self.session_public_keys.clear();

        // Delete all sessions from persistent storage
        for session_id in session_ids {
            let _ = self.history_backend.delete_session(&session_id).await;
        }
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
