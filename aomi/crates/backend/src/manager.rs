use anyhow::Result;
use aomi_core::BuildOpts;
use dashmap::DashMap;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    auth::NamespaceAuth,
    background::BackgroundTasks,
    build_backends,
    history::{HistoryBackend, DEFAULT_TITLE},
    namespace::{Namespace, Selection},
    types::{AomiBackend, ChatMessage, DefaultSessionState, SessionRecord},
};

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
    pub(crate) selection: Selection,
    pub(crate) metadata: SessionMetadata,
}

pub struct SessionManager {
    pub(crate) sessions: Arc<DashMap<String, SessionData>>,
    pub(crate) session_public_keys: Arc<DashMap<String, String>>,
    background_tasks: Arc<BackgroundTasks>,
    backends: Arc<DashMap<(Namespace, Selection), Arc<AomiBackend>>>,
    pub(crate) history_backend: Arc<dyn HistoryBackend>,
}

impl SessionManager {
    pub fn new(
        backends: HashMap<(Namespace, Selection), Arc<AomiBackend>>,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Self {
        let sessions = Arc::new(DashMap::new());
        let session_public_keys = Arc::new(DashMap::new());
        let background_tasks = Arc::new(BackgroundTasks::new(
            Arc::clone(&sessions),
            Arc::clone(&session_public_keys),
            Arc::clone(&history_backend),
        ));
        background_tasks.clone().start();

        let backends_map = Arc::new(DashMap::new());
        for ((namespace, selection), backend) in backends {
            backends_map.insert((namespace, selection), backend);
        }

        Self {
            sessions,
            session_public_keys,
            background_tasks,
            backends: backends_map,
            history_backend,
        }
    }

    pub fn with_backend(
        chat_backend: Arc<AomiBackend>,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Self {
        let mut backends: HashMap<(Namespace, Selection), Arc<AomiBackend>> = HashMap::new();
        backends.insert((Namespace::Default, Selection::default()), chat_backend);
        Self::new(backends, history_backend)
    }

    pub fn build_backend_map(
        default_backend: Arc<AomiBackend>,
        l2b_backend: Option<Arc<AomiBackend>>,
        forge_backend: Option<Arc<AomiBackend>>,
        polymarket_backend: Option<Arc<AomiBackend>>,
    ) -> HashMap<(Namespace, Selection), Arc<AomiBackend>> {
        let selection = Selection::default();
        let mut backends: HashMap<(Namespace, Selection), Arc<AomiBackend>> = HashMap::new();
        backends.insert((Namespace::Default, selection), default_backend);
        if let Some(l2b_backend) = l2b_backend {
            backends.insert((Namespace::L2b, selection), l2b_backend);
        }
        if let Some(forge_backend) = forge_backend {
            backends.insert((Namespace::Forge, selection), forge_backend);
        }
        if let Some(polymarket_backend) = polymarket_backend {
            backends.insert((Namespace::Polymarket, selection), polymarket_backend);
        }
        backends
    }

    /// Add a backend for a specific namespace and model selection.
    pub fn add_backend(
        &self,
        namespace: Namespace,
        selection: Selection,
        backend: Arc<AomiBackend>,
    ) {
        self.backends.insert((namespace, selection), backend);
    }

    /// Check if a backend exists for the given namespace and selection.
    pub fn has_backend(&self, namespace: Namespace, selection: Selection) -> bool {
        self.backends.contains_key(&(namespace, selection))
    }

    /// Ensure a backend exists for the given namespace and selection.
    /// Builds a new backend if one doesn't exist for this exact combination.
    pub async fn ensure_backend(&self, namespace: Namespace, selection: Selection) -> Result<()> {
        // If exact selection exists, nothing to do
        if self.has_backend(namespace, selection) {
            return Ok(());
        }

        // Build new backend for this namespace+selection combo
        debug!("Building backend for {:?}/{:?}", namespace, selection);
        let opts = BuildOpts {
            no_docs: true,  // Skip docs for faster builds
            skip_mcp: true, // Skip MCP for faster builds
            no_tools: false,
            selection,
        };

        let backends = build_backends(vec![(namespace, opts)]).await?;

        if let Some(backend) = backends.get(&(namespace, selection)) {
            self.add_backend(namespace, selection, Arc::clone(backend));
            Ok(())
        } else {
            Err(anyhow::anyhow!("Backend not found after building"))
        }
    }

    /// Initialize all backends and create a SessionManager
    pub async fn initialize(
        skip_docs: bool,
        skip_mcp: bool,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Result<Self> {
        tracing::info!("Initializing backends...");

        let selection = Selection::default();
        let backends = build_backends(vec![
            (
                Namespace::Default,
                BuildOpts {
                    no_docs: skip_docs,
                    skip_mcp,
                    no_tools: false,
                    selection,
                },
            ),
            (
                Namespace::L2b,
                BuildOpts {
                    no_docs: skip_docs,
                    skip_mcp,
                    no_tools: false,
                    selection,
                },
            ),
            (
                Namespace::Forge,
                BuildOpts {
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

        Ok(Self::new(backends, history_backend))
    }

    /// Replace the backend for a session, changing namespace and/or selection.
    /// Returns the new (namespace, selection) tuple.
    pub async fn replace_backend(
        &self,
        requested_backend: Option<Namespace>,
        requested_selection: Option<Selection>,
        state: Arc<Mutex<DefaultSessionState>>,
        current_backend: Namespace,
        current_selection: Selection,
    ) -> Result<(Namespace, Selection)> {
        let target_backend = requested_backend.unwrap_or(current_backend);
        let target_selection = requested_selection.unwrap_or(current_selection);

        // No change needed if both match
        if target_backend == current_backend && target_selection == current_selection {
            return Ok((current_backend, current_selection));
        }

        debug!(
            "Replacing backend: {:?}/{:?} -> {:?}/{:?}",
            current_backend, current_selection, target_backend, target_selection
        );

        // Ensure backend exists for this namespace+selection combo
        self.ensure_backend(target_backend, target_selection)
            .await?;

        let key = (target_backend, target_selection);
        let backend = self
            .backends
            .get(&key)
            .map(|entry| Arc::clone(entry.value()))
            .expect("backend should exist after ensure_backend");

        let current_messages = {
            let guard = state.lock().await;
            guard.messages.clone()
        };

        let session_state = DefaultSessionState::new(backend, current_messages).await?;

        {
            let mut guard = state.lock().await;
            // Shutdown old session's background tasks before replacing
            guard.shutdown();
            *guard = session_state;
        }

        Ok((target_backend, target_selection))
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
        self.background_tasks.subscribe_to_updates()
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
        selection: Selection,
        messages: Vec<ChatMessage>,
        metadata: SessionMetadata,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        // Ensure backend exists for this namespace+selection combo
        self.ensure_backend(namespace, selection).await?;

        let key = (namespace, selection);
        let backend = self
            .backends
            .get(&key)
            .map(|entry| Arc::clone(entry.value()))
            .expect("backend should exist after ensure_backend");

        let session_state = DefaultSessionState::new(backend, messages).await?;

        let session_data = SessionData {
            state: Arc::new(Mutex::new(session_state)),
            last_activity: Instant::now(),
            namespace,
            selection,
            metadata,
        };

        let new_session = session_data.state.clone();
        self.sessions.insert(session_id.to_string(), session_data);
        Ok(new_session)
    }

    /// Get or create a session with namespace authorization.
    ///
    /// This method:
    /// 1. Merges authorization from API key and user namespaces
    /// 2. Validates the requested namespace against authorization
    /// 3. Returns error if not authorized
    /// 4. Creates/retrieves session if authorized with the specified selection
    ///
    /// If `selection` is `None`, preserves the session's current selection (or uses default for new sessions).
    pub async fn get_or_create_session(
        &self,
        session_id: &str,
        auth: &mut NamespaceAuth,
        selection: Option<Selection>,
    ) -> anyhow::Result<Arc<Mutex<DefaultSessionState>>> {
        // Set public key mapping if provided
        if let Some(ref pk) = auth.pub_key {
            self.set_session_public_key(session_id, Some(pk.clone()))
                .await;
        }

        // Merge authorization from API key and user namespaces
        let user_namespaces = if let Some(ref pk) = auth.pub_key {
            self.get_user_namespaces(pk).await.ok()
        } else {
            None
        };
        auth.merge_authorization(user_namespaces);

        // Check if requested namespace is authorized
        if !auth.is_authorized() {
            return Err(anyhow::anyhow!(
                "Namespace '{}' not authorized. Allowed: {:?}",
                auth.requested_namespace,
                auth.current_authorization
            ));
        }

        let requested_backend = auth.requested_backend();

        // 1. Check if session exists in memory
        if let Some(session_data_ref) = self.sessions.get(session_id) {
            let state = session_data_ref.state.clone();
            let namespace = session_data_ref.namespace;
            let current_selection = session_data_ref.selection;
            drop(session_data_ref);

            // Handle backend switching if requested (namespace or selection change)
            // If selection is None, preserve current selection
            let (new_namespace, new_selection) = self
                .replace_backend(
                    requested_backend,
                    selection, // None = keep current, Some(s) = switch to s
                    state.clone(),
                    namespace,
                    current_selection,
                )
                .await?;

            if let Some(mut session_data) = self.sessions.get_mut(session_id) {
                session_data.namespace = new_namespace;
                session_data.selection = new_selection;
                session_data.last_activity = Instant::now();
            }

            return Ok(state);
        }

        // Use provided selection or default for new sessions
        let selection = selection.unwrap_or_default();

        // 2. Try to load from DB
        if let Some(stored) = self.history_backend.get_session(session_id).await? {
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
                .create_session(session_id, namespace, selection, stored.messages, metadata)
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
            .create_session(session_id, namespace, selection, Vec::new(), metadata)
            .await?;

        debug!(session_id, "Created new session");
        Ok(new_session)
    }

    #[allow(dead_code)]
    pub fn active_session_count(&self) -> usize {
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

    /// Gets the namespaces allowed for a user from the database.
    /// Returns empty vec if user not found.
    pub async fn get_user_namespaces(&self, public_key: &str) -> anyhow::Result<Vec<String>> {
        self.history_backend.get_user_namespaces(public_key).await
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
