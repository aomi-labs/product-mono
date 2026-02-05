use std::{collections::HashMap, sync::Arc};

use aomi_backend::{
    BuildOpts, ChatMessage, Namespace, Selection, SessionState, build_backends,
    session::{AomiBackend, DefaultSessionState},
};
use aomi_core::{AomiModel, SystemEvent};
use eyre::{ContextCompat, Result};
use tokio::sync::RwLock;

/// Type alias for the shared backends map
pub type BackendsMap = Arc<RwLock<HashMap<(Namespace, Selection), Arc<AomiBackend>>>>;

pub struct CliSession {
    session: DefaultSessionState,
    backends: BackendsMap,
    current_backend: Namespace,
    current_selection: Selection,
    opts: BuildOpts,
    user_state_override: Option<aomi_backend::UserState>,
}

impl CliSession {
    pub async fn new(backends: BackendsMap, backend: Namespace, opts: BuildOpts) -> Result<Self> {
        let selection = opts.selection;
        let backend_ref = {
            let guard = backends.read().await;
            guard
                .get(&(backend, selection))
                .context("requested backend not configured")?
                .clone()
        };

        let session = SessionState::new(Arc::clone(&backend_ref), Vec::new())
            .await
            .map_err(|e| eyre::eyre!(e.to_string()))?;

        Ok(Self {
            session,
            backends,
            current_backend: backend,
            current_selection: selection,
            opts,
            user_state_override: None,
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
            s if s.contains("admin-magic") => Some(Namespace::Admin),
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

        let backend_impl = {
            let guard = self.backends.read().await;
            guard
                .get(&(backend, self.current_selection))
                .context("requested backend not configured")?
                .clone()
        };

        let history = self.session.messages.clone();
        self.session = SessionState::new(Arc::clone(&backend_impl), history)
            .await
            .map_err(|e| eyre::eyre!(e.to_string()))?;
        if let Some(state) = self.user_state_override.clone() {
            self.session.sync_user_state(state).await;
        }
        self.current_backend = backend;
        Ok(())
    }

    pub async fn sync_state(&mut self) {
        self.session.sync_state().await;
    }

    pub async fn set_user_state_override(&mut self, state: aomi_backend::UserState) -> Result<()> {
        self.user_state_override = Some(state.clone());
        self.session.sync_user_state(state).await;
        Ok(())
    }

    pub fn push_system_event(&mut self, event: SystemEvent) {
        self.session.system_event_queue.push(event);
    }

    /// Take (consume) active system events (inline events from path 1)
    pub fn advance_frontend_events(&mut self) -> Vec<SystemEvent> {
        let mut events = self.session.advance_http_events();
        events.extend(self.session.advance_sse_events());
        events
    }

    pub fn rig_model(&self) -> AomiModel {
        self.opts.selection.rig
    }

    pub fn baml_client(&self) -> &str {
        self.opts.selection.baml.baml_client_name()
    }

    /// Check if there are ongoing tool calls that haven't completed yet
    pub async fn has_ongoing_tool_calls(&self) -> bool {
        self.session.has_ongoing_tool_calls().await
    }

    pub async fn set_models(&mut self, rig_model: AomiModel, baml_client: AomiModel) -> Result<()> {
        self.opts.selection.rig = rig_model;
        self.opts.selection.baml = baml_client;
        self.refresh_backends().await
    }

    pub async fn models_summary(&self) -> Result<String> {
        Ok(format!(
            "rig={} baml={}",
            self.opts.selection.rig.rig_label(),
            self.opts.selection.baml.baml_client_name()
        ))
    }

    async fn refresh_backends(&mut self) -> Result<()> {
        let current = self.current_backend;
        let selection = self.current_selection;
        let mut map = build_backends(vec![
            (Namespace::Default, self.opts),
            (Namespace::L2b, self.opts),
            (Namespace::Forge, self.opts),
        ])
        .await
        .map_err(|e| eyre::eyre!(e.to_string()))?
        .into_iter()
        .collect::<HashMap<_, _>>();

        if let Some(test_backend) = {
            let guard = self.backends.read().await;
            guard.get(&(Namespace::Test, selection)).cloned()
        } {
            map.insert((Namespace::Test, selection), test_backend);
        }

        {
            let mut guard = self.backends.write().await;
            *guard = map;
        }

        if let Some(backend_impl) = {
            let guard = self.backends.read().await;
            guard.get(&(current, selection)).cloned()
        } {
            let history = self.session.messages.clone();
            self.session = SessionState::new(Arc::clone(&backend_impl), history)
                .await
                .map_err(|e| eyre::eyre!(e.to_string()))?;
            self.current_backend = current;
        }

        Ok(())
    }
}
