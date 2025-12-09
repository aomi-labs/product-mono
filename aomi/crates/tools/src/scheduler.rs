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
    /// Multi-step - receiver stays here, first chunk taken via borrow
    MultiStep {
        tool_call_id: String,
        finished: bool,
        receiver: mpsc::Receiver<Result<Value>>,
        first_chunk_taken: bool,
    },
}

impl Debug for ToolResultFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Single { tool_call_id, .. } => {
                write!(f, "ToolResultFuture::Single({})", tool_call_id)
            }
            Self::MultiStep { tool_call_id, finished, first_chunk_taken, .. } => {
                write!(f, "ToolResultFuture::MultiStep({}, finished={}, first_taken={})",
                    tool_call_id, finished, first_chunk_taken)
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

    /// Get mutable access to the receiver for MultiStep variant
    pub fn get_receiver_mut(&mut self) -> Option<&mut mpsc::Receiver<Result<Value>>> {
        match self {
            Self::Single { .. } => None,
            Self::MultiStep { receiver, .. } => Some(receiver),
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
            Self::MultiStep { tool_call_id, finished, receiver, first_chunk_taken } => {
                match receiver.poll_recv(cx) {
                    Poll::Ready(Some(result)) => {
                        *first_chunk_taken = true;
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
pub struct ToolResultStream(pub BoxFuture<'static, (String, Result<Value, String>)>);

impl Debug for ToolResultStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ToolResultStream")
    }
}

impl Stream for ToolResultStream {
    type Item = (String, Result<Value, String>);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        match inner.poll(cx) {
            Poll::Ready(result) => Poll::Ready(Some(result)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl ToolResultStream {
    /// Create an empty stream for testing
    pub fn empty() -> Self {
        Self(async { ("".to_string(), Ok(Value::Null)) }.boxed())
    }

    /// Create a test stream with custom data
    pub fn from_result(call_id: String, result: Result<Value, String>) -> Self {
        Self(async move { (call_id, result) }.boxed())
    }

    /// Create from a shared future (for single-result tools)
    pub fn from_shared(future: Shared<BoxFuture<'static, (String, Result<Value, String>)>>) -> Self {
        Self(async move { future.await }.boxed())
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
    /// Pending tool results - unified for both single and multi-step
    pending_results: FuturesUnordered<ToolResultFuture>,
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

            // Add to pending_results - receiver stays here for subsequent polling
            self.pending_results.push(ToolResultFuture::MultiStep {
                tool_call_id: tool_call_id.clone(),
                finished: false,
                receiver: rx,
                first_chunk_taken: false,
            });

            // For multi-step, we need to get the first chunk from pending_results
            // Find the future we just added and take first chunk
            self.get_first_chunk_stream(tool_call_id)
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

    /// Get stream for first chunk of a multi-step tool result
    ///
    /// For multi-step tools, we need the first chunk for UI ACK but also need
    /// to keep the receiver in pending_results for subsequent polling.
    ///
    /// Solution: Use a oneshot channel to bridge. We poll the receiver once
    /// synchronously (try_recv) or spawn a task to get the first chunk.
    fn get_first_chunk_stream(&mut self, tool_call_id: String) -> ToolResultStream {
        // Find the multi-step future
        let tool_future = self.pending_results
            .iter_mut()
            .find(|f| f.tool_call_id() == &tool_call_id);

        if let Some(tool_future) = tool_future {
            if let Some(receiver) = tool_future.get_receiver_mut() {
                // Create a oneshot to send the first chunk
                let (tx, rx) = oneshot::channel();
                let call_id = tool_call_id.clone();

                // Try to get first chunk - if not ready yet, we need to poll
                // Use try_recv for non-blocking check, or spawn task for async
                match receiver.try_recv() {
                    Ok(result) => {
                        // Got first chunk synchronously
                        let _ = tx.send(result);
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {
                        // Not ready yet - the stream will wait
                        // We can't easily spawn here without taking ownership
                        // So return a future that polls pending_results
                        let call_id = tool_call_id.clone();
                        return ToolResultStream(async move {
                            // This will be resolved when poll_next_result is called
                            // For now, return "pending" status
                            (call_id, Ok(serde_json::json!({
                                "status": "pending",
                                "message": "Waiting for first result..."
                            })))
                        }.boxed());
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        // Channel closed
                        let _ = tx.send(Err(eyre::eyre!("Channel closed")));
                    }
                }

                // Return stream that waits on the oneshot
                return ToolResultStream(async move {
                    match rx.await {
                        Ok(result) => (call_id, result.map_err(|e| e.to_string())),
                        Err(_) => (call_id, Err("Channel closed".to_string())),
                    }
                }.boxed());
            }
        }

        // Fallback: tool not found or not multi-step
        ToolResultStream(async move {
            (tool_call_id, Err("Tool not found in pending results".to_string()))
        }.boxed())
    }

    /// Poll for the next completed tool result and add it to finished_results
    /// Returns None if no results ready
    pub async fn poll_next_result(&mut self) -> Option<()> {
        match self.pending_results.next().await {
            Some((call_id, result)) => {
                self.finished_results
                    .push((call_id, result.map_err(|e| eyre::eyre!(e))));
                Some(())
            }
            None => None,
        }
    }

    /// Get and clear all finished results
    pub fn take_finished_results(&mut self) -> Vec<(String, Result<Value>)> {
        std::mem::take(&mut self.finished_results)
    }

    /// Check if there are any pending results
    pub fn has_pending_results(&self) -> bool {
        !self.pending_results.is_empty()
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

    // Note: Tests have been temporarily commented out as they depend on the removed
    // tool-specific types (AbiEncoderTool, WalletTransactionTool, TimeTool).
    // These tests would need to be rewritten to use the Rig tools directly
    // if they were made public.

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
