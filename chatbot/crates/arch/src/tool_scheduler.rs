use crate::AomiApiTool;
use futures::future::{BoxFuture, FutureExt};
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{mpsc, oneshot};

static TOKIO_SINGLETON: OnceLock<std::io::Result<Arc<tokio::runtime::Runtime>>> = OnceLock::new();

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
    T: AomiApiTool + 'static,
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
                Err(e) => return Err(e),
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
    tools: Arc<HashMap<String, Arc<dyn AnyApiTool>>>,
    requests_rx: mpsc::Receiver<(
        SchedulerRequest,
        oneshot::Sender<Result<serde_json::Value, String>>,
    )>,
    requests_tx: mpsc::Sender<(
        SchedulerRequest,
        oneshot::Sender<Result<serde_json::Value, String>>,
    )>,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl ToolScheduler {
    /// Create a new typed scheduler with tool registry
    pub fn new() -> (ToolApiHandler, Self) {
        let (requests_tx, requests_rx) = mpsc::channel(100);

        let handler = ToolApiHandler::new(requests_tx.clone());

        let scheduler = ToolScheduler {
            tools: Arc::new(HashMap::new()),
            requests_rx,
            requests_tx,
            runtime: Self::get_tokio_singleton().unwrap(),
        };

        (handler, scheduler)
    }

    pub fn get_handler(&self) -> ToolApiHandler {
        ToolApiHandler {
            requests_tx: self.requests_tx.clone(),
        }
    }

    /// Register a tool in the scheduler
    pub fn register_tool<T>(&mut self, tool: T)
    where
        T: AomiApiTool + 'static,
        T::ApiRequest: for<'de> Deserialize<'de> + Send + 'static,
        T::ApiResponse: Serialize + Send + 'static,
    {
        let tool_name = tool.name().to_string();
        let tools = Arc::get_mut(&mut self.tools).unwrap();
        tools.insert(tool_name, Arc::new(tool));
    }

    fn get_tokio_singleton() -> Result<Arc<tokio::runtime::Runtime>, String> {
        match TOKIO_SINGLETON.get_or_init(|| tokio::runtime::Runtime::new().map(Arc::new)) {
            Ok(t) => Ok(t.clone()),
            Err(_e) => Err("Failed to get singleton runtime".to_string()),
        }
    }

    /// Start the scheduler loop
    pub fn run(self) {
        let ToolScheduler {
            tools,
            mut requests_rx,
            runtime,
            ..
        } = self;

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
                                    let result = if let Some(tool) = tools.get(&request.tool_name) {
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
            .iter()
            .map(|(name, tool)| (name.clone(), tool.description().to_string()))
            .collect()
    }
}

/// Handler for sending requests to the scheduler
pub struct ToolApiHandler {
    requests_tx: mpsc::Sender<(
        SchedulerRequest,
        oneshot::Sender<Result<serde_json::Value, String>>,
    )>,
}

impl ToolApiHandler {
    fn new(
        requests_tx: mpsc::Sender<(
            SchedulerRequest,
            oneshot::Sender<Result<serde_json::Value, String>>,
        )>,
    ) -> Self {
        Self { requests_tx }
    }

    /// Schedule a typed request that preserves type safety
    pub async fn request<T>(
        &self,
        tool: &T,
        request: T::ApiRequest,
    ) -> oneshot::Receiver<Result<T::ApiResponse, String>>
    where
        T: AomiApiTool,
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
                    let _ = tx.send(Err(error));
                }
                Err(_) => {
                    let _ = tx.send(Err("Channel closed".to_string()));
                }
            }
        });

        rx
    }

    /// Schedule raw JSON request
    pub async fn request_with_json(
        &self,
        tool_name: String,
        payload: serde_json::Value,
    ) -> oneshot::Receiver<Result<serde_json::Value, String>> {
        let (tx, rx) = oneshot::channel();
        let request = SchedulerRequest { tool_name, payload };
        let _ = self.requests_tx.send((request, tx)).await;
        rx
    }

    /// Convenience method for contract requests
    pub async fn request_contract_api(
        &self,
        contract_id: String,
        address: String,
        block_number: u64,
    ) -> oneshot::Receiver<Result<crate::ContractResponse, String>> {
        use crate::{ContractApi, ContractRequest, ContractRequestParams};
        let tool = ContractApi::new();
        let request = ContractRequest {
            request_id: contract_id,
            query: ContractRequestParams {
                address,
                block_number,
            },
        };
        self.request(&tool, request).await
    }

    /// Convenience method for weather requests  
    pub async fn request_weather_api(
        &self,
        weather_id: String,
        city: String,
        country: String,
    ) -> oneshot::Receiver<Result<crate::WeatherResponse, String>> {
        use crate::{WeatherApi, WeatherRequest, WeatherRequestParams};
        let tool = WeatherApi::new();
        let request = WeatherRequest {
            request_id: weather_id,
            query: WeatherRequestParams { city, country },
        };
        self.request(&tool, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContractApi, ContractRequest, ContractRequestParams, WeatherApi};

    #[tokio::test]
    async fn test_typed_scheduler_with_registration() {
        let (handler, mut scheduler) = ToolScheduler::new();

        // Register tools
        scheduler.register_tool(ContractApi::new());
        scheduler.register_tool(WeatherApi::new());

        // Verify tools are registered
        let tools = scheduler.list_tools();
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().any(|(name, _)| name == "contract_api"));
        assert!(tools.iter().any(|(name, _)| name == "weather_api"));

        // Start scheduler
        scheduler.run();

        // Test contract request
        let contract_receiver = handler
            .request_contract_api("0x123".to_string(), "0xcontract123".to_string(), 12345)
            .await;
        let contract_result = contract_receiver.await.unwrap();
        assert!(contract_result.is_ok());
        assert_eq!(contract_result.unwrap().contract_id, "0x123");

        // Test weather request
        let weather_receiver = handler
            .request_weather_api(
                "weather123".to_string(),
                "New York".to_string(),
                "USA".to_string(),
            )
            .await;
        let weather_result = weather_receiver.await.unwrap();
        assert!(weather_result.is_ok());
        assert_eq!(weather_result.unwrap().weather_id, "weather123");
    }

    #[tokio::test]
    async fn test_typed_scheduler_validation() {
        let (handler, mut scheduler) = ToolScheduler::new();
        scheduler.register_tool(ContractApi::new());
        scheduler.run();

        // Test with invalid JSON
        let invalid_json = serde_json::json!({"invalid": "data"});
        let result = handler
            .request_with_json("contract_api".to_string(), invalid_json)
            .await;
        let response = result.await.unwrap();
        assert!(response.is_err());
        let error = response.unwrap_err();
        println!("Actual error: {}", error);
        assert!(error.contains("Failed to deserialize") || error.contains("validation failed"));
    }

    #[tokio::test]
    async fn test_typed_scheduler_unknown_tool() {
        let (handler, scheduler) = ToolScheduler::new();
        scheduler.run();

        let json = serde_json::json!({"request_id": "0x123", "query": {"address": "0xabc", "block_number": 123}});
        let result = handler
            .request_with_json("unknown_tool".to_string(), json)
            .await;
        let response = result.await.unwrap();
        assert!(response.is_err());
        assert!(response.unwrap_err().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_schedule_typed_preserves_types() {
        let (handler, mut scheduler) = ToolScheduler::new();
        let contract_tool = ContractApi::new();
        scheduler.register_tool(contract_tool.clone());
        scheduler.run();

        // Use the typed interface
        let request = ContractRequest {
            request_id: "0x456".to_string(),
            query: ContractRequestParams {
                address: "0xowner456".to_string(),
                block_number: 54321,
            },
        };

        let receiver = handler.request(&contract_tool, request).await;
        let result = receiver.await.unwrap();

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.contract_id, "0x456");
        assert_eq!(response.status, "success");
    }

    #[tokio::test]
    async fn test_input_validation() {
        let (handler, mut scheduler) = ToolScheduler::new();
        scheduler.register_tool(ContractApi::new());
        scheduler.run();

        // Test with empty contract_id (should fail validation)
        let invalid_request = ContractRequest {
            request_id: "".to_string(), // Empty contract_id
            query: ContractRequestParams {
                address: "".to_string(), // Empty address should fail validation
                block_number: 123,
            },
        };

        let tool = ContractApi::new();
        let receiver = handler.request(&tool, invalid_request).await;
        let result = receiver.await.unwrap();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("validation failed"));
    }
}
