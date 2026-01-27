use aomi_baml::baml_client::{async_client::B, types::ChatMessage as BamlChatMessage};
use dashmap::DashMap;
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error};

use crate::{
    history::HistoryBackend,
    manager::SessionData,
    types::{DefaultSessionState, MessageSender},
};

/// Background task runner that handles periodic maintenance tasks.
/// Owns cloned Arc references to shared state for thread-safe access.
pub struct BackgroundTasks {
    // Interval settings
    title_gen_interval: Duration,
    cleanup_interval: Duration,
    session_timeout: Duration,

    // Shared state (cloned Arcs for thread safety)
    sessions: Arc<DashMap<String, SessionData>>,
    session_public_keys: Arc<DashMap<String, String>>,
    history_backend: Arc<dyn HistoryBackend>,
    system_update_tx: broadcast::Sender<(String, Value)>,
}

impl BackgroundTasks {
    const SYSTEM_UPDATE_BUFFER: usize = 64;

    /// Create a new BackgroundTasks instance with cloned references to shared state
    pub fn new(
        sessions: Arc<DashMap<String, SessionData>>,
        session_public_keys: Arc<DashMap<String, String>>,
        history_backend: Arc<dyn HistoryBackend>,
    ) -> Self {
        let (system_update_tx, _) = broadcast::channel(Self::SYSTEM_UPDATE_BUFFER);
        Self {
            title_gen_interval: Duration::from_secs(5),
            cleanup_interval: Duration::from_secs(60 * 5), // 5 minutes
            session_timeout: Duration::from_secs(60 * 60), // 1 hour
            sessions,
            session_public_keys,
            history_backend,
            system_update_tx,
        }
    }

    pub fn subscribe_to_updates(&self) -> broadcast::Receiver<(String, Value)> {
        self.system_update_tx.subscribe()
    }

    /// Start all background tasks
    pub fn start(self: Arc<Self>) {
        // Task 1: Title generation + notifications
        let title_task = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(title_task.title_gen_interval);
            loop {
                interval.tick().await;
                title_task.process_title_generation().await;
                title_task.broadcast_async_notifications().await;
            }
        });

        // Task 2: Session cleanup
        let cleanup_task = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_task.cleanup_interval);
            loop {
                interval.tick().await;
                cleanup_task.cleanup_inactive_sessions().await;
            }
        });
    }

    // =========================================================================
    // Session Cleanup
    // =========================================================================

    /// Clean up inactive sessions: flush to DB then remove from memory
    async fn cleanup_inactive_sessions(&self) {
        let now = Instant::now();

        // Step 1: Identify expired sessions (don't remove yet)
        let sessions_to_cleanup: Vec<(String, bool)> = self
            .sessions
            .iter()
            .filter_map(|entry| {
                let should_cleanup =
                    now.duration_since(entry.value().last_activity) >= self.session_timeout;
                if should_cleanup {
                    Some((entry.key().clone(), entry.value().metadata.memory_mode))
                } else {
                    None
                }
            })
            .collect();

        if sessions_to_cleanup.is_empty() {
            return;
        }

        // Step 2: Flush history BEFORE removing from memory
        // This prevents race condition where new session loads stale data from DB
        let mut successfully_flushed: Vec<String> = Vec::new();

        for (session_id, memory_mode) in &sessions_to_cleanup {
            let pubkey = self
                .session_public_keys
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
            match self.history_backend.flush_history(&pk, session_id).await {
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
            self.sessions.remove(&session_id);
            debug!(session_id, "Cleaned up inactive session");

            if self.session_public_keys.get(&session_id).is_some() {
                self.session_public_keys.remove(&session_id);
            }
        }
    }

    // =========================================================================
    // SSE Notifications
    // =========================================================================

    /// Collect pending SSE events from all sessions and broadcast them
    async fn broadcast_async_notifications(&self) {
        for entry in self.sessions.iter() {
            let session_id = entry.key().clone();
            let session_data = entry.value();

            if let Ok(mut state) = session_data.state.try_lock() {
                let events = state.advance_sse_events();
                for event in events {
                    let mut value = match event {
                        aomi_core::SystemEvent::AsyncCallback(v) => v,
                        aomi_core::SystemEvent::SystemNotice(msg) => json!({
                            "type": "system_notice",
                            "message": msg,
                        }),
                        _ => continue,
                    };
                    // Add session_id to all SSE events
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("session_id".to_string(), json!(session_id.clone()));
                    }
                    let _ = self.system_update_tx.send((session_id.clone(), value));
                }
            }
        }
    }

    // =========================================================================
    // Title Generation
    // =========================================================================

    /// Process all sessions for title generation
    async fn process_title_generation(&self) {
        let sessions_to_check = self.collect_sessions_for_title_gen();

        for (session_id, state_arc, title_renewal_stamp) in sessions_to_check {
            let Some(messages) = Self::build_baml_request(&state_arc, title_renewal_stamp).await
            else {
                continue;
            };

            match Self::call_title_service(messages).await {
                Some(title) => self.apply_generated_title(&session_id, title).await,
                None => {
                    error!("Failed to generate title for session {}", session_id);
                }
            }
        }
    }

    /// Collect sessions eligible for title generation
    fn collect_sessions_for_title_gen(
        &self,
    ) -> Vec<(String, Arc<Mutex<DefaultSessionState>>, usize)> {
        self.sessions
            .iter()
            .filter_map(|entry| {
                let session_id = entry.key().clone();
                let session_data = entry.value();

                // Skip archived sessions
                if session_data.metadata.is_archived {
                    return None;
                }

                Some((
                    session_id,
                    session_data.state.clone(),
                    session_data.metadata.title_renewal_stamp,
                ))
            })
            .collect()
    }

    /// Extract messages from session state for title generation
    async fn build_baml_request(
        state: &Arc<Mutex<DefaultSessionState>>,
        title_renewal_stamp: usize,
    ) -> Option<Vec<BamlChatMessage>> {
        let state = state.lock().await;

        // Skip if still processing or no new messages since last title generation
        if state.is_processing || state.messages.len() <= title_renewal_stamp {
            return None;
        }

        let messages: Vec<BamlChatMessage> = state
            .messages
            .iter()
            .filter(|msg| !matches!(msg.sender, MessageSender::System))
            .map(|msg| {
                let role = match msg.sender {
                    MessageSender::User => "user",
                    MessageSender::Assistant => "assistant",
                    _ => "user",
                };
                BamlChatMessage {
                    role: role.to_string(),
                    content: msg.content.clone(),
                }
            })
            .collect();

        if messages.is_empty() {
            None
        } else {
            Some(messages)
        }
    }

    /// Call BAML service to generate title (native FFI - no HTTP)
    async fn call_title_service(messages: Vec<BamlChatMessage>) -> Option<String> {
        B.GenerateTitle
            .with_client(aomi_baml::AomiModel::ClaudeOpus4.baml_client_name())
            .call(&messages)
            .await
            .ok()
            .map(|r| r.title)
    }

    /// Apply generated title to session and persist if changed
    async fn apply_generated_title(&self, session_id: &str, title: String) {
        let title_changed = {
            let Some(mut session_data) = self.sessions.get_mut(session_id) else {
                return;
            };

            let msg_count = {
                let state = session_data.state.lock().await;
                state.messages.len()
            };

            let changed = session_data.metadata.title != title;
            if changed {
                session_data.metadata.title = title.clone();
            }
            session_data.metadata.title_renewal_stamp = msg_count;
            changed
        };

        if title_changed {
            // Persist to database if session has pubkey
            if self.session_public_keys.get(session_id).is_some() {
                if let Err(e) = self
                    .history_backend
                    .update_session_title(session_id, &title)
                    .await
                {
                    error!("Failed to persist title for session {}: {}", session_id, e);
                }
            }

            let _ = self.system_update_tx.send((
                session_id.to_string(),
                json!({
                    "type": "title_changed",
                    "session_id": session_id,
                    "new_title": title,
                }),
            ));
            debug!("Auto-generated title for session {}: {}", session_id, title);
        }
    }
}
