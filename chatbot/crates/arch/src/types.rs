use std::future::Future;
use std::pin::Pin;


/// Trait for external API tools with associated request and response types
pub trait AomiApiTool: Send + Sync + Clone {
    type ApiRequest: Send + Sync + Clone;
    type ApiResponse: Send + Sync + Clone;
    
    /// Execute an API call returning a future
    fn call(&self, request: Self::ApiRequest) -> Pin<Box<dyn Future<Output = Result<Self::ApiResponse, String>> + Send>>;
    
    /// Get the name of this API tool
    fn name(&self) -> &'static str;
    
    /// Get a description of what this API tool does
    fn description(&self) -> &'static str;

    fn check_input(&self, request: Self::ApiRequest) -> bool;
}





#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContractApi, ContractRequest, ContractRequestParams, WeatherApi, WeatherRequest, WeatherRequestParams};

    #[tokio::test]
    async fn test_contract_api_tool_trait() {
        let client = ContractApi::new();
        
        assert_eq!(client.name(), "contract_api");
        assert_eq!(client.description(), "Retrieve contract information and execute contract queries");
        
        let request = ContractRequest {
            request_id: "0x123".to_string(),
            query: ContractRequestParams {
                address: "0xcontract123".to_string(),
                block_number: 12345,
            },
        };
        
        let result = client.call(request).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response.contract_id, "0x123");
        assert_eq!(response.status, "success");
    }

    #[tokio::test]
    async fn test_weather_api_tool_trait() {
        let client = WeatherApi::new();
        
        assert_eq!(client.name(), "weather_api");
        assert_eq!(client.description(), "Retrieve weather information and forecasts");
        
        let request = WeatherRequest {
            request_id: "weather123".to_string(),
            query: WeatherRequestParams {
                city: "New York".to_string(),
                country: "USA".to_string(),
            },
        };
        
        let result = client.call(request).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response.weather_id, "weather123");
        assert_eq!(response.status, "success");
    }

    #[tokio::test]
    async fn test_trait_polymorphism() {
        // Test that we can use the trait polymorphically
        async fn call_any_api<T: AomiApiTool>(tool: &T, request: T::ApiRequest) -> Result<T::ApiResponse, String> {
            tool.call(request).await
        }
        
        let contract_client = ContractApi::new();
        let contract_request = ContractRequest {
            request_id: "0x456".to_string(),
            query: ContractRequestParams {
                address: "0xowner456".to_string(),
                block_number: 54321,
            },
        };
        
        let result = call_any_api(&contract_client, contract_request).await;
        assert!(result.is_ok());
        
        let weather_client = WeatherApi::new();
        let weather_request = WeatherRequest {
            request_id: "weather456".to_string(),
            query: WeatherRequestParams {
                city: "London".to_string(),
                country: "UK".to_string(),
            },
        };
        
        let result = call_any_api(&weather_client, weather_request).await;
        assert!(result.is_ok());
    }
}