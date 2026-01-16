use crate::clients::{ExternalClients, init_external_clients};
use crate::streams::{ToolCompletion, ToolReciever};
use crate::types::ToolMetadata;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::{Mutex, OnceCell};
use tracing::warn;

static SCHEDULER: OnceCell<Arc<ToolScheduler>> = OnceCell::const_new();

// AnyApiTool trait + impl now live in types.rs for reuse

/// Runtime handle that may or may not own its runtime
enum SchedulerRuntime {
    /// Using an existing runtime (borrowed handle)
    Borrowed(tokio::runtime::Handle),
    /// We own this runtime (keeps it alive)
    Owned(tokio::runtime::Runtime),
}

impl SchedulerRuntime {
    /// Create a new SchedulerRuntime, owning a runtime in tests or when no runtime exists
    /// good for #[tokio::test(flavor = "multi_thread")]
    fn new() -> eyre::Result<Self> {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => Ok(Self::Borrowed(handle)),
            Err(_) => {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .thread_name("aomi-tool-scheduler")
                    .build()
                    .map_err(|err| eyre::eyre!("Failed to build tool scheduler runtime: {err}"))?;
                Ok(Self::Owned(rt))
            }
        }
    }

    /// Create a new owned rt, good for isolated test running without tokio
    fn new_owned() -> eyre::Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("aomi-tool-scheduler")
            .build()
            .map_err(|err| eyre::eyre!("Failed to build tool scheduler runtime: {err}"))?;
        Ok(Self::Owned(rt))
    }

    fn handle(&self) -> &tokio::runtime::Handle {
        match self {
            Self::Borrowed(h) => h,
            Self::Owned(rt) => rt.handle(),
        }
    }
}

/// Unified scheduler that manages session handlers and tool metadata
pub struct ToolScheduler {
    /// Metadata about registered tools (namespace, description, async support)
    tool_metadata: Arc<RwLock<HashMap<String, ToolMetadata>>>,
    /// Session handlers - one per active session
    session_handlers: Arc<RwLock<HashMap<String, Arc<Mutex<ToolHandler>>>>>,
    runtime: Arc<SchedulerRuntime>,
}

impl ToolScheduler {
    /// Create a new scheduler
    async fn new() -> Result<Self> {
        let runtime = SchedulerRuntime::new()?;

        // Initialize global external clients
        let clients = Arc::new(ExternalClients::new().await);
        init_external_clients(clients).await;

        let scheduler = ToolScheduler {
            tool_metadata: Arc::new(RwLock::new(HashMap::new())),
            session_handlers: Arc::new(RwLock::new(HashMap::new())),
            runtime: Arc::new(runtime),
        };

        Ok(scheduler)
    }

    pub async fn get_or_init() -> Result<Arc<ToolScheduler>> {
        let scheduler = SCHEDULER
            .get_or_try_init(|| async {
                let scheduler = Self::new().await?;
                Ok::<Arc<ToolScheduler>, eyre::Report>(Arc::new(scheduler))
            })
            .await?;

        Ok(scheduler.clone())
    }

    /// Helper to spawn an isolated scheduler on the current runtime without touching the global OnceCell.
    pub async fn new_for_test() -> Result<Arc<ToolScheduler>> {
        let runtime = SchedulerRuntime::new()?;
        let clients = Arc::new(ExternalClients::new_empty().await);
        init_external_clients(clients).await;

        let scheduler = Arc::new(ToolScheduler {
            tool_metadata: Arc::new(RwLock::new(HashMap::new())),
            session_handlers: Arc::new(RwLock::new(HashMap::new())),
            runtime: Arc::new(runtime),
        });

        Ok(scheduler)
    }

    /// Register an AomiTool with metadata
    pub fn register_tool<T>(&self, tool: T) -> Result<()>
    where
        T: Send + Sync + Clone + 'static,
        T: crate::AomiTool,
    {
        self.register_metadata(ToolMetadata::new(
            T::NAME.to_string(),
            T::NAMESPACE.to_string(),
            tool.description().to_string(),
            tool.support_async(),
        ))?;
        Ok(())
    }

    pub fn register_metadata(&self, meta: ToolMetadata) -> Result<()> {
        let mut metadata = self
            .tool_metadata
            .write()
            .map_err(|_| eyre::eyre!("Failed to acquire write lock"))?;
        metadata.insert(meta.name.clone(), meta);
        Ok(())
    }

    /// Get or create a session handler with namespace filtering
    ///
    /// This method manages per-session tool handlers with namespace-based tool filtering.
    /// Each session only has access to tools from specified namespaces.
    pub fn get_session_handler(
        &self,
        session_id: String,
        namespaces: Vec<String>,
    ) -> SessionToolHandler {
        let mut handlers = self.session_handlers.write().unwrap();

        if let Some(handler) = handlers.get(&session_id) {
            return Arc::clone(handler);
        }

        // Create new handler with filtered tool set
        let mut handler = ToolHandler::new(namespaces.clone());

        // Filter tools by namespace and populate available_tools
        let metadata_guard = self.tool_metadata.read().unwrap();

        for (name, meta) in metadata_guard.iter() {
            if namespaces.contains(&meta.namespace) {
                handler.avaliable_tools.insert(name.clone(), meta.clone());
            }
        }

        drop(metadata_guard);

        let handler_arc = Arc::new(Mutex::new(handler));
        handlers.insert(session_id.clone(), Arc::clone(&handler_arc));

        eprintln!(
            "Created new ToolHandler for session: {} with namespaces: {:?}",
            session_id, namespaces
        );
        handler_arc
    }

    /// Cleanup session when user logs off
    ///
    /// Phase 5: Now persists incomplete calls before removing from memory
    pub async fn cleanup_session(&self, session_id: &str) -> Result<Option<PersistedHandlerState>> {
        // Persist incomplete calls if handler exists
        let handler = {
            self.session_handlers.read().unwrap().get(session_id).cloned()
        };

        let persisted_state = if let Some(handler) = handler {
            let mut guard = handler.lock().await;
            Some(guard.sanitized_persist(3600).await?)
        } else {
            None
        };

        // Remove from memory
        self.session_handlers.write().unwrap().remove(session_id);
        eprintln!("Cleaned up session: {}", session_id);
        Ok(persisted_state)
    }

    /// Phase 5: Persist session handler state
    ///
    /// Polls all pending calls to completion and returns serialized state
    pub async fn persist_session(
        &self,
        session_id: &str,
        timeout_secs: u64,
    ) -> Result<Option<PersistedHandlerState>> {
        let handler = {
            self.session_handlers.read().unwrap().get(session_id).cloned()
        };

        if let Some(handler) = handler {
            let mut guard = handler.lock().await;
            Ok(Some(guard.sanitized_persist(timeout_secs).await?))
        } else {
            Ok(None)
        }
    }

    /// Phase 5: Restore session handler from persisted state
    ///
    /// Creates a new handler with the persisted state
    pub fn restore_session(
        &self,
        session_id: String,
        state: PersistedHandlerState,
    ) -> SessionToolHandler {
        let handler = ToolHandler::from_persisted(state);
        let handler_arc = Arc::new(Mutex::new(handler));
        self.session_handlers
            .write()
            .unwrap()
            .insert(session_id.clone(), Arc::clone(&handler_arc));
        eprintln!("Restored session: {}", session_id);
        handler_arc
    }

}

// ============================================================================
// ToolHandler - unified handler for both single and multi-step tools
// ============================================================================

pub type SessionToolHandler = Arc<Mutex<ToolHandler>>;

/// Persisted state for a ToolHandler session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedHandlerState {
    pub namespaces: Vec<String>,
    pub available_tools: HashMap<String, ToolMetadata>,
    pub completed_calls: Vec<ToolCompletion>,
}

/// Handler for managing tool execution lifecycle (2-phase: ongoing_calls → completed_calls)
pub struct ToolHandler {
    namespaces: Vec<String>,
    /// The allowed tool set of the requested namespace
    avaliable_tools: HashMap<String, ToolMetadata>,
    /// Active tool calls being polled for results
    ongoing_calls: Vec<ToolReciever>,
    /// Completed tool results ready for consumption
    completed_calls: Vec<ToolCompletion>,
}

impl ToolHandler {
    fn new(namespaces: Vec<String>) -> Self {
        Self {
            namespaces,
            avaliable_tools: HashMap::new(),
            ongoing_calls: Vec::new(),
            ongoing_streams: Vec::new(),
            completed_calls: Vec::new(),
        }
    }

    /// Register a tool receiver directly from V2 tools
    ///
    /// This method allows tools to self-schedule by directly registering their receivers.
    /// Used by the auto-impl in AomiToolWrapper.
    pub fn register_receiver(&mut self, receiver: ToolReciever) {
        self.ongoing_calls.push(receiver);
    }

    fn promote_ongoing(&mut self) {
        if self.ongoing_calls.is_empty() {
            return;
        }
        self.ongoing_streams
            .extend(self.ongoing_calls.drain(..));
    }

    /// Single-pass poll of all ongoing calls.
    /// Non-blocking: drains ready items into completed_calls, leaves pending calls.
    /// Returns number of newly completed items.
    pub fn poll_streams_once(&mut self) -> usize {
        use std::task::Poll;

        self.promote_ongoing();

        let mut count = 0;
        let mut i = 0;

        // Create a no-op waker for polling
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);

        while i < self.ongoing_streams.len() {
            let receiver = &mut self.ongoing_streams[i];
            let is_async = receiver.is_async();

            match receiver.poll_next(&mut cx) {
                Poll::Ready(Some((metadata, result))) => {
                    self.completed_calls.push(ToolCompletion {
                        metadata,
                        result,
                    });
                    count += 1;
                    if is_async {
                        i += 1;
                    } else {
                        self.ongoing_streams.swap_remove(i);
                    }
                }
                Poll::Ready(None) => {
                    self.ongoing_streams.swap_remove(i);
                }
                Poll::Pending => {
                    i += 1;
                }
            }
        }

        count
    }

    /// Drain and return all completed tool calls.
    /// Used by EventManager to push results to SystemEventQueue.
    pub fn take_completed_calls(&mut self) -> Vec<ToolCompletion> {
        std::mem::take(&mut self.completed_calls)
    }

    /// Check if there are completed calls ready to be consumed.
    pub fn has_completed_calls(&self) -> bool {
        !self.completed_calls.is_empty()
    }

    /// Check if there are any ongoing streams or pending calls
    pub fn has_ongoing_streams(&self) -> bool {
        !self.ongoing_streams.is_empty() || !self.ongoing_calls.is_empty()
    }

    /// Check if there are pending calls awaiting conversion to streams
    pub fn has_ongoing_calls(&self) -> bool {
        !self.ongoing_calls.is_empty()
    }

    /// Check if a tool uses multi-step results
    pub fn is_async(&self, tool_name: &str) -> bool {
        self.avaliable_tools
            .get(tool_name)
            .map(|meta| meta.is_async)
            .unwrap_or(false)
    }

    /// Get description for a tool (uses cached metadata)
    pub fn get_description(&self, tool_name: &str) -> String {
        self.avaliable_tools
            .get(tool_name)
            .map(|meta| meta.description.clone())
            .unwrap_or_else(|| tool_name.to_string())
    }


    /// Phase 5: Poll all pending calls to completion with timeout, then serialize state
    ///
    /// This method:
    /// 1. Promotes all pending calls to streams
    /// 2. Polls all streams until completion or timeout (300 seconds default)
    /// 3. Returns serialized state with only completed calls
    pub async fn sanitized_persist(&mut self, timeout_secs: u64) -> Result<PersistedHandlerState> {
        use tokio::time::{Duration, timeout};

        // Poll streams until completion or timeout
        let poll_result = timeout(Duration::from_secs(timeout_secs), async {
            loop {
                let completed_count = self.poll_streams_once();
                if completed_count == 0 && self.ongoing_streams.is_empty() {
                    break;
                }
                // Small delay between polls
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;

        if poll_result.is_err() {
            warn!(
                "Persistence timeout after {} seconds with {} ongoing streams",
                timeout_secs, self.ongoing_streams.len()
            );
        }

        // Return state with completed calls only
        Ok(PersistedHandlerState {
            namespaces: self.namespaces.clone(),
            available_tools: self.avaliable_tools.clone(),
            completed_calls: self.completed_calls.clone(),
        })
    }

    /// Phase 5: Restore handler from persisted state
    ///
    /// Creates a new handler with the persisted completed calls and tool metadata
    pub fn from_persisted(state: PersistedHandlerState) -> Self {
        Self {
            namespaces: state.namespaces,
            avaliable_tools: state.available_tools,
            ongoing_calls: Vec::new(),
            ongoing_streams: Vec::new(),
            completed_calls: state.completed_calls,
        }
    }

    /// Get persisted state without polling (for immediate serialization)
    pub fn to_persisted(&self) -> PersistedHandlerState {
        PersistedHandlerState {
            namespaces: self.namespaces.clone(),
            available_tools: self.avaliable_tools.clone(),
            completed_calls: self.completed_calls.clone(),
        }
    }
}
