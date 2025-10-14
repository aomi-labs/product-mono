use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRequest {
    pub contract_id: String,
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
                    contract_id: self.request.contract_id.clone(),
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
        let delay_ms = 200 + (request.contract_id.len() % 600);
        
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
    pub Weather_id: String,
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
                    weather_id: self.request.Weather_id.clone(),
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
        let delay_ms = 200 + (request.Weather_id.len() % 600);
        
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




