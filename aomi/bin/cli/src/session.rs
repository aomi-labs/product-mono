use std::{collections::HashMap, sync::Arc};

use aomi_backend::{
    ChatMessage, Namespace, SessionState,
    session::{AomiBackend, DefaultSessionState},
};
use aomi_core::{AomiModel, CoreApp, Selection, SystemEvent};
use aomi_forge::ForgeApp;
use aomi_l2beat::L2BeatApp;
use eyre::{ContextCompat, Result};
use tokio::sync::RwLock;

pub struct CliOpts {
    pub no_docs: bool,
    pub skip_mcp: bool,
    pub rig_model: AomiModel,
    pub baml_client: String,
}

pub struct CliSession {
    session: DefaultSessionState,
    backends: Arc<RwLock<HashMap<Namespace, Arc<AomiBackend>>>>,
    current_backend: Namespace,
    opts: CliOpts,
}

impl CliSession {
    pub async fn new(
        backends: Arc<RwLock<HashMap<Namespace, Arc<AomiBackend>>>>,
        backend: Namespace,
        opts: CliOpts,
    ) -> Result<Self> {
        let backend_ref = {
            let guard = backends.read().await;
            guard
                .get(&backend)
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
            opts,
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

        let backend_impl = {
            let guard = self.backends.read().await;
            guard
                .get(&backend)
                .context("requested backend not configured")?
                .clone()
        };

        let history = self.session.messages.clone();
        self.session = SessionState::new(Arc::clone(&backend_impl), history)
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

    pub fn current_backend(&self) -> Namespace {
        self.current_backend
    }

    pub fn rig_model(&self) -> AomiModel {
        self.opts.rig_model
    }

    pub fn baml_client(&self) -> &str {
        self.opts.baml_client.as_str()
    }

    /// Check if there are ongoing tool calls that haven't completed yet
    pub async fn has_ongoing_tool_calls(&self) -> bool {
        self.session.has_ongoing_tool_calls().await
    }

    pub async fn set_models(&mut self, rig_model: AomiModel, baml_client: String) -> Result<()> {
        self.opts.rig_model = rig_model;
        self.opts.baml_client = baml_client.clone();
        self.refresh_backends().await
    }

    pub async fn models_summary(&self) -> Result<String> {
        Ok(format!(
            "rig={} baml={}",
            self.opts.rig_model.rig_label(),
            self.opts.baml_client
        ))
    }

    async fn refresh_backends(&mut self) -> Result<()> {
        let current = self.current_backend;
        let mut map = HashMap::new();
        let selection = Selection {
            rig: self.opts.rig_model,
            baml: AomiModel::parse_baml(&self.opts.baml_client)
                .unwrap_or(AomiModel::ClaudeOpus4),
        };
        let chat_app = Arc::new(
            CoreApp::new_with_models(self.opts.no_docs, self.opts.skip_mcp, selection.rig)
                .await?,
        );
        let l2b_app = Arc::new(L2BeatApp::new_with_models(
            self.opts.no_docs,
            self.opts.skip_mcp,
            selection,
        )
        .await?);
        let forge_app = Arc::new(ForgeApp::new_with_models(
            self.opts.no_docs,
            self.opts.skip_mcp,
            selection,
        )
        .await?);

        let chat_backend: Arc<AomiBackend> = chat_app;
        let l2b_backend: Arc<AomiBackend> = l2b_app;
        let forge_backend: Arc<AomiBackend> = forge_app;

        map.insert(Namespace::Default, chat_backend);
        map.insert(Namespace::L2b, l2b_backend);
        map.insert(Namespace::Forge, forge_backend);

        if let Some(test_backend) = {
            let guard = self.backends.read().await;
            guard.get(&Namespace::Test).cloned()
        } {
            map.insert(Namespace::Test, test_backend);
        }

        {
            let mut guard = self.backends.write().await;
            *guard = map;
        }

        if let Some(backend_impl) = {
            let guard = self.backends.read().await;
            guard.get(&current).cloned()
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
