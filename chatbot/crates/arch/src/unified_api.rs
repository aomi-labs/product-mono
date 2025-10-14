use crate::{
    ContractApiClient, ContractRequest, ContractResponse,
    WeatherApiClient, WeatherRequest, WeatherResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Unified request enum that can hold different API request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiRequest {
    Contract(ContractRequest),
    Weather(WeatherRequest),
}

/// Unified response enum that can hold different API response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiResponse {
    Contract(ContractResponse),
    Weather(WeatherResponse),
}

/// Unified API client that dispatches to specific clients based on request type
#[derive(Clone)]
pub struct UnifiedApiClient {
    contract_client: Arc<ContractApiClient>,
    weather_client: Arc<WeatherApiClient>,
}

impl UnifiedApiClient {
    pub fn new() -> Self {
        Self {
            contract_client: Arc::new(ContractApiClient::new()),
            weather_client: Arc::new(WeatherApiClient::new()),
        }
    }

    /// Dispatch API call to the appropriate client based on request type
    pub async fn call_api(&self, request: ApiRequest) -> Result<ApiResponse, String> {
        match request {
            ApiRequest::Contract(contract_req) => {
                match self.contract_client.get_contract(contract_req).await {
                    Ok(response) => Ok(ApiResponse::Contract(response)),
                    Err(e) => Err(e.to_string()),
                }
            }
            ApiRequest::Weather(weather_req) => {
                match self.weather_client.get_weather(weather_req).await {
                    Ok(response) => Ok(ApiResponse::Weather(response)),
                    Err(e) => Err(e.to_string()),
                }
            }
        }
    }

    /// Helper method to create a contract request
    pub fn contract_request(contract_id: String, query: String) -> ApiRequest {
        ApiRequest::Contract(ContractRequest { contract_id, query })
    }

    /// Helper method to create a weather request
    pub fn weather_request(weather_id: String, query: String) -> ApiRequest {
        ApiRequest::Weather(WeatherRequest { 
            Weather_id: weather_id, 
            query 
        })
    }
}

impl Default for UnifiedApiClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for working with unified responses
impl ApiResponse {
    /// Check if this is a contract response
    pub fn is_contract(&self) -> bool {
        matches!(self, ApiResponse::Contract(_))
    }

    /// Check if this is a weather response
    pub fn is_weather(&self) -> bool {
        matches!(self, ApiResponse::Weather(_))
    }

    /// Extract contract response if available
    pub fn as_contract(&self) -> Option<&ContractResponse> {
        match self {
            ApiResponse::Contract(response) => Some(response),
            _ => None,
        }
    }

    /// Extract weather response if available
    pub fn as_weather(&self) -> Option<&WeatherResponse> {
        match self {
            ApiResponse::Weather(response) => Some(response),
            _ => None,
        }
    }

    /// Get a generic status string
    pub fn status(&self) -> &str {
        match self {
            ApiResponse::Contract(response) => &response.status,
            ApiResponse::Weather(response) => &response.status,
        }
    }
}

#[tokio::test]
async fn test_unified_api_contract() {
    let client = UnifiedApiClient::new();
    let request = UnifiedApiClient::contract_request("0x123".to_string(), "balance".to_string());
    
    let response = client.call_api(request).await.unwrap();
    assert!(response.is_contract());
    assert_eq!(response.status(), "success");
}

#[tokio::test]
async fn test_unified_api_weather() {
    let client = UnifiedApiClient::new();
    let request = UnifiedApiClient::weather_request("weather123".to_string(), "temperature".to_string());
    
    let response = client.call_api(request).await.unwrap();
    assert!(response.is_weather());
    assert_eq!(response.status(), "success");
}