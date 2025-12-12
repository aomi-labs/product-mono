use crate::clients::{init_external_clients, ExternalClients};
use crate::tool_stream::{SchedulerRequest, ToolResultFuture, ToolResultSender, ToolResultStream};
use crate::types::{AomiApiTool, AnyApiTool};
use eyre::Result;
use futures::stream::{FuturesUnordered, StreamExt};
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use tokio::sync::{mpsc, oneshot, OnceCell};
use tracing::{debug, error, warn};

static SCHEDULER: OnceCell<Arc<ToolScheduler>> = OnceCell::const_new();

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
        let (runtime, runtime_guard) = if cfg!(test) {
            // In tests, own the runtime so the global scheduler outlives individual test runtimes
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("aomi-tool-scheduler")
                .build()
                .map_err(|err| eyre::eyre!("Failed to build tool scheduler runtime: {err}"))?;
            let handle = rt.handle().clone();
            (Arc::new(handle), Some(Arc::new(rt)))
        } else {
            match tokio::runtime::Handle::try_current() {
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
    /// Pending futures before conversion to streams (internal channel handles)
    pub pending_futures: Vec<ToolResultFuture>,
    /// Pending streams to poll for results (converted from futures, ready for UI presentation)
    pending_streams: Vec<ToolResultStream>,
    /// Cache for tool metadata: tool_name -> (multi_steps, static_topic)
    tool_info: HashMap<String, (bool, String)>,
}

impl ToolApiHandler {
    fn new(requests_tx: mpsc::Sender<(SchedulerRequest, ToolResultSender)>) -> Self {
        Self {
            requests_tx,
            pending_futures: Vec::new(),
            pending_streams: Vec::new(),
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
    /// Enqueues a ToolResultFuture to pending_futures for later conversion
    pub async fn request(
        &mut self,
        tool_name: String,
        payload: Value,
        call_id: String,
    ) {
        let is_multi_step = self.tool_info
            .get(&tool_name)
            .map(|(multi, _)| *multi)
            .unwrap_or(false);

        let request = SchedulerRequest {
            tool_name: tool_name.clone(),
            payload,
        };

        let tool_future = if is_multi_step {
            let rx = self.send_multi_step_request(request).await;
            ToolResultFuture::new_multi_step(call_id.clone(), rx)
        } else {
            let rx = self.send_oneshot_request(request).await;
            ToolResultFuture::new_single(call_id.clone(), rx)
        };

        // Enqueue future - caller will retrieve and convert to streams
        self.pending_futures.push(tool_future);
    }


    async fn send_oneshot_request(&self, request: SchedulerRequest) -> oneshot::Receiver<Result<Value>> {
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.requests_tx.send((request, ToolResultSender::Oneshot(tx))).await {
            error!("Failed to send request to scheduler: {}", e);
        }
        rx
    }

    async fn send_multi_step_request(&self, request: SchedulerRequest) -> mpsc::Receiver<Result<Value>> {
        let (tx, rx) = mpsc::channel::<Result<Value>>(100);

        if let Err(e) = self.requests_tx.send((request, ToolResultSender::MultiStep(tx))).await {
            error!("Failed to send request to scheduler: {}", e);
        }
        rx
    }

    /// Convert any queued pending results into pollable streams.
    /// Returns Some if work was done, None otherwise.
    pub async fn poll_futures_to_streams(&mut self) -> Option<()> {
        if self.pending_futures.is_empty() {
            return None;
        }

        let streams = self.take_futures();
        if !streams.is_empty() { Some(()) } else { None }
    }

    /// Pop the most recent pending future, convert to streams, and return both
    /// (pending/internal, ui/ack). Caller is responsible for registering the
    /// pending stream via add_pending_stream().
    pub fn take_last_future_as_streams(&mut self) -> Option<(ToolResultStream, ToolResultStream)> {
        let mut future = self.pending_futures.pop()?;
        Some(future.into_shared_streams())
    }

    /// Move all pending futures into streams and return a mutable reference to the pending streams.
    pub fn take_futures(&mut self) -> &mut Vec<ToolResultStream> {
        while let Some(mut fut) = self.pending_futures.pop() {
            let (pending_stream, _) = fut.into_shared_streams();
            self.pending_streams.push(pending_stream);
        }
        &mut self.pending_streams
    }

    /// Get mutable reference to pending_streams for finalization
    pub fn pending_streams_mut(&mut self) -> &mut Vec<ToolResultStream> {
        &mut self.pending_streams
    }

    /// Await the next item from any pending stream. Removes exhausted streams.
    pub async fn poll_streams_to_next_result(
        &mut self,
    ) -> Option<(String, Result<Value, String>)> {
        use std::future::poll_fn;
        use std::task::Poll;

        poll_fn(|cx| {
            let mut i = 0;
            while i < self.pending_streams.len() {
                let stream = &mut self.pending_streams[i];
                match Pin::new(stream).poll_next(cx) {
                    Poll::Ready(Some(item)) => return Poll::Ready(Some(item)),
                    Poll::Ready(None) => {
                        self.pending_streams.swap_remove(i);
                        continue;
                    }
                    Poll::Pending => {
                        i += 1;
                    }
                }
            }

            if self.pending_streams.is_empty() {
                Poll::Ready(None)
            } else {
                Poll::Pending
            }
        })
        .await
    }

    /// Check if there are any pending streams
    pub fn has_pending_streams(&self) -> bool {
        !self.pending_streams.is_empty() || !self.pending_futures.is_empty()
    }

    /// Check if there are pending results awaiting conversion to streams
    pub fn has_pending_futures(&self) -> bool {
        !self.pending_futures.is_empty()
    }

    /// Add an external stream to pending_streams (for agent tools not in scheduler)
    pub fn add_pending_stream(&mut self, stream: ToolResultStream) {
        self.pending_streams.push(stream);
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

}

#[cfg(test)]
#[path = "test.rs"]
mod tests;
