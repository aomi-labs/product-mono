use std::{collections::HashMap, sync::Arc};

use aomi_backend::{
    Namespace, ChatMessage, SessionState,
    session::{AomiBackend, DefaultSessionState},
};
use aomi_chat::SystemEvent;
use eyre::{ContextCompat, Result};

pub struct CliSession {
    session: DefaultSessionState,
    backends: Arc<HashMap<Namespace, Arc<AomiBackend>>>,
    current_backend: Namespace,
}

impl CliSession {
    pub async fn new(
        backends: Arc<HashMap<Namespace, Arc<AomiBackend>>>,
        backend: Namespace,
    ) -> Result<Self> {
        let backend_ref = backends
            .get(&backend)
            .context("requested backend not configured")?;

        let session = SessionState::new(Arc::clone(backend_ref), Vec::new())
            .await
            .map_err(|e| eyre::eyre!(e.to_string()))?;

        Ok(Self {
            session,
            backends,
            current_backend: backend,
        })
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.session.messages
    }

    pub fn is_processing(&self) -> bool {
        self.session.is_processing
    }

    pub fn has_streaming_messages(&self) -> bool {
        self.session.messages.iter().any(|msg| msg.is_streaming)
    }

    pub async fn send_user_input(&mut self, input: &str) -> Result<()> {
        let normalized = input.to_lowercase();
        let requested_backend = match normalized.as_str() {
            s if s.contains("default-magic") => Some(Namespace::Default),
            s if s.contains("l2beat-magic") => Some(Namespace::L2b),
            s if s.contains("forge-magic") => Some(Namespace::Forge),
            s if s.contains("test-magic") => Some(Namespace::Test),
            _ => None,
        };

        if let Some(target_backend) = requested_backend {
            self.switch_backend(target_backend).await?;
        }

        self.session
            .send_user_input(input.to_string())
            .await
            .map_err(|e| eyre::eyre!(e.to_string()))
    }

    pub async fn switch_backend(&mut self, backend: Namespace) -> Result<()> {
        if backend == self.current_backend {
            return Ok(());
        }

        let backend_impl = self
            .backends
            .get(&backend)
            .context("requested backend not configured")?;

        let history = self.session.messages.clone();
        self.session = SessionState::new(Arc::clone(backend_impl), history)
            .await
            .map_err(|e| eyre::eyre!(e.to_string()))?;
        self.current_backend = backend;
        Ok(())
    }

    pub async fn sync_state(&mut self) {
        self.session.sync_state().await;
    }

    pub fn push_system_event(&mut self, event: SystemEvent) {
        self.session.system_event_queue.push(event);
    }

    /// Take (consume) active system events (inline events from path 1)
    pub fn advance_frontend_events(&mut self) -> Vec<SystemEvent> {
        self.session.advance_frontend_events()
    }
}
