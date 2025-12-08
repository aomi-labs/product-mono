use crate::clients::{ExternalClients, init_external_clients};
use crate::types::{AnyApiTool, AomiApiTool};
use eyre::{Report as ErrReport, Result};
use futures::Stream;
use futures::future::{BoxFuture, FutureExt, IntoStream};
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll}; // <-- this one matters!
use tokio::sync::{OnceCell, mpsc, oneshot};
use tracing::{debug, error, warn};

pub type ToolResultFutureInner = BoxFuture<'static, (String, Result<Value, String>)>;

// Wrapper to impl Debug
pub struct ToolResultFuture(pub ToolResultFutureInner);

impl Debug for ToolResultFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ToolResultFuture")
    }
}

impl Future for ToolResultFuture {
    type Output = (String, Result<Value, String>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: We are only projecting the pin to the inner future,
        // and we never move the inner value after it’s been pinned.
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        inner.poll(cx)
    }
}


pub struct ToolResultFuture2 {
    pub tool_call_id: String,
    pub finished: bool,
    pub result_rx: mpsc::Receiver<Result<Value>>,
}


impl Future for ToolResultFuture2 {
    type Output = (String, Result<Value, String>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let ToolResultFuture2 { tool_call_id, finished, result_rx } = self.get_mut();
        match result_rx.poll_recv(cx) {
            Poll::Ready(Some(result)) => {
                match result {
                    Ok(result) => {
                        // Tool layer is responsible for setting the finished flag
                        *finished = result["finished"].as_bool().unwrap_or(false);
                        Poll::Ready((tool_call_id.clone(), Ok(result)))
                    },
                    Err(e) => Poll::Ready((tool_call_id.clone(), Err(e.to_string()))),
                }
            },
            Poll::Ready(None) => Poll::Ready((tool_call_id.clone(), Ok(Value::Null))),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct ToolResultStream2<'a>(BoxFuture<'a, (String, Result<Value, ErrReport>)>);


pub struct ToolResultStream(pub IntoStream<ToolResultFuture>);

impl Debug for ToolResultStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ToolResultStream")
    }
}

impl Stream for ToolResultStream {
    type Item = (String, Result<Value, String>);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY: projecting the pin safely into inner stream
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        inner.poll_next(cx)
    }
}

impl ToolResultStream {
    /// Create an empty stream for testing
    pub fn empty() -> Self {
        let empty_future = async { ("".to_string(), Ok(serde_json::Value::Null)) }.boxed();
        let wrapped_future = ToolResultFuture(empty_future);
        let stream = wrapped_future.into_stream();
        Self(stream)
    }

    /// Create a test stream with custom data
    pub fn from_result(call_id: String, result: Result<Value, String>) -> Self {
        let future = async move { (call_id, result) }.boxed();
        let wrapped_future = ToolResultFuture(future);
        let stream = wrapped_future.into_stream();
        Self(stream)
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
    requests_tx: mpsc::Sender<(SchedulerRequest, oneshot::Sender<Result<Value>>)>,
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
        mpsc::Receiver<(SchedulerRequest, oneshot::Sender<Result<Value>>)>,
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
            let supports_streaming = tool.supports_streaming();
            let static_topic = tool.static_topic().to_string();
            handler
                .too_info
                .insert(name.clone(), (supports_streaming, static_topic));
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
        mut requests_rx: mpsc::Receiver<(SchedulerRequest, oneshot::Sender<Result<Value>>)>,
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

                                    let result = if let Some(tool) = tool_option {
                                        if tool.validate_json(&request.payload) {
                                            tool.call_with_json(request.payload).await
                                        } else {
                                            Err(eyre::eyre!("Request validation failed"))
                                        }
                                    } else {
                                        warn!("Unknown tool requested: {}", request.tool_name);
                                        Err(eyre::eyre!(
                                            "Unknown tool: {}",
                                            request.tool_name
                                        ))
                                    };

                                    // Respond to the awaiting oneshot listener
                                    if reply_tx.send(result).is_err() {
                                        warn!("Failed to send tool result - receiver dropped");
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

/// Handler for sending requests to the scheduler
pub struct ToolApiHandler {
    requests_tx: mpsc::Sender<(SchedulerRequest, oneshot::Sender<Result<Value>>)>,

    requests_tx2: mpsc::Sender<(SchedulerRequest, mpsc::Sender<Result<Value>>)>,
    pending_results2: FuturesUnordered<ToolResultFuture2>,


    pending_results: FuturesUnordered<ToolResultFuture>,
    finished_results: Vec<(String, Result<Value>)>,
    /// Cache for tool metadata: tool_name -> (supports_streaming, static_topic)
    too_info: HashMap<String, (bool, String)>,
}

impl ToolApiHandler {
    fn new(requests_tx: mpsc::Sender<(SchedulerRequest, oneshot::Sender<Result<Value>>)>) -> Self {

        let (requests_tx2, requests_rx2) = mpsc::channel(100);

        Self {
            requests_tx,

            // TODO
            requests_tx2,
            pending_results2: FuturesUnordered::new(),
            
            pending_results: FuturesUnordered::new(),
            finished_results: Vec::new(),
            too_info: HashMap::new(),
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

        // Send through the channel
        let (internal_tx, internal_rx) = oneshot::channel();
        let _ = self
            .requests_tx
            .send((scheduler_request, internal_tx))
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

    pub async fn request_with_json_stream(
        &mut self,
        tool_name: String,
        payload: Value,
        tool_call_id: String,
    ) -> ToolResultStream {
        let stream = self
            .request_with_json(tool_name, payload, tool_call_id)
            .await
            .into_stream();
        ToolResultStream(stream)
    }

    /// Schedule raw JSON request and return a tool result ID for tracking
    pub async fn request_with_json(
        &mut self,
        tool_name: String,
        payload: Value,
        tool_call_id: String,
    ) -> ToolResultFuture {
        let (tx, rx) = oneshot::channel();
        let request = SchedulerRequest {
            tool_name: tool_name.clone(),
            payload,
        };

        // Send the request to the scheduler
        if let Err(e) = self.requests_tx.send((request, tx)).await {
            error!("Failed to send request to scheduler: {}", e);
        }

        // Create a future that converts the oneshot response to our format
        let future = async move {
            match rx.await {
                Ok(result) => {
                    // Push the Result regardless of success or failure
                    let result_str = result.map_err(|e| e.to_string());
                    (tool_call_id, result_str)
                }
                Err(e) => {
                    // Channel error - return as Err
                    (tool_call_id, Result::<Value, String>::Err(e.to_string()))
                }
            }
        }
        .shared();

        let pending = ToolResultFuture(future.clone().boxed());
        let ret = ToolResultFuture(future.clone().boxed());

        // Add to our pending results
        self.add_pending_result(pending);
        ret
    }


    pub fn get_tool_stream(&mut self, tool_call_id: String) -> ToolResultStream2<'_> {
        let first_chunk_fut = self.pending_results2
            .iter_mut()
            .find(|tool_future| tool_future.tool_call_id == tool_call_id)
            .map(|tool_future| async move {
                    (
                        tool_call_id, 
                        tool_future.result_rx.recv().await.unwrap()
                    )
                }.boxed()
            )
            .unwrap();
        ToolResultStream2(first_chunk_fut)
    }

    pub fn get_tool_status(&mut self, tool_call_id: String) -> bool {
        self.pending_results2
            .iter_mut()
            .find(|tool_future| tool_future.tool_call_id == tool_call_id)
            .map(|tool_future| tool_future.finished)
            .unwrap_or(false)
    }


    pub async fn request2(
        &mut self,
        tool_name: String,
        payload: Value,
        tool_call_id: String,
    ) {
        let result_rx = self.request_inner(tool_name, payload).await;
        self.add_pending_result2(tool_call_id, result_rx);
    }

    pub async fn request_inner(
        &mut self,
        tool_name: String,
        payload: Value,
    ) -> mpsc::Receiver<Result<Value>> {
        let (tx, rx) = mpsc::channel(100);
        let request = SchedulerRequest {
            tool_name: tool_name.clone(),
            payload,
        };

        // Send the request to the scheduler
        if let Err(e) = self.requests_tx2.send((request, tx)).await {
            error!("Failed to send request to scheduler: {}", e);
        }
        return rx;
    }

    pub async fn poll_next_result2(&mut self) -> Option<()> { 
        match self.pending_results2.next().await {
            Some((call_id, result)) => {
                self.finished_results
                    .push((call_id, result.map_err(|e| eyre::eyre!(e))));
                Some(())
            }
            None => None,
        }
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

    pub fn add_pending_result2(&mut self, tool_call_id: String, result_rx: mpsc::Receiver<Result<Value>>) {
        self.pending_results2.push(ToolResultFuture2 { tool_call_id, finished: false, result_rx });
    }

    /// Check if a tool supports streaming (uses cached metadata)
    pub async fn supports_streaming(&mut self, tool_name: &str) -> bool {
        self.too_info
            .get(tool_name)
            .map(|(supports, _)| *supports)
            .unwrap_or(false)
    }

    /// Get topic for a tool (uses cached metadata)
    pub async fn get_topic(&mut self, tool_name: &str) -> String {
        self.too_info
            .get(tool_name)
            .map(|(_, topic)| topic.clone())
            .unwrap_or_else(|| tool_name.to_string())
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

        let json = serde_json::json!({"function_signature": "test()", "arguments": []});
        handler
            .request_with_json("unknown_tool".to_string(), json, "1".to_string())
            .await;
        let response = handler.poll_next_result().await;
        assert!(response.is_some(), "Expected tool result");

        // Tool should return error as Result::Err
        let results = handler.take_finished_results();
        assert_eq!(results.len(), 1);
        let (_call_id, result) = &results[0];

        // Check that the result is an Err variant
        assert!(result.is_err(), "Result should be an Err for unknown tool");
        let error = result.as_ref().unwrap_err();
        let error_msg = format!("{}", error);
        assert!(
            error_msg.contains("Unknown tool"),
            "Error message should mention 'Unknown tool'"
        );

        // Request with streaming for an unknown tool
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
