use crate::AomiApiTool;
use eyre::{Context, Result};
use futures::future::{BoxFuture, FutureExt};
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::{OnceCell, mpsc, oneshot};
use tracing::{error, warn, debug};

pub type ToolResultFuture = BoxFuture<'static, (String, Result<Value>)>;
pub type ToolStreamFuture = BoxFuture<'static, (String, mpsc::Receiver<String>)>;

static SCHEDULER: OnceCell<Arc<ToolScheduler>> = OnceCell::const_new();

/// Type-erased request that can hold any tool request as JSON
#[derive(Debug, Clone)]
pub struct SchedulerRequest {
    pub tool_name: String,
    pub payload: Value,
}

/// Trait object for type-erased API tools
pub trait AnyApiTool: Send + Sync {
    fn call_with_json(&self, payload: Value) -> BoxFuture<'static, Result<Value>>;
    fn validate_json(&self, payload: &Value) -> bool;
    fn tool(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn static_topic(&self) -> &'static str;

    /// Check if this tool supports streaming
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Call with streaming support - default implementation wraps non-streaming call
    fn call_with_stream(
        &self,
        payload: Value,
        tool_id: String,
        stream_tx: mpsc::Sender<String>,
    ) -> BoxFuture<'static, Result<Value>> {
        let fut = self.call_with_json(payload);
        let tool_name = self.tool().to_string();

        async move {
            let start_time = Instant::now();

            // Send started message
            let _ = stream_tx
                .send(format!("[{}] Starting {}", tool_id, tool_name))
                .await;

            // Execute the tool
            match fut.await {
                Ok(result) => {
                    // Send completed message
                    let duration_ms = start_time.elapsed().as_millis();
                    let _ = stream_tx
                        .send(format!("[{}] Completed in {}ms", tool_id, duration_ms))
                        .await;
                    Ok(result)
                }
                Err(e) => {
                    // Send error message
                    let _ = stream_tx.send(format!("[{}] Failed: {}", tool_id, e)).await;
                    Err(e)
                }
            }
        }
        .boxed()
    }
}

/// Implement AnyApiTool for any ExternalApiTool
impl<T> AnyApiTool for T
where
    T: AomiApiTool + Clone + 'static,
    T::ApiRequest: for<'de> Deserialize<'de> + Send + 'static,
    T::ApiResponse: Serialize + Send + 'static,
{
    fn call_with_json(&self, payload: Value) -> BoxFuture<'static, Result<Value>> {
        let tool = self.clone();
        async move {
            // 1. Deserialize JSON to T::ApiRequest
            let request: T::ApiRequest =
                serde_json::from_value(payload).wrap_err("Failed to deserialize request")?;

            // 2. Validate input using the tool's validation
            if !tool.check_input(request.clone()) {
                return Err(eyre::eyre!("Request validation failed"));
            }

            // 3. Call the actual API
            let response = tool.call(request).await.wrap_err("Tool call failed")?;

            // 4. Serialize response back to JSON
            serde_json::to_value(response).wrap_err("Failed to serialize response")
        }
        .boxed()
    }

    fn validate_json(&self, payload: &Value) -> bool {
        // Try to deserialize to check if JSON structure is valid
        match serde_json::from_value::<T::ApiRequest>(payload.clone()) {
            Ok(request) => self.check_input(request),
            Err(_) => false,
        }
    }

    fn tool(&self) -> &'static str {
        <T as AomiApiTool>::name(self)
    }

    fn description(&self) -> &'static str {
        <T as AomiApiTool>::description(self)
    }

    fn static_topic(&self) -> &'static str {
        <T as AomiApiTool>::static_topic(self)
    }
}

/// Unified scheduler that can handle any registered API tool
pub struct ToolScheduler {
    tools: Arc<RwLock<HashMap<String, Arc<dyn AnyApiTool>>>>,
    requests_tx: mpsc::Sender<(SchedulerRequest, oneshot::Sender<Result<Value>>)>,
    runtime: Arc<tokio::runtime::Handle>,
}

impl ToolScheduler {
    /// Create a new typed scheduler with tool registry
    #[allow(clippy::type_complexity)]
    fn new() -> (
        Self,
        mpsc::Receiver<(SchedulerRequest, oneshot::Sender<Result<Value>>)>,
    ) {
        let (requests_tx, requests_rx) = mpsc::channel(1000);
        let runtime = tokio::runtime::Handle::current();

        let scheduler = ToolScheduler {
            tools: Arc::new(RwLock::new(HashMap::new())),
            requests_tx,
            runtime: Arc::new(runtime),
        };

        (scheduler, requests_rx)
    }

    pub async fn get_or_init() -> Result<Arc<ToolScheduler>> {
        let scheduler = SCHEDULER
            .get_or_init(|| async {
                let (scheduler, requests_rx) = Self::new();
                let scheduler = Arc::new(scheduler);
                // Start the scheduler's event loop in the background
                Self::run(scheduler.clone(), requests_rx);
                scheduler
            })
            .await;

        Ok(scheduler.clone())
    }

    pub fn get_handler(&self) -> ToolApiHandler {
        let mut handler = ToolApiHandler::new(self.requests_tx.clone());
        // Pre-populate the cache with current tools
        let tools_guard = self.tools.read().unwrap();
        for (name, tool) in tools_guard.iter() {
            let supports_streaming = tool.supports_streaming();
            let static_topic = tool.static_topic().to_string();
            handler.too_info.insert(
                name.clone(),
                (supports_streaming, static_topic)
            );
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
                                    if let Err(_) = reply_tx.send(result) {
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
    pending_results: FuturesUnordered<ToolResultFuture>,
    finished_results: Vec<(String, Result<Value>)>,
    /// Cache for tool metadata: tool_name -> (supports_streaming, static_topic)
    too_info: HashMap<String, (bool, String)>,
}

impl ToolApiHandler {
    fn new(requests_tx: mpsc::Sender<(SchedulerRequest, oneshot::Sender<Result<Value>>)>) -> Self {
        Self {
            requests_tx,
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

    /// Schedule raw JSON request and return a tool result ID for tracking
    pub async fn request_with_json(
        &mut self,
        tool_name: String,
        payload: Value,
        tool_call_id: String,
    ) {
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
                    (tool_call_id, result)
                }
                Err(_) => {
                    // Channel error - return as Err
                    (
                        tool_call_id,
                        Err(eyre::eyre!("Tool scheduler channel closed unexpectedly")),
                    )
                }
            }
        }
        .boxed();

        // Add to our pending results
        self.pending_results.push(future);
    }

    /// Poll for the next completed tool result and add it to finished_results
    /// Returns None if no results ready
    pub async fn poll_next_result(&mut self) -> Option<()> {
        match self.pending_results.next().await {
            Some((call_id, result)) => {
                self.finished_results.push((call_id, result));
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

    /// Request with streaming support - returns immediately with a stream receiver
    pub async fn request_with_stream(
        &mut self,
        tool_name: String,
        payload: Value,
        tool_call_id: String,
    ) -> mpsc::Receiver<String> {
        let (event_tx, event_rx) = mpsc::channel(100);

        // Create a future that executes the tool with streaming and records its result
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        let tool_option = {
            let tools_guard = scheduler.tools.read().unwrap();
            tools_guard.get(&tool_name).cloned()
        };
        let tool_name_for_logs = tool_name.clone();
        let tool_id = tool_call_id.clone();

        let future = async move {
            match tool_option {
                Some(tool) => {
                    if tool.validate_json(&payload) {
                        match tool
                            .call_with_stream(payload, tool_id.clone(), event_tx.clone())
                            .await
                        {
                            Ok(result) => {
                                debug!("Tool {} completed successfully", tool_name_for_logs);
                                (tool_id, Ok(result))
                            }
                            Err(e) => {
                                error!("Tool {} failed: {}", tool_name_for_logs, e);
                                let _ = event_tx
                                    .send(format!("[{}] Failed: {}", tool_id, e))
                                    .await;
                                (tool_id, Err(e))
                            }
                        }
                    } else {
                        warn!(
                            "Tool {} validation failed for payload",
                            tool_name_for_logs
                        );
                        let err = eyre::eyre!("Request validation failed");
                        let _ = event_tx
                            .send(format!("[{}] Failed: {}", tool_id, err))
                            .await;
                        (tool_id, Err(err))
                    }
                }
                None => {
                    warn!(
                        "Unknown tool requested for streaming: {}",
                        tool_name_for_logs
                    );
                    let err = eyre::eyre!("Unknown tool: {}", tool_name_for_logs);
                    let _ = event_tx
                        .send(format!(
                            "[{}] Failed: Unknown tool: {}",
                            tool_id, tool_name_for_logs
                        ))
                        .await;
                    (tool_id, Err(err))
                }
            }
        }
        .boxed();

        // Record the future so the completion pipeline can consume the result
        self.pending_results.push(future);

        // Return the receiver directly
        event_rx
    }

    // Note: Convenience methods for specific tools have been removed.
    // Use the generic request() method with the Rig tool instances directly.
    // Example:
    // let mut handler = scheduler.get_handler();
    // let tool = crate::abi_encoder::ENCODE_FUNCTION_CALL;
    // let request = crate::abi_encoder::EncodeFunctionCallParameters { ... };
    // handler.request(&tool, request).await?;
    // let json_receiver = handler.take_json_receiver().unwrap();
    // let json_response = json_receiver.await?;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Tests have been temporarily commented out as they depend on the removed
    // tool-specific types (AbiEncoderTool, WalletTransactionTool, TimeTool).
    // These tests would need to be rewritten to use the Rig tools directly
    // if they were made public.

    #[tokio::test]
    async fn test_typed_scheduler_unknown_tool() {
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
    }

    #[tokio::test]
    async fn test_streaming_tool_execution() {
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        let mut handler = scheduler.get_handler();

        // Request with streaming for an unknown tool
        let json = serde_json::json!({"test": "data"});
        let mut message_rx = handler
            .request_with_stream("unknown_tool".to_string(), json, "stream_1".to_string())
            .await;

        // Should receive a failure message
        let message = message_rx.recv().await;
        assert!(message.is_some(), "Should receive stream message");

        let msg = message.unwrap();
        assert!(msg.contains("[stream_1]"), "Message should contain tool ID");
        assert!(msg.contains("Failed"), "Message should indicate failure");
        assert!(
            msg.contains("Unknown tool"),
            "Message should mention unknown tool"
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
