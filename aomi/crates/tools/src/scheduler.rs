use crate::clients::{init_external_clients, ExternalClients};
use crate::tool_stream::{SchedulerRequest, ToolResultFuture, ToolResultSender, ToolResultStream};
use crate::types::{AomiApiTool, AnyApiTool};
use eyre::Result;
use futures::future::{BoxFuture, FutureExt};
use futures::stream::{FuturesUnordered, StreamExt};
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
    /// Pending tool futures (single and multi-step share the same queue)
    pending_results: Vec<ToolResultFuture>,
    /// Collected finished results for finalize_tool_results
    finished_results: Vec<(String, Result<Value>)>,
    /// Cache for tool metadata: tool_name -> (multi_steps, static_topic)
    tool_info: HashMap<String, (bool, String)>,
}

impl ToolApiHandler {
    fn new(requests_tx: mpsc::Sender<(SchedulerRequest, ToolResultSender)>) -> Self {
        Self {
            requests_tx,
            pending_results: Vec::new(),
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

            // Add to pending results (receiver drives subsequent chunks)
            self.pending_results.push(ToolResultFuture::new_multi_step(
                tool_call_id.clone(),
                rx,
                first_tx,
            ));

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
            self.pending_results
                .push(ToolResultFuture::new_single(tool_call_id.clone(), shared_future.clone()));

            // Return stream from shared future
            ToolResultStream::from_shared(shared_future)
        }
    }

    /// Poll for the next completed tool result and add it to finished_results
    /// Returns None if no results ready
    pub async fn poll_next_result(&mut self) -> Option<()> {
        use std::future::poll_fn;
        use std::task::Poll;

        // Poll all pending tool futures (single and multi-step)
        poll_fn(|cx| {
            let mut i = 0;
            while i < self.pending_results.len() {
                let fut = &mut self.pending_results[i];
                // SAFETY: We're not moving the future, just polling it in place
                let pinned = unsafe { Pin::new_unchecked(fut) };
                if let Poll::Ready((call_id, result)) = pinned.poll(cx) {
                    self.finished_results
                        .push((call_id, result.map_err(|e| eyre::eyre!(e))));
                    // Check if finished, remove if so
                    if self.pending_results[i].is_finished() {
                        self.pending_results.swap_remove(i);
                    }
                    return Poll::Ready(Some(()));
                }
                i += 1;
            }

            // Nothing ready
            if self.pending_results.is_empty() {
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
#[path = "test.rs"]
mod tests;
