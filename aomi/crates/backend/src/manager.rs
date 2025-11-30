use anyhow::Result;
use baml_client::{
    apis::{configuration::Configuration, default_api},
    models::{ChatMessage as BamlChatMessage, GenerateTitleRequest},
};
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
    session::{BackendwithTool, ChatMessage, DefaultSessionState, HistorySession, SystemUpdate},
};

const SESSION_TIMEOUT: u64 = 3600; // 1 hour
const SESSION_LIST_LIMIT: usize = i32::MAX as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BackendType {
    Default,
    L2b,
}

/// Metadata about a session (managed by SessionManager, not SessionState)
#[derive(Clone)]
pub struct SessionMetadata {
    pub title: Option<String>,
    pub is_archived: bool,
    pub last_gen_title_msg: usize,
    pub history_sessions: Vec<HistorySession>,
}

struct SessionData {
    state: Arc<Mutex<DefaultSessionState>>,
    last_activity: Instant,
    backend_kind: BackendType,
    memory_mode: bool,
    // Metadata fields (not chat-stream related)
    title: Option<String>,
    history_sessions: Vec<HistorySession>,
    is_archived: bool,
    last_gen_title_msg: usize,
}

pub struct SessionManager {
    sessions: Arc<DashMap<String, SessionData>>,
    session_public_keys: Arc<DashMap<String, String>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    backends: Arc<HashMap<BackendType, Arc<BackendwithTool>>>,
    history_backend: Arc<dyn HistoryBackend>,
    system_update_tx: broadcast::Sender<SystemUpdate>,
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
        let (system_update_tx, _system_update_rx) = broadcast::channel(64);
        // NOTE: _system_update_rx is intentionally dropped here.
        // The broadcast channel works with only senders - receivers are created via subscribe().
        // Watch for:
        // - If buffer fills (64 messages) with no subscribers, oldest messages are dropped (expected)
        // - If send() is called with no subscribers, it returns Err (we ignore with `let _ = ...`)
        // Memory leaks are not a concern since the channel is bounded.
        Self {
            sessions: Arc::new(DashMap::new()),
            session_public_keys: Arc::new(DashMap::new()),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(SESSION_TIMEOUT),
            backends,
            history_backend,
            system_update_tx,
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

        // Only need messages from SessionState now - metadata is in SessionData
        let current_messages = {
            let guard = state.lock().await;
            guard.messages.clone()
        };

        let session_state =
            DefaultSessionState::new(backend, current_messages).await?;

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
    pub fn subscribe_to_updates(&self) -> tokio::sync::broadcast::Receiver<SystemUpdate> {
        self.system_update_tx.subscribe()
    }

    /// Updates the title of a session in memory and persists to storage
    pub async fn update_session_title(&self, session_id: &str, title: String) -> anyhow::Result<()> {
        if let Some(mut session_data) = self.sessions.get_mut(session_id) {
            session_data.title = Some(title.clone());
            tracing::info!("Updated title for session {} - {}", session_id, title);
            drop(session_data);

            // Persist title when backing storage exists
            self.history_backend
                .update_session_title(session_id, &title)
                .await?;

            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found: {}", session_id))
        }
    }

    pub async fn set_session_public_key(&self, session_id: &str, public_key: Option<String>) {
        if let Some(pk) = public_key {
            self.session_public_keys
                .insert(session_id.to_string(), pk.clone());

            // Create session in database when pubkey is first associated and load historical messages
            // This handles the case where session was created without a pubkey
            // Get current title from session data to persist to DB
            let current_title = self.get_session_title(session_id);
            match self
                .history_backend
                .get_or_create_history(Some(pk), session_id.to_string(), current_title)
                .await
            {
                Ok(historical_summary) => {
                    if let Some(session_data) = self.sessions.get(session_id) {
                        let session = session_data.state.clone();
                        drop(session_data);

                        let session = session.lock().await;

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

    pub fn get_public_key(&self, session_id: &str) -> Option<String> {
        self.session_public_keys.get(session_id).map(|pk| pk.value().clone())
    }

    /// Get a session only if it exists in memory. Does NOT recreate deleted sessions.
    pub fn get_session_if_exists(&self, session_id: &str) -> Option<Arc<Mutex<DefaultSessionState>>> {
        self.sessions.get(session_id).map(|entry| entry.state.clone())
    }

    /// Get the title for a session
    pub fn get_session_title(&self, session_id: &str) -> Option<String> {
        self.sessions.get(session_id).and_then(|entry| entry.title.clone())
    }

    /// Get session metadata (title, is_archived, last_gen_title_msg, history_sessions)
    pub fn get_session_metadata(&self, session_id: &str) -> Option<SessionMetadata> {
        self.sessions.get(session_id).map(|entry| SessionMetadata {
            title: entry.title.clone(),
            is_archived: entry.is_archived,
            last_gen_title_msg: entry.last_gen_title_msg,
            history_sessions: entry.history_sessions.clone(),
        })
    }

    pub async fn get_or_create_session(
        &self,
        session_id: &str,
        requested_backend: Option<BackendType>,
        initial_title: Option<String>,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        let pubkey = self
            .session_public_keys
            .get(session_id)
            .map(|pk| pk.value().clone());

        let history_sessions = if let Some(pk) = pubkey.clone() {
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
        } else {
            Vec::new()
        };

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
                let mut historical_messages = Vec::new();

                // Load historical messages and ensure DB session exists (if pubkey is present)
                // Pass initial_title to persist when creating new session in DB
                if let Some(msg) = self
                    .history_backend
                    .get_or_create_history(pubkey.clone(), session_id.to_string(), initial_title.clone())
                    .await?
                {
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
                let session_state =
                    DefaultSessionState::new(backend, historical_messages)
                        .await?;

                let session_data = SessionData {
                    state: Arc::new(Mutex::new(session_state)),
                    last_activity: Instant::now(),
                    backend_kind,
                    memory_mode: false,
                    title: initial_title,
                    history_sessions,
                    is_archived: false,
                    last_gen_title_msg: 0,
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

    pub fn start_title_generation_task(self: Arc<Self>) {
        let manager = Arc::clone(&self);
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                // Collect sessions that might need title updates with their metadata
                // We check metadata from SessionData without locking SessionState
                let sessions_to_check: Vec<(String, Arc<Mutex<DefaultSessionState>>, usize)> = manager
                    .sessions
                    .iter()
                    .filter_map(|entry| {
                        let session_id = entry.key().clone();
                        let session_data = entry.value();

                        // Skip archived sessions
                        if session_data.is_archived {
                            return None;
                        }

                        // Skip if title is already set and not a fallback marker `#[...]`
                        if let Some(ref title) = session_data.title {
                            if !title.starts_with("#[") {
                                return None; // Has user-provided or already-generated title
                            }
                        }

                        Some((session_id, session_data.state.clone(), session_data.last_gen_title_msg))
                    })
                    .collect();

                for (session_id, state_arc, last_gen_title_msg) in sessions_to_check {
                    // Now we need to lock SessionState only to get messages
                    let baml_messages: Vec<BamlChatMessage> = {
                        let state = state_arc.lock().await;

                        // Skip if still processing
                        if state.is_processing {
                            continue;
                        }

                        // Skip if no new messages since last summarization
                        if state.messages.len() <= last_gen_title_msg {
                            continue;
                        }

                        // Convert messages to BAML format
                        state
                            .messages
                            .iter()
                            .filter(|msg| !matches!(msg.sender, crate::session::MessageSender::System))
                            .map(|msg| {
                                let role = match msg.sender {
                                    crate::session::MessageSender::User => "user",
                                    crate::session::MessageSender::Assistant => "assistant",
                                    _ => "user",
                                };
                                BamlChatMessage::new(
                                    role.to_string(),
                                    msg.content.clone(),
                                )
                            })
                            .collect()
                    }; // Lock released here

                    // Need at least 1 message to summarize
                    if baml_messages.is_empty() {
                        continue;
                    }

                    // Call BAML service (no lock held)
                    let baml_config = Configuration {
                        base_path: std::env::var("BAML_SERVER_URL")
                            .unwrap_or_else(|_| "http://localhost:2024".to_string()),
                        ..Default::default()
                    };

                    let request = GenerateTitleRequest::new(baml_messages);

                    match default_api::generate_title(&baml_config, request).await {
                        Ok(result) => {
                            // Update title and last_gen_title_msg in SessionData (no SessionState lock needed)
                            if let Some(mut session_data) = manager.sessions.get_mut(&session_id) {
                                let msg_count = {
                                    let state = session_data.state.lock().await;
                                    state.messages.len()
                                };

                                // Only update if title actually changed (#7 - deduplication)
                                let title_changed = session_data.title.as_ref() != Some(&result.title);
                                if title_changed {
                                    session_data.title = Some(result.title.clone());
                                }
                                session_data.last_gen_title_msg = msg_count;
                                drop(session_data);

                                // Only broadcast and persist if title changed
                                if title_changed {
                                    // Persist title to database
                                    if let Err(e) = manager.history_backend
                                        .update_session_title(&session_id, &result.title)
                                        .await
                                    {
                                        tracing::error!(
                                            "Failed to persist title for session {}: {}",
                                            session_id,
                                            e
                                        );
                                    }

                                    let _ = manager.system_update_tx.send(SystemUpdate::TitleChanged {
                                        session_id: session_id.clone(),
                                        new_title: result.title.clone(),
                                    });
                                    tracing::info!(
                                        "📝 Auto-generated title for session {}: {}",
                                        session_id,
                                        result.title
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to generate title for session {}: {}",
                                session_id,
                                e
                            );
                        }
                    }
                }
            }
        });
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
