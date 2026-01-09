use crate::clients::{ExternalClients, init_external_clients};
use crate::streams::{
    SchedulerRequest, ToolCompletion, ToolReciever, ToolResultSender, ToolStream,
};
use crate::types::{AnyTool, AomiTool, AsyncTool, AsyncToolWrapper};
use eyre::Result;
use futures::Stream;
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use tokio::sync::{Mutex, OnceCell, mpsc, oneshot};
use tracing::{debug, error, warn};

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

/// Unified scheduler that can handle any registered API tool
pub struct ToolScheduler {
    tools: Arc<RwLock<HashMap<String, Arc<dyn AnyTool>>>>,
    /// Unified channel - sender type determines oneshot vs mpsc handling
    requests_tx: mpsc::Sender<(SchedulerRequest, ToolResultSender)>,
    runtime: Arc<SchedulerRuntime>,
}

impl ToolScheduler {
    /// Create a new typed scheduler with tool registry
    #[allow(clippy::type_complexity)]
    async fn new() -> Result<(Self, mpsc::Receiver<(SchedulerRequest, ToolResultSender)>)> {
        let (requests_tx, requests_rx) = mpsc::channel(100);
        let runtime = SchedulerRuntime::new()?;

        // Initialize global external clients
        let clients = Arc::new(ExternalClients::new().await);
        init_external_clients(clients).await;

        let scheduler = ToolScheduler {
            tools: Arc::new(RwLock::new(HashMap::new())),
            requests_tx,
            runtime: Arc::new(runtime),
        };

        Ok((scheduler, requests_rx))
    }

    pub async fn get_or_init() -> Result<Arc<ToolScheduler>> {
        let scheduler = SCHEDULER
            .get_or_try_init(|| async {
                let (scheduler, requests_rx) = Self::new().await?;
                let scheduler = Arc::new(scheduler);
                // Start the scheduler's event loop in the background
                Self::run(scheduler.clone(), requests_rx);
                Ok::<Arc<ToolScheduler>, eyre::Report>(scheduler)
            })
            .await?;

        Ok(scheduler.clone())
    }

    /// Helper to spawn an isolated scheduler on the current runtime without touching the global OnceCell.
    pub async fn new_for_test() -> Result<Arc<ToolScheduler>> {
        let (requests_tx, requests_rx) = mpsc::channel(100);

        let runtime = SchedulerRuntime::new_for_test()?;
        let clients = Arc::new(ExternalClients::new_for_test().await);
        init_external_clients(clients).await;

        let scheduler = Arc::new(ToolScheduler {
            tools: Arc::new(RwLock::new(HashMap::new())),
            requests_tx,
            runtime: Arc::new(runtime),
        });

        // Start the scheduler loop on the existing runtime
        Self::run(scheduler.clone(), requests_rx);
        Ok(scheduler)
    }

    /// Register a multi-step tool that streams chunks over time.
    pub fn register_multi_step_tool<T>(&self, tool: T) -> Result<()>
    where
        T: AsyncTool + Clone + 'static,
    {
        let tool_name = tool.name().to_string();
        let wrapper = AsyncToolWrapper { inner: tool };

        let mut tools = self
            .tools
            .write()
            .map_err(|_| eyre::eyre!("Failed to acquire write lock"))?;
        tools.insert(tool_name, Arc::new(wrapper));
        Ok(())
    }

    pub fn get_handler(self: &Arc<Self>) -> ToolHandler {
        let mut handler = ToolHandler::new(self.requests_tx.clone());
        // Pre-populate the cache with current tools
        let tools_guard = self.tools.read().unwrap();
        for (name, tool) in tools_guard.iter() {
            let multi_steps = tool.multi_steps();
            let static_topic = tool.static_topic().to_string();
            handler
                .tool_info
                .insert(name.clone(), (multi_steps, static_topic));
        }
        handler
    }

    /// Register a tool in the scheduler
    pub fn register_tool<T>(&self, tool: T) -> Result<()>
    where
        T: AomiTool + Clone + 'static,
        T::ApiRequest: for<'de> Deserialize<'de> + Send + 'static,
        T::ApiResponse: Serialize + Send + 'static,
    {
        let tool_name = tool.name().to_string();
        let mut tools = self
            .tools
            .write()
            .map_err(|_| eyre::eyre!("Failed to acquire write lock"))?;
        tools.insert(tool_name, Arc::new(tool));
        Ok(())
    }

    /// Register a tool that already implements AnyApiTool (escape hatch).
    pub fn register_any_tool(&self, tool: Arc<dyn AnyTool>) -> Result<()> {
        let tool_name = tool.tool().to_string();
        let mut tools = self
            .tools
            .write()
            .map_err(|_| eyre::eyre!("Failed to acquire write lock"))?;
        tools.insert(tool_name, tool);
        Ok(())
    }

    /// Spawn the scheduler loop in the background
    fn run(
        scheduler: Arc<Self>,
        mut requests_rx: mpsc::Receiver<(SchedulerRequest, ToolResultSender)>,
    ) {
        let tools = scheduler.tools.clone();
        let runtime = scheduler.runtime.clone();

        runtime.handle().spawn(async move {
            debug!("Starting tool scheduler event loop");
            let mut jobs = FuturesUnordered::new();
            let mut channel_closed = false;

            loop {
                tokio::select! {
                    // Accept new request if available
                    maybe_req = requests_rx.recv(), if !channel_closed => {
                        match maybe_req {
                            Some((request, reply_tx)) => {
                                let tools = tools.clone();

                                // Each request becomes an async job
                                jobs.push(async move {
                                    // Get the tool first, outside the async block
                                    let tool_option = {
                                        let tools_guard = tools.read().unwrap();
                                        tools_guard.get(&request.tool_name).cloned()
                                    }; // Guard is dropped here

                                    if let Some(tool) = tool_option {
                                        if tool.validate_json(&request.payload).is_ok() {
                                            // Match on sender type to determine execution path
                                            match reply_tx {
                                                ToolResultSender::Oneshot(tx) => {
                                                    // Single result: execute and send
                                                    let result = tool.call_with_json(request.payload).await;
                                                    if tx.send(result).is_err() {
                                                        warn!("Failed to send tool result - receiver dropped");
                                                    }
                                                }
                                                ToolResultSender::MultiStep(tx) => {
                                                    // Multi-step: tool owns sender, sends multiple results
                                                    if let Err(e) = tool.call_with_sender(request.payload, tx).await {
                                                        warn!("Multi-step tool execution failed: {}", e);
                                                    }
                                                    // Tool is responsible for sending results and closing channel
                                                }
                                            }
                                        } else {
                                            // Validation failed
                                            let err = Err(eyre::eyre!("Request validation failed"));
                                            match reply_tx {
                                                ToolResultSender::Oneshot(tx) => { let _ = tx.send(err); }
                                                ToolResultSender::MultiStep(tx) => { let _ = tx.send(err).await; }
                                            }
                                        }
                                    } else {
                                        // Unknown tool
                                        warn!("Unknown tool requested: {}", request.tool_name);
                                        let err = Err(eyre::eyre!("Unknown tool: {}", request.tool_name));
                                        match reply_tx {
                                            ToolResultSender::Oneshot(tx) => { let _ = tx.send(err); }
                                            ToolResultSender::MultiStep(tx) => { let _ = tx.send(err).await; }
                                        }
                                    }
                                });
                            }
                            None => {
                                debug!("Tool scheduler request channel closed");
                                channel_closed = true;
                                if jobs.is_empty() {
                                    break;
                                }
                            }
                        }
                    }
                    // Process completed requests
                    Some(_) = jobs.next(), if !jobs.is_empty() => {
                        // FuturesUnordered automatically drives concurrency
                    }
                    else => {
                        if channel_closed && jobs.is_empty() {
                            debug!("Tool scheduler shutting down - no more requests");
                            break;
                        }
                    },
                }
            }
            debug!("Tool scheduler event loop terminated");
        });
    }

    /// Get list of registered tools
    pub fn list_tools(&self) -> Vec<(String, String)> {
        self.tools
            .read()
            .unwrap()
            .iter()
            .map(|(name, tool)| (name.clone(), tool.description().to_string()))
            .collect()
    }

    /// Get list of registered tools
    pub fn list_tool_names(&self) -> Vec<String> {
        self.tools.read().unwrap().keys().cloned().collect()
    }

    /// Get static topic for a tool by name
    pub fn get_topic(&self, tool_name: &str) -> String {
        self.tools
            .read()
            .unwrap()
            .get(tool_name)
            .map(|tool| tool.static_topic().to_string())
            .unwrap_or_else(|| tool_name.to_string())
    }

    pub fn validate_multi_step_result(
        &self,
        tool_name: &str,
        value: &Value,
    ) -> eyre::Result<Value> {
        let tools = self.tools.read().map_err(|e| eyre::eyre!(e.to_string()))?;
        if let Some(tool) = tools.get(tool_name) {
            tool.validate_async_result(value)
        } else {
            Ok(value.clone())
        }
    }
}

// ============================================================================
// ToolHandler - unified handler for both single and multi-step tools
// ============================================================================

pub type SessionToolHander = Arc<Mutex<ToolHandler>>;

/// Handler for sending requests to the scheduler
pub struct ToolHandler {
    /// Unified channel for all tool requests
    requests_tx: mpsc::Sender<(SchedulerRequest, ToolResultSender)>,
    /// Unresolved tool calls before conversion to streams (internal channel handles)
    unresolved_calls: Vec<ToolReciever>,
    /// Ongoing streams to poll for results (converted from unresolved calls, ready for UI presentation)
    ongoing_streams: Vec<ToolStream>,
    /// Completed tool results ready to be consumed by EventManager
    completed_calls: Vec<ToolCompletion>,
    /// Cache for tool metadata: tool_name -> (multi_steps, static_topic)
    tool_info: HashMap<String, (bool, String)>,
}

impl ToolHandler {
    fn new(requests_tx: mpsc::Sender<(SchedulerRequest, ToolResultSender)>) -> Self {
        Self {
            requests_tx,
            unresolved_calls: Vec::new(),
            ongoing_streams: Vec::new(),
            completed_calls: Vec::new(),
            tool_info: HashMap::new(),
        }
    }

    /// Schedule a typed request that preserves type safety
    pub async fn request_typed<T>(
        &mut self,
        tool: &T,
        request: T::ApiRequest,
    ) -> oneshot::Receiver<Result<T::ApiResponse>>
    where
        T: AomiTool + Clone,
        T::ApiRequest: Serialize,
        T::ApiResponse: for<'de> Deserialize<'de> + 'static,
    {
        let (tx, rx) = oneshot::channel();

        // Serialize the request to JSON
        let payload = serde_json::to_value(request).unwrap();

        let scheduler_request = SchedulerRequest {
            tool_name: tool.name().to_string(),
            payload,
        };

        // Send through the channel with oneshot sender
        let (internal_tx, internal_rx) = oneshot::channel();
        let _ = self
            .requests_tx
            .send((scheduler_request, ToolResultSender::Oneshot(internal_tx)))
            .await;

        // Convert response back to typed result
        tokio::spawn(async move {
            match internal_rx.await {
                Ok(Ok(json_response)) => {
                    let result: Result<T::ApiResponse> =
                        serde_json::from_value(json_response).map_err(Into::into);
                    let _ = tx.send(result);
                }
                Ok(Err(error)) => {
                    let _ = tx.send(Err(error));
                }
                Err(_) => {
                    let _ = tx.send(Err(eyre::eyre!("Channel closed")));
                }
            }
        });
        rx
    }

    /// Unified request method - handles both single and multi-step tools
    /// Enqueues a ToolReciever to unresolved_calls for later conversion
    pub async fn request(&mut self, tool_name: String, payload: Value, call_id: String) {
        let is_multi_step = self
            .tool_info
            .get(&tool_name)
            .map(|(multi, _)| *multi)
            .unwrap_or(false);

        let request = SchedulerRequest {
            tool_name: tool_name.clone(),
            payload,
        };

        let tool_future = if is_multi_step {
            let rx = self.send_multi_step_request(request).await;
            ToolReciever::new_multi_step(call_id.clone(), tool_name.clone(), rx)
        } else {
            let rx = self.send_oneshot_request(request).await;
            ToolReciever::new_single(call_id.clone(), tool_name.clone(), rx)
        };

        // Enqueue unresolved call - caller will retrieve and convert to streams
        self.unresolved_calls.push(tool_future);
    }

    async fn send_oneshot_request(
        &self,
        request: SchedulerRequest,
    ) -> oneshot::Receiver<Result<Value>> {
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self
            .requests_tx
            .send((request, ToolResultSender::Oneshot(tx)))
            .await
        {
            error!("Failed to send request to scheduler: {}", e);
        }
        rx
    }

    async fn send_multi_step_request(
        &self,
        request: SchedulerRequest,
    ) -> mpsc::Receiver<Result<Value>> {
        let (tx, rx) = mpsc::channel::<Result<Value>>(100);

        if let Err(e) = self
            .requests_tx
            .send((request, ToolResultSender::MultiStep(tx)))
            .await
        {
            error!("Failed to send request to scheduler: {}", e);
        }
        rx
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
                    let result = if is_multi_step {
                        match &result {
                            Ok(value) => Self::validate_multi_step_result(&tool_name, value)
                                .map_err(|e| e.to_string()),
                            Err(err) => Err(err.clone()),
                        }
                    } else {
                        result
                    };
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
        self.tool_info
            .get(tool_name)
            .map(|(multi, _)| *multi)
            .unwrap_or(false)
    }

    /// Get topic for a tool (uses cached metadata)
    pub fn get_topic(&self, tool_name: &str) -> String {
        self.tool_info
            .get(tool_name)
            .map(|(_, topic)| topic.clone())
            .unwrap_or_else(|| tool_name.to_string())
    }

    fn validate_multi_step_result(tool_name: &str, value: &Value) -> eyre::Result<Value> {
        if let Some(scheduler) = SCHEDULER.get() {
            scheduler.validate_multi_step_result(tool_name, value)
        } else {
            Ok(value.clone())
        }
    }

    #[cfg(test)]
    pub(crate) fn test_set_tool_metadata(&mut self, name: &str, is_multi_step: bool, topic: &str) {
        self.tool_info
            .insert(name.to_string(), (is_multi_step, topic.to_string()));
    }
}
