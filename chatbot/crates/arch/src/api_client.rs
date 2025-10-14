use crate::AomiApiTool;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRequest {
    pub request_id: String,
    pub query: String,
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
                        "query_result": format!("Fake result for: {}", self.request.query)
                    }),
                    status: "success".to_string(),
                }))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Clone)]
pub struct ContractApiClient {
    client_id: String,
}

impl ContractApiClient {
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

impl Default for ContractApiClient {
    fn default() -> Self {
        Self::new()
    }
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherRequest {
    pub request_id: String,
    pub query: String,
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
                        "balance": 1000000,
                        "owner": "0x1234...",
                        "query_result": format!("Fake result for: {}", self.request.query)
                    }),
                    status: "success".to_string(),
                }))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Clone)]
pub struct WeatherApiClient {
    client_id: String,
}

impl WeatherApiClient {
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

impl Default for WeatherApiClient {
    fn default() -> Self {
        Self::new()
    }
}

// Implement ExternalApiTool for ContractApiClient
impl AomiApiTool for ContractApiClient {
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
        !request.request_id.is_empty() && !request.query.is_empty()
    }
}

// Implement ExternalApiTool for WeatherApiClient  
impl AomiApiTool for WeatherApiClient {
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
        !request.request_id.is_empty() && !request.query.is_empty()
    }
}




