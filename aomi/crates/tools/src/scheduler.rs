use crate::clients::{ExternalClients, init_external_clients};
use crate::types::{AnyApiTool, AomiApiTool};
use eyre::Result;
use futures::Stream;
use futures::future::{BoxFuture, FutureExt, Shared};
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use tokio::sync::{OnceCell, mpsc, oneshot};
use tracing::{debug, error, warn};

// ============================================================================
// Channel Types for Tool Results
// ============================================================================

/// Sender side for tool results - either oneshot (single result) or mpsc (multi-step)
pub enum ToolResultSender {
    /// Single result - low overhead, for most tools
    Oneshot(oneshot::Sender<Result<Value>>),
    /// Multi-step results - tool owns this and sends multiple chunks
    MultiStep(mpsc::Sender<Result<Value>>),
}

/// Receiver side for tool results
pub enum ToolResultReceiver {
    /// Single result receiver
    Oneshot(oneshot::Receiver<Result<Value>>),
    /// Multi-step receiver - yields multiple results over time
    MultiStep(mpsc::Receiver<Result<Value>>),
}

// ============================================================================
// Unified ToolResultFuture - handles both single and multi-step results
// ============================================================================

/// Unified future for tool results - supports both oneshot and multi-step tools
pub enum ToolResultFuture {
    /// Single result - uses shared future so both pending_results and stream can access
    Single {
        tool_call_id: String,
        future: Shared<BoxFuture<'static, (String, Result<Value, String>)>>,
    },
    /// Multi-step - receiver stays here, first chunk sent via oneshot
    MultiStep {
        tool_call_id: String,
        finished: bool,
        receiver: mpsc::Receiver<Result<Value>>,
        first_chunk_tx: Option<oneshot::Sender<Result<Value>>>,
    },
}

impl Debug for ToolResultFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Single { tool_call_id, .. } => {
                write!(f, "ToolResultFuture::Single({})", tool_call_id)
            }
            Self::MultiStep { tool_call_id, finished, first_chunk_tx, .. } => {
                write!(f, "ToolResultFuture::MultiStep({}, finished={}, first_pending={})",
                    tool_call_id, finished, first_chunk_tx.is_some())
            }
        }
    }
}

impl ToolResultFuture {
    pub fn tool_call_id(&self) -> &str {
        match self {
            Self::Single { tool_call_id, .. } => tool_call_id,
            Self::MultiStep { tool_call_id, .. } => tool_call_id,
        }
    }

    pub fn is_finished(&self) -> bool {
        match self {
            Self::Single { .. } => false, // Single futures are polled once
            Self::MultiStep { finished, .. } => *finished,
        }
    }

    /// Get a cloneable future for Single variant (for ToolResultStream)
    pub fn get_shared_future(&self) -> Option<Shared<BoxFuture<'static, (String, Result<Value, String>)>>> {
        match self {
            Self::Single { future, .. } => Some(future.clone()),
            Self::MultiStep { .. } => None,
        }
    }

    /// Check if first chunk has been taken (sent to oneshot)
    pub fn first_chunk_taken(&self) -> bool {
        match self {
            Self::Single { .. } => true, // Single always "taken"
            Self::MultiStep { first_chunk_tx, .. } => first_chunk_tx.is_none(),
        }
    }
}

impl Future for ToolResultFuture {
    type Output = (String, Result<Value, String>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this {
            Self::Single { tool_call_id: _, future } => {
                // Poll the shared future
                let pinned = Pin::new(future);
                pinned.poll(cx)
            }
            Self::MultiStep { tool_call_id, finished, receiver, first_chunk_tx } => {
                match receiver.poll_recv(cx) {
                    Poll::Ready(Some(result)) => {
                        // If first chunk sender exists, send result there (for UI ACK)
                        // Convert to same type as stream output since eyre::Report isn't Clone
                        if let Some(tx) = first_chunk_tx.take() {
                            let first_result = result.as_ref()
                                .map(|v| v.clone())
                                .map_err(|e| eyre::eyre!(e.to_string()));
                            let _ = tx.send(first_result);
                        }
                        match result {
                            Ok(value) => {
                                // Tool layer sets "finished" flag in result JSON
                                *finished = value.get("finished")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                Poll::Ready((tool_call_id.clone(), Ok(value)))
                            }
                            Err(e) => {
                                *finished = true; // Error means we're done
                                Poll::Ready((tool_call_id.clone(), Err(e.to_string())))
                            }
                        }
                    }
                    Poll::Ready(None) => {
                        // Channel closed
                        *finished = true;
                        Poll::Ready((tool_call_id.clone(), Ok(Value::Null)))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}


// ============================================================================
// ToolResultStream - yields first chunk for UI ACK
// ============================================================================

/// Stream for the first result chunk - used for UI ACK via ChatCommand::ToolCall
/// This is a single-item stream that yields once and then completes.
pub struct ToolResultStream {
    future: Option<BoxFuture<'static, (String, Result<Value, String>)>>,
}

impl Debug for ToolResultStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ToolResultStream")
    }
}

impl Stream for ToolResultStream {
    type Item = (String, Result<Value, String>);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.future.as_mut() {
            Some(fut) => {
                // SAFETY: We're not moving the future, just polling it
                let pinned = unsafe { Pin::new_unchecked(fut) };
                match pinned.poll(cx) {
                    Poll::Ready(result) => {
                        this.future = None; // Mark as completed
                        Poll::Ready(Some(result))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
            None => Poll::Ready(None), // Already completed
        }
    }
}

impl ToolResultStream {
    /// Create an empty stream for testing
    pub fn empty() -> Self {
        Self { future: Some(async { ("".to_string(), Ok(Value::Null)) }.boxed()) }
    }

    /// Create a test stream with custom data
    pub fn from_result(call_id: String, result: Result<Value, String>) -> Self {
        Self { future: Some(async move { (call_id, result) }.boxed()) }
    }

    /// Create from a shared future (for single-result tools)
    pub fn from_shared(future: Shared<BoxFuture<'static, (String, Result<Value, String>)>>) -> Self {
        Self { future: Some(async move { future.await }.boxed()) }
    }

    /// Create from a boxed future
    pub fn from_future(future: BoxFuture<'static, (String, Result<Value, String>)>) -> Self {
        Self { future: Some(future) }
    }
}

static SCHEDULER: OnceCell<Arc<ToolScheduler>> = OnceCell::const_new();

/// Type-erased request that can hold any tool request as JSON
#[derive(Debug, Clone)]
pub struct SchedulerRequest {
    pub tool_name: String,
    pub payload: Value,
}

// AnyApiTool trait + impl now live in types.rs for reuse

/// Unified scheduler that can handle any registered API tool
pub struct ToolScheduler {
    tools: Arc<RwLock<HashMap<String, Arc<dyn AnyApiTool>>>>,
    /// Unified channel - sender type determines oneshot vs mpsc handling
    requests_tx: mpsc::Sender<(SchedulerRequest, ToolResultSender)>,
    runtime: Arc<tokio::runtime::Handle>,
    // Keep an owned runtime alive when we had to create one ourselves
    _runtime_guard: Option<Arc<tokio::runtime::Runtime>>,
    #[allow(dead_code)]
    clients: Arc<ExternalClients>,
}

impl ToolScheduler {
    /// Create a new typed scheduler with tool registry
    #[allow(clippy::type_complexity)]
    async fn new() -> Result<(
        Self,
        mpsc::Receiver<(SchedulerRequest, ToolResultSender)>,
    )> {
        let (requests_tx, requests_rx) = mpsc::channel(100);
        let (runtime, runtime_guard) = match tokio::runtime::Handle::try_current() {
            Ok(handle) => (Arc::new(handle), None),
            Err(_) => {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .thread_name("aomi-tool-scheduler")
                    .build()
                    .map_err(|err| eyre::eyre!("Failed to build tool scheduler runtime: {err}"))?;
                let handle = rt.handle().clone();
                (Arc::new(handle), Some(Arc::new(rt)))
            }
        };
        let clients = Arc::new(ExternalClients::new().await);
        init_external_clients(clients.clone()).await;

        let scheduler = ToolScheduler {
            tools: Arc::new(RwLock::new(HashMap::new())),
            requests_tx,
            runtime,
            _runtime_guard: runtime_guard,
            clients,
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

    pub fn get_handler(&self) -> ToolApiHandler {
        let mut handler = ToolApiHandler::new(self.requests_tx.clone());
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
        T: AomiApiTool + Clone + 'static,
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

    /// Spawn the scheduler loop in the background
    fn run(
        scheduler: Arc<Self>,
        mut requests_rx: mpsc::Receiver<(SchedulerRequest, ToolResultSender)>,
    ) {
        let tools = scheduler.tools.clone();
        let runtime = scheduler.runtime.clone();

        runtime.spawn(async move {
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
                                        if tool.validate_json(&request.payload) {
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
}

// ============================================================================
// ToolApiHandler - unified handler for both single and multi-step tools
// ============================================================================

/// Handler for sending requests to the scheduler
pub struct ToolApiHandler {
    /// Unified channel for all tool requests
    requests_tx: mpsc::Sender<(SchedulerRequest, ToolResultSender)>,
    /// Pending single-result futures
    pending_results: FuturesUnordered<ToolResultFuture>,
    /// Pending multi-step futures (polled separately, re-added after each result)
    multi_step_futures: Vec<ToolResultFuture>,
    /// Collected finished results for finalize_tool_results
    finished_results: Vec<(String, Result<Value>)>,
    /// Cache for tool metadata: tool_name -> (multi_steps, static_topic)
    tool_info: HashMap<String, (bool, String)>,
}

impl ToolApiHandler {
    fn new(requests_tx: mpsc::Sender<(SchedulerRequest, ToolResultSender)>) -> Self {
        Self {
            requests_tx,
            pending_results: FuturesUnordered::new(),
            multi_step_futures: Vec::new(),
            finished_results: Vec::new(),
            tool_info: HashMap::new(),
        }
    }

    /// Schedule a typed request that preserves type safety
    pub async fn request<T>(
        &mut self,
        tool: &T,
        request: T::ApiRequest,
    ) -> oneshot::Receiver<Result<T::ApiResponse>>
    where
        T: AomiApiTool + Clone,
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
    /// Returns a ToolResultStream for the first chunk (ACK) and adds to pending_results
    pub async fn request_with_json_stream(
        &mut self,
        tool_name: String,
        payload: Value,
        tool_call_id: String,
    ) -> ToolResultStream {
        let is_multi_step = self.tool_info
            .get(&tool_name)
            .map(|(multi, _)| *multi)
            .unwrap_or(false);

        let request = SchedulerRequest {
            tool_name: tool_name.clone(),
            payload,
        };

        if is_multi_step {
            // Multi-step tool: use mpsc channel
            let (tx, rx) = mpsc::channel::<Result<Value>>(100);

            if let Err(e) = self.requests_tx.send((request, ToolResultSender::MultiStep(tx))).await {
                error!("Failed to send request to scheduler: {}", e);
            }

            // Oneshot for first chunk - will be filled by first poll_next_result
            let (first_tx, first_rx) = oneshot::channel();

            // Add to multi_step_futures (separate from FuturesUnordered)
            self.multi_step_futures.push(ToolResultFuture::MultiStep {
                tool_call_id: tool_call_id.clone(),
                finished: false,
                receiver: rx,
                first_chunk_tx: Some(first_tx),
            });

            // Return stream that waits on first chunk
            let call_id = tool_call_id.clone();
            ToolResultStream::from_future(async move {
                match first_rx.await {
                    Ok(result) => (call_id, result.map_err(|e| e.to_string())),
                    Err(_) => (call_id, Err("Channel closed".to_string())),
                }
            }.boxed())
        } else {
            // Single-result tool: use oneshot channel with shared future
            let (tx, rx) = oneshot::channel();

            if let Err(e) = self.requests_tx.send((request, ToolResultSender::Oneshot(tx))).await {
                error!("Failed to send request to scheduler: {}", e);
            }

            // Create shared future for both pending_results and stream
            let tool_call_id_clone = tool_call_id.clone();
            let shared_future = async move {
                match rx.await {
                    Ok(result) => (tool_call_id_clone, result.map_err(|e| e.to_string())),
                    Err(e) => (tool_call_id_clone, Err(e.to_string())),
                }
            }
            .boxed()
            .shared();

            // Add to pending_results
            self.pending_results.push(ToolResultFuture::Single {
                tool_call_id: tool_call_id.clone(),
                future: shared_future.clone(),
            });

            // Return stream from shared future
            ToolResultStream::from_shared(shared_future)
        }
    }

    /// Poll for the next completed tool result and add it to finished_results
    /// Returns None if no results ready
    pub async fn poll_next_result(&mut self) -> Option<()> {
        use std::future::poll_fn;
        use std::task::Poll;

        // Poll both single-result futures and multi-step futures
        poll_fn(|cx| {
            // First, poll FuturesUnordered for single-result tools
            if !self.pending_results.is_empty() {
                if let Poll::Ready(Some((call_id, result))) = self.pending_results.poll_next_unpin(cx) {
                    self.finished_results
                        .push((call_id, result.map_err(|e| eyre::eyre!(e))));
                    return Poll::Ready(Some(()));
                }
            }

            // Then poll multi-step futures
            for i in 0..self.multi_step_futures.len() {
                let fut = &mut self.multi_step_futures[i];
                // SAFETY: We're not moving the future, just polling it in place
                let pinned = unsafe { Pin::new_unchecked(fut) };
                if let Poll::Ready((call_id, result)) = pinned.poll(cx) {
                    self.finished_results
                        .push((call_id, result.map_err(|e| eyre::eyre!(e))));
                    // Check if finished, remove if so
                    if self.multi_step_futures[i].is_finished() {
                        self.multi_step_futures.swap_remove(i);
                    }
                    return Poll::Ready(Some(()));
                }
            }

            // Nothing ready
            if self.pending_results.is_empty() && self.multi_step_futures.is_empty() {
                Poll::Ready(None)
            } else {
                Poll::Pending
            }
        }).await
    }

    /// Get and clear all finished results
    pub fn take_finished_results(&mut self) -> Vec<(String, Result<Value>)> {
        std::mem::take(&mut self.finished_results)
    }

    /// Check if there are any pending results
    pub fn has_pending_results(&self) -> bool {
        !self.pending_results.is_empty() || !self.multi_step_futures.is_empty()
    }

    /// Add an external future to the pending results (for agent tools not in scheduler)
    pub fn add_pending_result(&mut self, future: ToolResultFuture) {
        self.pending_results.push(future);
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

    /// Check if a specific tool call is finished (for multi-step tools)
    pub fn is_tool_finished(&self, tool_call_id: &str) -> bool {
        self.pending_results
            .iter()
            .find(|f| f.tool_call_id() == tool_call_id)
            .map(|f| f.is_finished())
            .unwrap_or(true) // If not found, consider it finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::format_tool_name;
    use std::time::Duration;

    // Mock multi-step tool for testing
    struct MockMultiStepTool;

    impl AnyApiTool for MockMultiStepTool {
        fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
            async { Ok(serde_json::json!({"step": 1})) }.boxed()
        }

        fn validate_json(&self, _payload: &Value) -> bool {
            true
        }

        fn tool(&self) -> &'static str {
            "mock_multi_step"
        }

        fn description(&self) -> &'static str {
            "Mock multi-step tool"
        }

        fn static_topic(&self) -> &'static str {
            "mock_multi_step"
        }

        fn multi_steps(&self) -> bool {
            true
        }

        fn call_with_sender(
            &self,
            _payload: Value,
            sender: mpsc::Sender<Result<Value>>,
        ) -> BoxFuture<'static, Result<()>> {
            async move {
                // Send 3 results with small delays
                for i in 1..=3 {
                    let finished = i == 3;
                    let result = serde_json::json!({
                        "step": i,
                        "finished": finished
                    });
                    if sender.send(Ok(result)).await.is_err() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Ok(())
            }
            .boxed()
        }
    }

    // Mock single-result tool for comparison
    struct MockSingleTool;

    impl AnyApiTool for MockSingleTool {
        fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
            async { Ok(serde_json::json!({"result": "single"})) }.boxed()
        }

        fn validate_json(&self, _payload: &Value) -> bool {
            true
        }

        fn tool(&self) -> &'static str {
            "mock_single"
        }

        fn description(&self) -> &'static str {
            "Mock single tool"
        }

        fn static_topic(&self) -> &'static str {
            "mock_single"
        }

        fn multi_steps(&self) -> bool {
            false
        }
    }

    // Mock multi-step tool that returns error
    struct MockMultiStepErrorTool;

    impl AnyApiTool for MockMultiStepErrorTool {
        fn call_with_json(&self, _payload: Value) -> BoxFuture<'static, Result<Value>> {
            async { Err(eyre::eyre!("error")) }.boxed()
        }

        fn validate_json(&self, _payload: &Value) -> bool {
            true
        }

        fn tool(&self) -> &'static str {
            "mock_multi_step_error"
        }

        fn description(&self) -> &'static str {
            "Mock multi-step error tool"
        }

        fn static_topic(&self) -> &'static str {
            "mock_multi_step_error"
        }

        fn multi_steps(&self) -> bool {
            true
        }

        fn call_with_sender(
            &self,
            _payload: Value,
            sender: mpsc::Sender<Result<Value>>,
        ) -> BoxFuture<'static, Result<()>> {
            async move {
                // Send one success, then error
                let _ = sender.send(Ok(serde_json::json!({"step": 1}))).await;
                let _ = sender.send(Err(eyre::eyre!("step 2 failed"))).await;
                Ok(())
            }
            .boxed()
        }
    }

    fn register_mock_tools(scheduler: &ToolScheduler) {
        let mut tools = scheduler.tools.write().unwrap();
        // Only insert if not already present (idempotent for global scheduler)
        tools.entry("mock_multi_step".to_string()).or_insert_with(|| Arc::new(MockMultiStepTool));
        tools.entry("mock_single".to_string()).or_insert_with(|| Arc::new(MockSingleTool));
        tools.entry("mock_multi_step_error".to_string()).or_insert_with(|| Arc::new(MockMultiStepErrorTool));
    }

    // Generate unique call IDs for tests to avoid interference
    fn unique_call_id(prefix: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        format!("{}_{}", prefix, COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn test_format_tool_name_snake_case() {
        assert_eq!(
            format_tool_name("encode_function_call"),
            "Encode function call"
        );
        assert_eq!(format_tool_name("get_current_time"), "Get current time");
        assert_eq!(format_tool_name("send_transaction"), "Send transaction");
    }

    #[test]
    fn test_format_tool_name_non_snake_case() {
        assert_eq!(format_tool_name("MyTool"), "My tool");
        assert_eq!(format_tool_name("GetTime"), "Get time");
        assert_eq!(format_tool_name("encode"), "Encode");
    }

    #[test]
    fn test_format_tool_name_caching() {
        let result1 = format_tool_name("test_tool");
        let result2 = format_tool_name("test_tool");
        // Should return the same reference due to caching
        assert!(std::ptr::eq(result1, result2));
    }

    #[tokio::test]
    async fn test_typed_scheduler_unknown_tool_and_streaming() {
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        let mut handler = scheduler.get_handler();

        // Scheduler is already running via get_or_init

        // Request with streaming for an unknown tool (unified API)
        let json = serde_json::json!({"test": "data"});
        let mut tool_stream = handler
            .request_with_json_stream("unknown_tool".to_string(), json, "stream_1".to_string())
            .await;

        // Should receive a failure message
        let message = tool_stream.next().await;
        assert!(message.is_some(), "Should receive stream message");

        let (call_id, result) = message.unwrap();
        assert_eq!(call_id, "stream_1");
        assert!(result.is_err(), "Result should be an Err for unknown tool");

        let error_msg = result.unwrap_err();
        assert!(
            error_msg.contains("Unknown tool"),
            "Message should mention unknown tool: {}",
            error_msg
        );
    }

    /// Test 1: Multi-step tool sends multiple results
    /// - First result should come via the stream (for UI ACK)
    /// - Subsequent results via poll_next_result
    #[tokio::test]
    async fn test_multi_step_tool_multiple_results() {
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        register_mock_tools(&scheduler);

        let mut handler = scheduler.get_handler();
        // Manually add tool_info since we registered directly
        handler.tool_info.insert("mock_multi_step".to_string(), (true, "Mock multi step".to_string()));

        let call_id = unique_call_id("multi_step");
        let json = serde_json::json!({});
        let mut stream = handler
            .request_with_json_stream("mock_multi_step".to_string(), json, call_id.clone())
            .await;

        // Use select! to poll both stream and handler concurrently (like completion.rs)
        let mut first_chunk = None;
        let mut results = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

        while tokio::time::Instant::now() < deadline {
            tokio::select! {
                chunk = stream.next(), if first_chunk.is_none() => {
                    if let Some(c) = chunk {
                        first_chunk = Some(c);
                    }
                }
                _ = handler.poll_next_result(), if handler.has_pending_results() => {
                    results.extend(handler.take_finished_results());
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    // Check if we're done
                    if first_chunk.is_some() && !handler.has_pending_results() {
                        break;
                    }
                }
            }
        }

        // Verify first chunk
        let (recv_call_id, result) = first_chunk.expect("Should receive first chunk via stream");
        assert_eq!(recv_call_id, call_id);
        let value = result.expect("First result should be Ok");
        assert_eq!(value.get("step").unwrap(), 1);

        // Should have received steps 2 and 3 (step 1 went to stream)
        assert!(results.len() >= 1, "Should have received more results: {:?}", results);
    }

    /// Test 2: Single-result tool uses oneshot path
    #[tokio::test]
    async fn test_single_tool_uses_oneshot() {
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        register_mock_tools(&scheduler);

        let mut handler = scheduler.get_handler();
        handler.tool_info.insert("mock_single".to_string(), (false, "Mock single".to_string()));

        let call_id = unique_call_id("single");
        let json = serde_json::json!({});
        let mut stream = handler
            .request_with_json_stream("mock_single".to_string(), json, call_id.clone())
            .await;

        // Result comes via stream (with timeout for safety)
        let result = tokio::time::timeout(Duration::from_secs(5), stream.next()).await
            .expect("Timeout")
            .expect("Should receive result via stream");
        let (recv_id, value) = result;
        assert_eq!(recv_id, call_id);
        let value = value.expect("Result should be Ok");
        assert_eq!(value.get("result").unwrap(), "single");

        // Stream done
        assert!(stream.next().await.is_none());

        // poll_next_result also gets the same result (shared future)
        if handler.has_pending_results() {
            handler.poll_next_result().await;
            let finished = handler.take_finished_results();
            assert_eq!(finished.len(), 1);
            assert_eq!(finished[0].1.as_ref().unwrap().get("result").unwrap(), "single");
        }
    }

    /// Test 3: Multi-step tool with error propagates correctly
    #[tokio::test]
    async fn test_multi_step_tool_error() {
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        register_mock_tools(&scheduler);

        let mut handler = scheduler.get_handler();
        handler.tool_info.insert("mock_multi_step_error".to_string(), (true, "Mock error".to_string()));

        let call_id = unique_call_id("error");
        let json = serde_json::json!({});
        let mut stream = handler
            .request_with_json_stream("mock_multi_step_error".to_string(), json, call_id)
            .await;

        // Use select! to poll both stream and handler concurrently
        let mut first_chunk = None;
        let mut results = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

        while tokio::time::Instant::now() < deadline {
            tokio::select! {
                chunk = stream.next(), if first_chunk.is_none() => {
                    if let Some(c) = chunk {
                        first_chunk = Some(c);
                    }
                }
                _ = handler.poll_next_result(), if handler.has_pending_results() => {
                    results.extend(handler.take_finished_results());
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    if first_chunk.is_some() && !handler.has_pending_results() {
                        break;
                    }
                }
            }
        }

        // First chunk should be success
        let (_, result) = first_chunk.expect("Should receive first chunk");
        assert!(result.is_ok(), "First result should be Ok");

        // Should have the error in results
        let has_error = results.iter().any(|(_, r)| r.is_err());
        assert!(has_error, "Should have received error result: {:?}", results);
    }

    /// Test 4: Verify multi_steps flag is correctly detected
    #[tokio::test]
    async fn test_multi_step_flag_detection() {
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        register_mock_tools(&scheduler);

        let handler = scheduler.get_handler();

        // These should be detected based on tool_info cache
        // (handler needs to be created after tools are registered for cache to populate)
        let mut handler2 = scheduler.get_handler();
        handler2.tool_info.insert("mock_multi_step".to_string(), (true, "test".to_string()));
        handler2.tool_info.insert("mock_single".to_string(), (false, "test".to_string()));

        assert!(handler2.is_multi_step("mock_multi_step"));
        assert!(!handler2.is_multi_step("mock_single"));
        assert!(!handler2.is_multi_step("nonexistent"));
    }
}

// Example test for future error handling patterns
#[cfg(test)]
mod future_tests {
    use eyre::Result;
    use futures::TryFutureExt;

    async fn might_fail(i: u32) -> Result<u32> {
        if i.is_multiple_of(2) {
            Ok(i * 2)
        } else {
            Err(eyre::eyre!("odd number"))
        }
    }

    #[tokio::test]
    async fn test_future_error_handling() {
        let fut = might_fail(3);

        // Apply a map_err transformation *without awaiting yet*
        let fut2 = fut.map_err(|e| e.wrap_err("error"));

        // Still a Future — it's lazy!
        match fut2.await {
            Ok(v) => println!("ok: {v}"),
            Err(e) => println!("err: {e}"),
        }
    }
}
