use crate::AomiApiTool;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRequestParams {
    pub address: String,
    pub block_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherRequestParams {
    pub city: String,
    pub country: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRequest {
    pub request_id: String,
    pub query: ContractRequestParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractResponse {
    pub contract_id: String,
    pub data: serde_json::Value,
    pub status: String,
}

unsafe impl Send for ContractResponse {}
unsafe impl Sync for ContractResponse {}

pub struct GetContractFuture {
    request: ContractRequest,
    delay: Pin<Box<tokio::time::Sleep>>,
    completed: bool,
}

impl Future for GetContractFuture {
    type Output = Result<ContractResponse, anyhow::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.completed {
            return Poll::Ready(Err(anyhow::anyhow!("Future already completed")));
        }

        // Check if delay is complete
        match self.delay.as_mut().poll(cx) {
            Poll::Ready(()) => {
                self.completed = true;
                Poll::Ready(Ok(ContractResponse {
                    contract_id: self.request.request_id.clone(),
                    data: serde_json::json!({
                        "balance": 1000000,
                        "owner": "0x1234...",
                        "query_result": format!("Fake result for contract {} at block {}", self.request.query.address, self.request.query.block_number)
                    }),
                    status: "success".to_string(),
                }))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Clone)]
pub struct ContractApi {
    client_id: String,
}

impl ContractApi {
    pub fn new() -> Self {
        Self {
            client_id: Uuid::new_v4().to_string(),
        }
    }

    pub fn get_contract_async(&self, request: ContractRequest) -> GetContractFuture {
        // Simulate variable network delay (200-800ms)
        let delay_ms = 200 + (request.request_id.len() % 600);
        
        GetContractFuture {
            request,
            delay: Box::pin(tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms as u64))),
            completed: false,
        }
    }

    // Convenience async function wrapper
    pub async fn get_contract(&self, request: ContractRequest) -> Result<ContractResponse, anyhow::Error> {
        self.get_contract_async(request).await
    }
}

impl Default for ContractApi {
    fn default() -> Self {
        Self::new()
    }
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherRequest {
    pub request_id: String,
    pub query: WeatherRequestParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherResponse {
    pub weather_id: String,
    pub data: serde_json::Value,
    pub status: String,
}

unsafe impl Send for WeatherResponse {}
unsafe impl Sync for WeatherResponse {}

pub struct GetWeatherFuture {
    request: WeatherRequest,
    delay: Pin<Box<tokio::time::Sleep>>,
    completed: bool,
}

impl Future for GetWeatherFuture {
    type Output = Result<WeatherResponse, anyhow::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.completed {
            return Poll::Ready(Err(anyhow::anyhow!("Future already completed")));
        }

        // Check if delay is complete
        match self.delay.as_mut().poll(cx) {
            Poll::Ready(()) => {
                self.completed = true;
                Poll::Ready(Ok(WeatherResponse {
                    weather_id: self.request.request_id.clone(),
                    data: serde_json::json!({
                        "temperature": "22°C",
                        "humidity": "65%",
                        "query_result": format!("Fake weather for {} in {}", self.request.query.city, self.request.query.country)
                    }),
                    status: "success".to_string(),
                }))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Clone)]
pub struct WeatherApi {
    client_id: String,
}

impl WeatherApi {
    pub fn new() -> Self {
        Self {
            client_id: Uuid::new_v4().to_string(),
        }
    }

    pub fn get_weather_async(&self, request: WeatherRequest) -> GetWeatherFuture {
        // Simulate variable network delay (200-800ms)
        let delay_ms = 200 + (request.request_id.len() % 600);
        
        GetWeatherFuture {
            request,
            delay: Box::pin(tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms as u64))),
            completed: false,
        }
    }

    // Convenience async function wrapper
    pub async fn get_weather(&self, request: WeatherRequest) -> Result<WeatherResponse, anyhow::Error> {
        self.get_weather_async(request).await
    }
}

impl Default for WeatherApi {
    fn default() -> Self {
        Self::new()
    }
}

// Implement ExternalApiTool for ContractApiClient
impl AomiApiTool for ContractApi {
    type ApiRequest = ContractRequest;
    type ApiResponse = ContractResponse;
    
    fn call(&self, request: Self::ApiRequest) -> Pin<Box<dyn Future<Output = Result<Self::ApiResponse, String>> + Send>> {
        let client = self.clone();
        Box::pin(async move {
            match client.get_contract(request).await {
                Ok(response) => Ok(response),
                Err(e) => Err(e.to_string()),
            }
        })
    }
    
    fn name(&self) -> &'static str {
        "contract_api"
    }
    
    fn description(&self) -> &'static str {
        "Retrieve contract information and execute contract queries"
    }

    fn check_input(&self, request: Self::ApiRequest) -> bool {
        // Validate contract request
        !request.request_id.is_empty() && !request.query.address.is_empty()
    }
}

// Implement ExternalApiTool for WeatherApiClient  
impl AomiApiTool for WeatherApi {
    type ApiRequest = WeatherRequest;
    type ApiResponse = WeatherResponse;
    
    fn call(&self, request: Self::ApiRequest) -> Pin<Box<dyn Future<Output = Result<Self::ApiResponse, String>> + Send>> {
        let client = self.clone();
        Box::pin(async move {
            match client.get_weather(request).await {
                Ok(response) => Ok(response),
                Err(e) => Err(e.to_string()),
            }
        })
    }
    
    fn name(&self) -> &'static str {
        "weather_api"
    }
    
    fn description(&self) -> &'static str {
        "Retrieve weather information and forecasts"
    }

    fn check_input(&self, request: Self::ApiRequest) -> bool {
        // Validate weather request
        !request.request_id.is_empty() && !request.query.city.is_empty() && !request.query.country.is_empty()
    }
}





// Implement rig::tool::Tool for ContractApi
impl rig::tool::Tool for ContractApi {
    const NAME: &'static str = "contract_api";
    type Args = ContractRequestParams;
    type Output = ContractResponse;
    type Error = rig::tool::ToolError;

    fn name(&self) -> String {
        "contract_api".to_string()
    }

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        let parameters = serde_json::json!({
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "The contract address to query"
                },
                "block_number": {
                    "type": "integer",
                    "description": "The block number to query at"
                }
            },
            "required": ["address", "block_number"]
        });

        rig::completion::ToolDefinition {
            name: "contract_api".to_string(),
            description: "Retrieve contract information and execute contract queries".to_string(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let request = ContractRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            query: args,
        };

        self.get_contract(request)
            .await
            .map_err(|e| rig::tool::ToolError::ToolCallError(e.into()))
    }
}

// Experimental - keeping as reference for future channel-based tool implementation
trait MyRigTool {
    type Args;
    type Output;
    type Error;

    fn call_async(
        &self, 
        args: Self::Args
    ) -> impl Future<Output = tokio::sync::oneshot::Receiver<Result<Self::Output, Self::Error>>> + Send;
}

impl MyRigTool for WeatherApi {
    type Args = WeatherRequestParams;
    type Output = WeatherResponse;
    type Error = String;

    async fn call_async(&self, args: WeatherRequestParams) ->  tokio::sync::oneshot::Receiver<Result<Self::Output, Self::Error>> {
        let request = WeatherRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            query: args,
        };
        let handler = crate::SCHEDULER_SINGLETON.get().unwrap().as_ref().unwrap().get_handler();
        handler.request(self, request).await
    }
}



// Implement rig::tool::Tool for WeatherApi
impl rig::tool::Tool for WeatherApi {
    const NAME: &'static str = "weather_api";
    type Args = WeatherRequestParams;
    type Output = WeatherResponse;
    type Error = rig::tool::ToolError;

    fn name(&self) -> String {
        "weather_api".to_string()
    }

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        let parameters = serde_json::json!({
            "type": "object",
            "properties": {
                "city": {
                    "type": "string",
                    "description": "The city to get weather for"
                },
                "country": {
                    "type": "string",
                    "description": "The country code"
                }
            },
            "required": ["city", "country"]
        });

        rig::completion::ToolDefinition {
            name: "weather_api".to_string(),
            description: "Retrieve weather information and forecasts".to_string(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let request = WeatherRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            query: args,
        };

        self.get_weather(request)
            .await
            .map_err(|e| rig::tool::ToolError::ToolCallError(e.into()))
    }
}

#[cfg(test)]
mod rig_integration_tests {
    use super::*;
    use rig::tool::Tool;

    #[tokio::test]
    async fn test_contract_api_rig_tool() {
        let tool = ContractApi::new();
        
        // Test tool definition
        let definition = tool.definition("test".to_string()).await;
        assert_eq!(definition.name, "contract_api");
        assert_eq!(definition.description, "Retrieve contract information and execute contract queries");
        
        // Test tool call
        let args = ContractRequestParams {
            address: "0x123".to_string(),
            block_number: 12345,
        };
        
        let result = rig::tool::Tool::call(&tool, args).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response.status, "success");
    }

    #[tokio::test]
    async fn test_weather_api_rig_tool() {
        let tool = WeatherApi::new();
        
        // Test tool definition
        let definition = tool.definition("test".to_string()).await;
        assert_eq!(definition.name, "weather_api");
        assert_eq!(definition.description, "Retrieve weather information and forecasts");
        
        // Test tool call
        let args = WeatherRequestParams {
            city: "New York".to_string(),
            country: "USA".to_string(),
        };
        
        let result = rig::tool::Tool::call(&tool, args).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response.status, "success");
    }
}




