use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use aomi_backend::{
    BackendType, ChatMessage, SessionState,
    session::{BackendwithTool, DefaultSessionState},
};

pub struct CliSession {
    session: DefaultSessionState,
    backends: Arc<HashMap<BackendType, Arc<BackendwithTool>>>,
    current_backend: BackendType,
}

impl CliSession {
    pub async fn new(
        backends: Arc<HashMap<BackendType, Arc<BackendwithTool>>>,
        backend: BackendType,
    ) -> Result<Self> {
        let backend_ref = backends
            .get(&backend)
            .context("requested backend not configured")?;

        let session = SessionState::new(Arc::clone(backend_ref), Vec::new(), Vec::new(), None).await?;

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

    pub async fn process_user_message(&mut self, input: &str) -> Result<()> {
        let normalized = input.to_lowercase();
        let requested_backend = if normalized.contains("l2b-magic-off") {
            Some(BackendType::Default)
        } else if normalized.contains("l2beat-magic") {
            Some(BackendType::L2b)
        } else {
            None
        };

        if let Some(target_backend) = requested_backend {
            self.switch_backend(target_backend).await?;
        }

        self.session.process_user_message(input.to_string()).await
    }

    pub async fn switch_backend(&mut self, backend: BackendType) -> Result<()> {
        if backend == self.current_backend {
            return Ok(());
        }

        let backend_impl = self
            .backends
            .get(&backend)
            .context("requested backend not configured")?;

        let history = self.session.messages.clone();
        self.session = SessionState::new(Arc::clone(backend_impl), history, Vec::new(), None).await?;
        self.current_backend = backend;
        Ok(())
    }

    pub async fn update_state(&mut self) {
        self.session.update_state().await;
    }
}
