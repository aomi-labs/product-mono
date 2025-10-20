use anyhow::Result;
use aomi_agent::{ChatApp, Message};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::session::SessionState;

struct SessionData {
    state: Arc<Mutex<SessionState>>,
    history: Arc<Mutex<Vec<Message>>>,
    last_activity: Instant,
}

pub struct SessionManager {
    // thread_pools: ThreadPools,
    // sessions: HashMap<String, TerminalState>,
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    chat_app: Arc<ChatApp>,
}

impl SessionManager {
    pub fn new(chat_app: Arc<ChatApp>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(1800), // 30 minutes
            chat_app,
        }
    }

    pub async fn get_or_create_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<Mutex<SessionState>>, anyhow::Error> {
        let mut sessions = self.sessions.write().await;

        if let Some(session_data) = sessions.get_mut(session_id) {
            session_data.last_activity = Instant::now();
            Ok(session_data.state.clone())
        } else {
            let history = Arc::new(Mutex::new(Vec::new()));
            let web_chat_state = SessionState::new(
                Arc::clone(&self.chat_app),
                Arc::clone(&history),
            )
            .await?;
            let session_data = SessionData {
                state: Arc::new(Mutex::new(web_chat_state)),
                history,
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

    #[allow(dead_code)]
    pub async fn get_active_session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    pub fn chat_app(&self) -> Arc<ChatApp> {
        Arc::clone(&self.chat_app)
    }
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}
