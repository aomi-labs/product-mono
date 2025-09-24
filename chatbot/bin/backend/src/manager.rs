use anyhow::Result;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    agent_pool::AgentPool,
    session::{ReadinessState, SessionState},
};

pub type SharedSessionManager = Arc<SessionManager>;

struct SessionData {
    state: Arc<Mutex<SessionState>>,
    last_activity: Instant,
}

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    skip_docs: bool,
    agent_pool: Option<Arc<AgentPool>>,
}

impl SessionManager {
    pub fn new(skip_docs: bool) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval: Duration::from_secs(300),
            session_timeout: Duration::from_secs(1800),
            skip_docs,
            agent_pool: None,
        }
    }

    pub fn with_agent_pool(mut self, agent_pool: Arc<AgentPool>) -> Self {
        self.agent_pool = Some(agent_pool);
        self
    }

    pub async fn process_message_with_pool(
        &self,
        session_id: &str,
        message: String,
    ) -> Result<crate::session::SessionResponse> {
        let agent_pool = self.agent_pool.as_ref().ok_or_else(|| anyhow::anyhow!("Agent pool not initialised"))?;

        let session_state = self.get_or_create_session(session_id).await?;
        let mut state_guard = session_state.lock().await;

        let Some(trimmed_message) = state_guard.process_msg_from_ui(message).await? else {
            return Ok(state_guard.get_state());
        };

        let chat_history = state_guard.chat_history.clone();
        drop(state_guard);

        let result = agent_pool.process_message(trimmed_message, chat_history).await;

        let mut state_guard = session_state.lock().await;
        match result {
            Ok(processing_result) => state_guard.apply_agent_result(processing_result),
            Err(err) => {
                let detail = format!("Agent processing failed: {err}");
                state_guard.append_system(&detail);
                state_guard.set_readiness(ReadinessState::error(detail));
                state_guard.is_processing = false;
            }
        }

        Ok(state_guard.get_state())
    }

    pub async fn get_or_create_session(&self, session_id: &str) -> Result<Arc<Mutex<SessionState>>> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            session.last_activity = Instant::now();
            Ok(session.state.clone())
        } else {
            let state = SessionState::new(self.skip_docs).await?;
            let session_data = SessionData {
                state: Arc::new(Mutex::new(state)),
                last_activity: Instant::now(),
            };
            let state_clone = session_data.state.clone();
            sessions.insert(session_id.to_string(), session_data);
            println!("📝 Created new session: {}", session_id);
            Ok(state_clone)
        }
    }

    pub async fn cleanup_inactive_sessions(&self) {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();

        sessions.retain(|session_id, session_data| {
            let active = now.duration_since(session_data.last_activity) < self.session_timeout;
            if !active {
                println!("🗑️ Cleaning up inactive session: {}", session_id);
            }
            active
        });
    }

    pub async fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_some() {
            println!("🗑️ Manually removed session: {}", session_id);
        }
    }

    pub async fn start_cleanup_task(self: Arc<Self>) {
        let cleanup_manager = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_manager.cleanup_interval);
            loop {
                interval.tick().await;
                cleanup_manager.cleanup_inactive_sessions().await;
            }
        });
    }

    pub async fn get_active_session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
