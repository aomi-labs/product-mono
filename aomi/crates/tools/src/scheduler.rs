use crate::clients::{ExternalClients, init_external_clients};
use crate::streams::{ToolCompletion, ToolReciever, ToolStream};
use aomi_tools_v2::AomiTool;
use eyre::Result;
use futures::Stream;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
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

    fn new_for_test() -> eyre::Result<Self> {
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

    fn handle(&self) -> &tokio::runtime::Handle {
        match self {
            Self::Borrowed(h) => h,
            Self::Owned(rt) => rt.handle(),
        }
    }
}

/// Tool metadata for registration and filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub namespace: String,
    pub description: String,
    pub is_async: bool,
}

impl ToolMetadata {
    pub fn new(name: String, namespace: String, description: String, is_async: bool) -> Self {
        Self {
            name,
            namespace,
            description,
            is_async,
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
        let runtime = SchedulerRuntime::new_for_test()?;
        let clients = Arc::new(ExternalClients::new_for_test().await);
        init_external_clients(clients).await;

        let scheduler = Arc::new(ToolScheduler {
            tool_metadata: Arc::new(RwLock::new(HashMap::new())),
            session_handlers: Arc::new(RwLock::new(HashMap::new())),
            runtime: Arc::new(runtime),
        });

        Ok(scheduler)
    }

    /// Register a V2 AomiTool with metadata
    pub fn register_tool<T>(&self, tool: T) -> Result<()>
    where
        T: Send + Sync + Clone + 'static,
        T: aomi_tools_v2::AomiTool,
    {
        let mut metadata = self
            .tool_metadata
            .write()
            .map_err(|_| eyre::eyre!("Failed to acquire write lock"))?;
        metadata.insert(
            T::NAME.to_string(),
            ToolMetadata::new(
                T::NAME.to_string(),
                T::NAMESPACE.to_string(),
                tool.description().to_string(),
                tool.support_async(),
            ),
        );
        Ok(())
    }

    /// Get or create a session handler with namespace filtering
    ///
    /// This method manages per-session tool handlers with namespace-based tool filtering.
    /// Each session only has access to tools from specified namespaces.
    pub fn get_session_handler_with_namespaces(
        self: &Arc<Self>,
        session_id: String,
        namespaces: Vec<String>,
    ) -> SessionToolHander {
        let mut handlers = self.session_handlers.write().unwrap();

        if let Some(handler) = handlers.get(&session_id) {
            return Arc::clone(handler);
        }

        // Create new handler with filtered tool set
        let mut handler = ToolHandler::new();
        handler.namespaces = namespaces.clone();

        // Filter tools by namespace and populate available_tools
        let metadata_guard = self.tool_metadata.read().unwrap();

        for (name, meta) in metadata_guard.iter() {
            if namespaces.contains(&meta.namespace) {
                handler.available_tools.insert(name.clone(), meta.clone());
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
            Some(guard.sanitized_persist(3).await?)
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
    ) -> SessionToolHander {
        let handler = ToolHandler::from_persisted(state);
        let handler_arc = Arc::new(Mutex::new(handler));
        self.session_handlers
            .write()
            .unwrap()
            .insert(session_id.clone(), Arc::clone(&handler_arc));
        eprintln!("Restored session: {}", session_id);
        handler_arc
    }

    /// Register a tool in the scheduler
    pub fn register_tool<T>(&self, tool: T) -> Result<()>
    where
        T: AomiTool + Clone + 'static,
        T::ApiRequest: for<'de> Deserialize<'de> + Send + 'static,
        T::ApiResponse: Serialize + Send + 'static,
    {
        let tool_name = tool.name().to_string();
        let description = tool.description().to_string();

        // Register the tool itself
        let mut tools = self
            .tools
            .write()
            .map_err(|_| eyre::eyre!("Failed to acquire write lock"))?;
        tools.insert(tool_name.clone(), Arc::new(tool));
        drop(tools);

        // Register metadata (Phase 1 addition)
        let mut metadata = self
            .tool_metadata
            .write()
            .map_err(|_| eyre::eyre!("Failed to acquire write lock"))?;
        metadata.insert(
            tool_name.clone(),
            ToolMetadata::new(
                tool_name,
                "default".to_string(), // TODO: Add namespace parameter in future
                description,
                false, // Regular tools are not async
            ),
        );

        Ok(())
    }
}

// ============================================================================
// ToolHandler - unified handler for both single and multi-step tools
// ============================================================================

pub type SessionToolHander = Arc<Mutex<ToolHandler>>;

/// Persisted state for a ToolHandler session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedHandlerState {
    pub namespaces: Vec<String>,
    pub available_tools: HashMap<String, ToolMetadata>,
    pub completed_calls: Vec<ToolCompletion>,
}

/// Handler for managing tool execution lifecycle
pub struct ToolHandler {
    namespaces: Vec<String>,
    /// The allowed tool set of the requested namespace
    available_tools: HashMap<String, ToolMetadata>,
    /// Unresolved tool calls (receivers not yet converted to streams)
    unresolved_calls: Vec<ToolReciever>,
    /// Ongoing streams being polled
    ongoing_streams: Vec<ToolStream>,
    /// Completed tool results ready for consumption
    completed_calls: Vec<ToolCompletion>,
}

impl ToolHandler {
    fn new() -> Self {
        Self {
            namespaces: Vec::new(),
            available_tools: HashMap::new(),
            unresolved_calls: Vec::new(),
            ongoing_streams: Vec::new(),
            completed_calls: Vec::new(),
        }
    }

    /// Register a tool receiver directly from V2 tools
    ///
    /// This method allows tools to self-schedule by directly registering their receivers.
    /// Used by the auto-impl in AomiToolWrapper.
    pub fn register_receiver(&mut self, receiver: ToolReciever) {
        self.unresolved_calls.push(receiver);
    }

    /// Convert any unresolved calls into pollable streams.
    /// Returns Some if work was done, None otherwise.
    pub async fn resolve_calls(&mut self) -> Option<Vec<ToolStream>> {
        if self.unresolved_calls.is_empty() {
            return None;
        }
        let mut ui_streams = Vec::new();
        while let Some(mut receiver) = self.unresolved_calls.pop() {
            let (bg_stream, ui_stream) = receiver.into_shared_streams();
            self.add_ongoing_stream(bg_stream);
            ui_streams.push(ui_stream);
        }
        if !self.ongoing_streams.is_empty() {
            Some(ui_streams)
        } else {
            None
        }
    }

    /// Pop the most recent unresolved call, convert to streams
    /// add bg stream to ongoing_stream while returning the ui stream
    pub fn resolve_last_call(&mut self) -> Option<ToolStream> {
        let mut receiver = self.unresolved_calls.pop()?;
        let (bg_stream, ui_stream) = receiver.into_shared_streams();
        self.add_ongoing_stream(bg_stream);
        Some(ui_stream)
    }

    /// Get reference to unresolved_calls
    pub fn unresolved_calls(&self) -> &Vec<ToolReciever> {
        &self.unresolved_calls
    }

    /// Get mutable reference to ongoing_streams for finalization
    pub fn ongoing_streams_mut(&mut self) -> &mut Vec<ToolStream> {
        &mut self.ongoing_streams
    }

    /// Single-pass poll of all ongoing streams.
    /// Non-blocking: drains ready items into completed_calls, leaves pending streams.
    /// Returns number of newly completed items.
    pub fn poll_streams_once(&mut self) -> usize {
        use std::task::Poll;

        let mut count = 0;
        let mut i = 0;

        // Create a no-op waker for polling
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);

        while i < self.ongoing_streams.len() {
            let stream = &mut self.ongoing_streams[i];
            let tool_name = stream.tool_name.clone();
            let is_multi_step = stream.is_multi_step();
            let is_first_chunk = is_multi_step && !stream.first_chunk_sent;

            match Pin::new(&mut *stream).poll_next(&mut cx) {
                Poll::Ready(Some((call_id, result))) => {
                    let result = result;
                    if is_first_chunk {
                        stream.first_chunk_sent = true;
                    }
                    self.completed_calls.push(ToolCompletion {
                        call_id,
                        tool_name,
                        sync: !is_multi_step || is_first_chunk,
                        result,
                    });
                    count += 1;
                    if is_multi_step {
                        // Keep stream alive for follow-up chunks
                        i += 1;
                    } else {
                        // Single-step: stream consumed
                        self.ongoing_streams.swap_remove(i);
                    }
                }
                Poll::Ready(None) => {
                    // Stream exhausted, remove it
                    self.ongoing_streams.swap_remove(i);
                    // Don't increment i
                }
                Poll::Pending => {
                    // Stream not ready, move to next
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

    /// Async version: polls once, yields if nothing ready, returns next completion.
    /// Used by tests and legacy code. For background poller, use poll_streams_once().
    pub async fn poll_streams(&mut self) -> Option<ToolCompletion> {
        loop {
            if self.ongoing_streams.is_empty() && self.completed_calls.is_empty() {
                return None;
            }

            // First drain any already-completed calls
            if let Some(completion) = self.completed_calls.pop() {
                return Some(completion);
            }

            // Then try to poll for new completions
            let count = self.poll_streams_once();
            if count > 0 {
                // New completions available, return one
                return self.completed_calls.pop();
            }

            if self.ongoing_streams.is_empty() {
                return None;
            }

            // Yield to allow other tasks to run
            tokio::task::yield_now().await;
        }
    }

    /// Check if there are any ongoing streams or unresolved calls
    pub fn has_ongoing_streams(&self) -> bool {
        !self.ongoing_streams.is_empty() || !self.unresolved_calls.is_empty()
    }

    /// Check if there are unresolved calls awaiting conversion to streams
    pub fn has_unresolved_calls(&self) -> bool {
        !self.unresolved_calls.is_empty()
    }

    /// Add an external stream to ongoing_streams (for agent tools not in scheduler)
    pub fn add_ongoing_stream(&mut self, stream: ToolStream) {
        self.ongoing_streams.push(stream);
    }

    /// Check if a tool uses multi-step results
    pub fn is_multi_step(&self, tool_name: &str) -> bool {
        self.available_tools
            .get(tool_name)
            .map(|meta| meta.is_async)
            .unwrap_or(false)
    }

    /// Get topic for a tool (uses cached metadata)
    pub fn get_topic(&self, tool_name: &str) -> String {
        self.available_tools
            .get(tool_name)
            .map(|meta| meta.description.clone())
            .unwrap_or_else(|| tool_name.to_string())
    }

    #[cfg(test)]
    pub(crate) fn test_set_tool_metadata(&mut self, name: &str, is_async: bool, description: &str) {
        self.available_tools.insert(
            name.to_string(),
            ToolMetadata::new(name.to_string(), "test".to_string(), description.to_string(), is_async),
        );
    }

    /// Phase 5: Poll all pending calls to completion with timeout, then serialize state
    ///
    /// This method:
    /// 1. Resolves all unresolved calls to streams
    /// 2. Polls all streams until completion or timeout (3 seconds default)
    /// 3. Returns serialized state with only completed calls
    pub async fn sanitized_persist(&mut self, timeout_secs: u64) -> Result<PersistedHandlerState> {
        use tokio::time::{Duration, timeout};

        // First, resolve any unresolved calls
        let _ = self.resolve_calls().await;

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
                timeout_secs,
                self.ongoing_streams.len()
            );
        }

        // Return state with completed calls only
        Ok(PersistedHandlerState {
            namespaces: self.namespaces.clone(),
            available_tools: self.available_tools.clone(),
            completed_calls: self.completed_calls.clone(),
        })
    }

    /// Phase 5: Restore handler from persisted state
    ///
    /// Creates a new handler with the persisted completed calls and tool metadata
    pub fn from_persisted(state: PersistedHandlerState) -> Self {
        Self {
            namespaces: state.namespaces,
            available_tools: state.available_tools,
            unresolved_calls: Vec::new(),
            ongoing_streams: Vec::new(),
            completed_calls: state.completed_calls,
        }
    }

    /// Get persisted state without polling (for immediate serialization)
    pub fn to_persisted(&self) -> PersistedHandlerState {
        PersistedHandlerState {
            namespaces: self.namespaces.clone(),
            available_tools: self.available_tools.clone(),
            completed_calls: self.completed_calls.clone(),
        }
    }
}
