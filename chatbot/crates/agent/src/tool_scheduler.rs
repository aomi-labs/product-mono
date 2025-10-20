use crate::AomiApiTool;
use futures::future::{BoxFuture, FutureExt};
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::{OnceCell, mpsc, oneshot};

pub type ToolResultFuture = BoxFuture<'static, Result<(String, String), String>>;

static SCHEDULER: OnceCell<Arc<ToolScheduler>> = OnceCell::const_new();
/// Type-erased request that can hold any tool request as JSON
#[derive(Debug, Clone)]
pub struct SchedulerRequest {
    pub tool_name: String,
    pub payload: serde_json::Value,
}

/// Trait object for type-erased API tools
pub trait AnyApiTool: Send + Sync {
    fn call_with_json(
        &self,
        payload: serde_json::Value,
    ) -> BoxFuture<'static, Result<serde_json::Value, String>>;
    fn validate_json(&self, payload: &serde_json::Value) -> bool;
    fn tool(&self) -> &'static str;
    fn description(&self) -> &'static str;
}

/// Implement AnyApiTool for any ExternalApiTool
impl<T> AnyApiTool for T
where
    T: AomiApiTool + Clone + 'static,
    T::ApiRequest: for<'de> Deserialize<'de> + Send + 'static,
    T::ApiResponse: Serialize + Send + 'static,
{
    fn call_with_json(
        &self,
        payload: serde_json::Value,
    ) -> BoxFuture<'static, Result<serde_json::Value, String>> {
        let tool = self.clone();
        async move {
            // 1. Deserialize JSON to T::ApiRequest
            let request: T::ApiRequest = match serde_json::from_value(payload) {
                Ok(req) => req,
                Err(e) => return Err(format!("Failed to deserialize request: {}", e)),
            };

            // 2. Validate input using the tool's validation
            if !tool.check_input(request.clone()) {
                return Err("Request validation failed".to_string());
            }

            // 3. Call the actual API
            let response = match tool.call(request).await {
                Ok(resp) => resp,
                Err(e) => return Err(e.to_string()),
            };

            // 4. Serialize response back to JSON
            match serde_json::to_value(response) {
                Ok(json) => Ok(json),
                Err(e) => Err(format!("Failed to serialize response: {}", e)),
            }
        }
        .boxed()
    }

    fn validate_json(&self, payload: &serde_json::Value) -> bool {
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
}

/// Unified scheduler that can handle any registered API tool
pub struct ToolScheduler {
    tools: Arc<RwLock<HashMap<String, Arc<dyn AnyApiTool>>>>,
    requests_tx: mpsc::Sender<(
        SchedulerRequest,
        oneshot::Sender<Result<serde_json::Value, String>>,
    )>,
    runtime: Arc<tokio::runtime::Handle>,
}

impl ToolScheduler {
    /// Create a new typed scheduler with tool registry
    fn new() -> (
        Self,
        mpsc::Receiver<(
            SchedulerRequest,
            oneshot::Sender<Result<serde_json::Value, String>>,
        )>,
    ) {
        let (requests_tx, requests_rx) = mpsc::channel(100);
        let runtime = tokio::runtime::Handle::current();

        let scheduler = ToolScheduler {
            tools: Arc::new(RwLock::new(HashMap::new())),
            requests_tx,
            runtime: Arc::new(runtime),
        };

        (scheduler, requests_rx)
    }

    pub async fn get_or_init() -> Result<Arc<ToolScheduler>, String> {
        Ok(SCHEDULER
            .get_or_init(|| async {
                let (scheduler, requests_rx) = Self::new();
                let scheduler = Arc::new(scheduler);
                // Start the scheduler's event loop in the background
                Self::run(scheduler.clone(), requests_rx);
                scheduler
            })
            .await
            .clone())
    }

    pub fn get_handler(&self) -> ToolApiHandler {
        ToolApiHandler::new(self.requests_tx.clone())
    }

    /// Register a tool in the scheduler
    pub fn register_tool<T>(&self, tool: T) -> Result<(), String>
    where
        T: AomiApiTool + Clone + 'static,
        T::ApiRequest: for<'de> Deserialize<'de> + Send + 'static,
        T::ApiResponse: Serialize + Send + 'static,
    {
        let tool_name = tool.name().to_string();
        self.tools
            .write()
            .map_err(|_| "Failed to acquire write lock".to_string())?
            .insert(tool_name, Arc::new(tool));
        Ok(())
    }

    /// Spawn the scheduler loop in the background
    fn run(
        scheduler: Arc<Self>,
        mut requests_rx: mpsc::Receiver<(
            SchedulerRequest,
            oneshot::Sender<Result<serde_json::Value, String>>,
        )>,
    ) {
        let tools = scheduler.tools.clone();
        let runtime = scheduler.runtime.clone();

        runtime.spawn(async move {
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
                                            Err("Request validation failed".to_string())
                                        }
                                    } else {
                                        Err(format!("Unknown tool: {}", request.tool_name))
                                    };

                                    // Respond to the awaiting oneshot listener
                                    let _ = reply_tx.send(result);
                                });
                            }
                            None => {
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
                            break;
                        }
                    },
                }
            }
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
        self.tools
            .read()
            .unwrap().keys().map(|name| name.clone())
            .collect()
    }
}

/// Handler for sending requests to the scheduler
pub struct ToolApiHandler {
    requests_tx: mpsc::Sender<(
        SchedulerRequest,
        oneshot::Sender<Result<serde_json::Value, String>>,
    )>,
    pending_results: FuturesUnordered<ToolResultFuture>,
}

impl ToolApiHandler {
    fn new(
        requests_tx: mpsc::Sender<(
            SchedulerRequest,
            oneshot::Sender<Result<serde_json::Value, String>>,
        )>,
    ) -> Self {
        Self {
            requests_tx,
            pending_results: FuturesUnordered::new(),
        }
    }

    /// Schedule a typed request that preserves type safety
    pub async fn request<T>(
        &mut self,
        tool: &T,
        request: T::ApiRequest,
    ) -> oneshot::Receiver<Result<T::ApiResponse, String>>
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
                    match serde_json::from_value::<T::ApiResponse>(json_response) {
                        Ok(typed_response) => {
                            let _ = tx.send(Ok(typed_response));
                        }
                        Err(e) => {
                            let _ = tx.send(Err(format!("Failed to deserialize response: {}", e)));
                        }
                    }
                }
                Ok(Err(error)) => {
                    let _ = tx.send(Err(error.clone()));
                }
                Err(_) => {
                    let error_msg = "Channel closed".to_string();
                    let _ = tx.send(Err(error_msg.clone()));
                }
            }
        });
        rx
    }

    /// Schedule raw JSON request and return a tool result ID for tracking
    pub async fn request_with_json(
        &mut self,
        tool_name: String,
        payload: serde_json::Value,
        tool_call_id: String,
    ) {
        let (tx, rx) = oneshot::channel();
        let request = SchedulerRequest {
            tool_name: tool_name.clone(),
            payload,
        };

        // Send the request to the scheduler
        let _ = self.requests_tx.send((request, tx)).await;

        // Create a future that converts the oneshot response to our format
        let future = async move {
            match rx.await {
                Ok(Ok(json_response)) => {
                    let output = serde_json::to_string_pretty(&json_response)
                        .unwrap_or_else(|_| "Tool execution successful".to_string());
                    Ok((tool_call_id, output))
                }
                Ok(Err(err)) => Err(format!("Tool execution failed: {}", err)),
                Err(_) => Err("Tool scheduler channel closed unexpectedly".to_string()),
            }
        }
        .boxed();

        // Add to our pending results
        self.pending_results.push(future);
    }

    /// Poll for the next completed tool result
    pub async fn poll_next_result(&mut self) -> Option<Result<(String, String), String>> {
        self.pending_results.next().await
    }

    /// Check if there are any pending results
    pub fn has_pending_results(&self) -> bool {
        !self.pending_results.is_empty()
    }

    /// Add an external future to the pending results (for agent tools not in scheduler)
    pub fn add_external_future(&mut self, future: ToolResultFuture) {
        self.pending_results.push(future);
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
        let response = handler.poll_next_result().await.unwrap();
        assert!(response.is_err());
        assert!(response.unwrap_err().contains("Unknown tool"));
    }
}

// Example test for future error handling patterns
#[cfg(test)]
mod future_tests {
    use futures::TryFutureExt;

    async fn might_fail(i: u32) -> Result<u32, String> {
        if i % 2 == 0 {
            Ok(i * 2)
        } else {
            Err("odd number".to_string())
        }
    }

    #[tokio::test]
    async fn test_future_error_handling() {
        let fut = might_fail(3);

        // Apply a map_err transformation *without awaiting yet*
        let fut2 = fut.map_err(|e| format!("error: {}", e));

        // Still a Future — it's lazy!
        match fut2.await {
            Ok(v) => println!("ok: {v}"),
            Err(e) => println!("err: {e}"),
        }
    }
}
