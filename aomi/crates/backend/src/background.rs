use aomi_baml::baml_client::{async_client::B, types::ChatMessage as BamlChatMessage};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

use crate::{
    manager::SessionManager,
    types::{DefaultSessionState, MessageSender},
};

impl SessionManager {
    /// Start background tasks: title generation + async notification broadcasting
    pub fn start_background_tasks(self: Arc<Self>) {
        let manager = Arc::clone(&self);
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        tokio::spawn(async move {
            loop {
                interval.tick().await;
                manager.process_title_generation().await;
                manager.broadcast_async_notifications().await;
            }
        });
    }

    /// Collect pending async events from all sessions and broadcast them via SSE
    async fn broadcast_async_notifications(&self) {
        for entry in self.sessions.iter() {
            let session_id = entry.key().clone();
            let session_data = entry.value();

            // Try to get pending notifications without blocking
            if let Ok(mut state) = session_data.state.try_lock() {
                let events = state.advance_frontend_events();
                for event in events {
                    if let aomi_chat::SystemEvent::AsyncUpdate(mut value) = event {
                        if let Some(obj) = value.as_object_mut() {
                            obj.insert("session_id".to_string(), json!(session_id));
                        }
                        let _ = self.system_update_tx.send(value);
                    }
                }
            }
        }
    }

    /// Process all sessions for title generation
    async fn process_title_generation(&self) {
        let sessions_to_check = self.collect_sessions_for_title_gen();

        for (session_id, state_arc, last_gen_title_msg) in sessions_to_check {
            let Some(messages) = Self::build_baml_request(&state_arc, last_gen_title_msg).await
            else {
                continue;
            };

            match Self::call_title_service(messages).await {
                Some(title) => self.apply_generated_title(&session_id, title).await,
                None => {
                    tracing::error!("Failed to generate title for session {}", session_id);
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

                // Skip archived or user-titled sessions
                if session_data.is_archived || session_data.is_user_title {
                    return None;
                }

                Some((
                    session_id,
                    session_data.state.clone(),
                    session_data.last_gen_title_msg,
                ))
            })
            .collect()
    }

    /// Extract messages from session state for title generation
    async fn build_baml_request(
        state: &Arc<Mutex<DefaultSessionState>>,
        last_gen_title_msg: usize,
    ) -> Option<Vec<BamlChatMessage>> {
        let state = state.lock().await;

        // Skip if still processing or no new messages
        if state.is_processing || state.messages.len() <= last_gen_title_msg {
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

            // Race condition check: user rename wins
            if session_data.is_user_title {
                tracing::info!(
                    "Skipping auto-generated title for session {} - user has manually set title",
                    session_id
                );
                return;
            }

            let msg_count = {
                let state = session_data.state.lock().await;
                state.messages.len()
            };

            let changed = session_data.title.as_ref() != Some(&title);
            if changed {
                session_data.title = Some(title.clone());
                session_data.is_user_title = false;
            }
            session_data.last_gen_title_msg = msg_count;
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
                    tracing::error!("Failed to persist title for session {}: {}", session_id, e);
                }
            }

            let _ = self.system_update_tx.send(json!({
                "type": "title_changed",
                "session_id": session_id,
                "new_title": title,
            }));
            tracing::info!(
                "📝 Auto-generated title for session {}: {}",
                session_id,
                title
            );
        }
    }
}
