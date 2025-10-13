use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use tokio::sync::{Mutex, RwLock};

use aomi_terminal::{ChatTerminal, TerminalInput};

use crate::session::WebStateResponse;
use crate::session2::Frontend;
use crate::threads::ThreadPool;

const DEFAULT_POOL_SIZE: usize = 4;

struct SessionData {
    frontend: Arc<Mutex<Frontend>>,
    last_activity: Instant,
}

pub struct SessionManager {
    thread_pool: ThreadPool,
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
    app: Arc<ChatTerminal>,
    skip_docs: bool,
}

impl SessionManager {
    pub fn new(skip_docs: bool) -> Self {
        let app = Arc::new(ChatTerminal::new());
        let thread_pool = ThreadPool::new(DEFAULT_POOL_SIZE, Arc::clone(&app));
        Self {
            thread_pool,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval: Duration::from_secs(300),
            session_timeout: Duration::from_secs(1800),
            app,
            skip_docs,
        }
    }

    pub async fn process_user_message(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<WebStateResponse> {
        let thread = self
            .thread_pool
            .acquire()
            .await
            .context("no available threads to handle request")?;

        let frontend = self.get_or_create_session(session_id).await?;

        {
            let mut guard = frontend.lock().await;
            guard.begin_request(message);
        }

        let state_snapshot = {
            let guard = frontend.lock().await;
            guard.terminal_state.clone()
        };

        let (next_state, events) = thread
            .thread()
            .run(
                session_id,
                state_snapshot,
                TerminalInput::new(session_id, message),
            );

        {
            let mut guard = frontend.lock().await;
            guard.apply_thread_events(next_state, events);
        }

        self.touch_session(session_id).await;

        let snapshot = {
            let guard = frontend.lock().await;
            guard.snapshot()
        };

        Ok(snapshot)
    }

    async fn get_or_create_session(&self, session_id: &str) -> Result<Arc<Mutex<Frontend>>> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.last_activity = Instant::now();
            return Ok(Arc::clone(&session.frontend));
        }

        let frontend = Frontend::new(self.app.initialize_state());
        let frontend = Arc::new(Mutex::new(frontend));
        let session_data = SessionData {
            frontend: Arc::clone(&frontend),
            last_activity: Instant::now(),
        };

        sessions.insert(session_id.to_string(), session_data);
        println!("📝 Created stateless session: {}", session_id);
        Ok(frontend)
    }

    async fn touch_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.last_activity = Instant::now();
        }
    }

    pub async fn cleanup_inactive_sessions(&self) {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|session_id, session| {
            let alive = now.duration_since(session.last_activity) < self.session_timeout;
            if !alive {
                println!("🗑️ Cleaning up inactive stateless session: {}", session_id);
            }
            alive
        });
    }

    pub async fn start_cleanup_task(self: Arc<Self>) {
        let manager = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(manager.cleanup_interval);
            loop {
                interval.tick().await;
                manager.cleanup_inactive_sessions().await;
            }
        });
    }

    #[allow(dead_code)]
    pub async fn get_active_session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    pub fn skip_docs(&self) -> bool {
        self.skip_docs
    }
}
