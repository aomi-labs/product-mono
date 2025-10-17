use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};


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

// ABI Encoder Tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiEncoderRequest {
    pub function_signature: String,
    pub arguments: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiEncoderResponse {
    pub encoded_data: String,
}

#[derive(Clone)]
pub struct AbiEncoderTool;

impl AbiEncoderTool {
    pub fn new() -> Self {
        Self
    }
}

impl AomiApiTool for AbiEncoderTool {
    type ApiRequest = AbiEncoderRequest;
    type ApiResponse = AbiEncoderResponse;
    
    fn call(&self, request: Self::ApiRequest) -> Pin<Box<dyn Future<Output = Result<Self::ApiResponse, String>> + Send>> {
        Box::pin(async move {
            match crate::abi_encoder::encode_function_call(request.function_signature, request.arguments) {
                Ok(encoded) => Ok(AbiEncoderResponse { encoded_data: encoded }),
                Err(e) => Err(e.to_string()),
            }
        })
    }
    
    fn name(&self) -> &'static str {
        "encode_function_call"
    }
    
    fn description(&self) -> &'static str {
        "Encodes a function call into hex calldata for any contract function"
    }

    fn check_input(&self, request: Self::ApiRequest) -> bool {
        !request.function_signature.is_empty() && request.function_signature.contains('(') && request.function_signature.contains(')')
    }
}

// Wallet Transaction Tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTransactionRequest {
    pub to: String,
    pub value: String,
    pub data: String,
    pub gas_limit: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTransactionResponse {
    pub transaction: serde_json::Value,
}

#[derive(Clone)]
pub struct WalletTransactionTool;

impl WalletTransactionTool {
    pub fn new() -> Self {
        Self
    }
}

impl AomiApiTool for WalletTransactionTool {
    type ApiRequest = WalletTransactionRequest;
    type ApiResponse = WalletTransactionResponse;
    
    fn call(&self, request: Self::ApiRequest) -> Pin<Box<dyn Future<Output = Result<Self::ApiResponse, String>> + Send>> {
        Box::pin(async move {
            match crate::wallet::send_transaction_to_wallet(
                request.to,
                request.value,
                request.data,
                request.gas_limit,
                request.description,
            ) {
                Ok(tx) => Ok(WalletTransactionResponse { transaction: tx }),
                Err(e) => Err(e.to_string()),
            }
        })
    }
    
    fn name(&self) -> &'static str {
        "send_transaction_to_wallet"
    }
    
    fn description(&self) -> &'static str {
        "Send a crafted transaction to the user's wallet for approval and signing"
    }

    fn check_input(&self, request: Self::ApiRequest) -> bool {
        !request.to.is_empty() 
            && request.to.starts_with("0x") 
            && request.to.len() == 42
            && !request.value.is_empty()
            && request.value.parse::<u128>().is_ok()
            && request.data.starts_with("0x")
            && !request.description.is_empty()
            && request.gas_limit.as_ref().map_or(true, |g| g.parse::<u64>().is_ok())
    }
}

// Time Tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRequest {
    // Empty request - no parameters needed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeResponse {
    pub timestamp: String,
}

#[derive(Clone)]
pub struct TimeTool;

impl TimeTool {
    pub fn new() -> Self {
        Self
    }
}

impl AomiApiTool for TimeTool {
    type ApiRequest = TimeRequest;
    type ApiResponse = TimeResponse;
    
    fn call(&self, _request: Self::ApiRequest) -> Pin<Box<dyn Future<Output = Result<Self::ApiResponse, String>> + Send>> {
        Box::pin(async move {
            match crate::time::get_current_time() {
                Ok(timestamp) => Ok(TimeResponse { timestamp }),
                Err(e) => Err(e.to_string()),
            }
        })
    }
    
    fn name(&self) -> &'static str {
        "get_current_time"
    }
    
    fn description(&self) -> &'static str {
        "Get the current Unix timestamp"
    }

    fn check_input(&self, _request: Self::ApiRequest) -> bool {
        true // No validation needed for time requests
    }
}





#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_abi_encoder_tool() {
        let tool = AbiEncoderTool::new();
        
        assert_eq!(tool.name(), "encode_function_call");
        assert_eq!(tool.description(), "Encodes a function call into hex calldata for any contract function");
        
        let request = AbiEncoderRequest {
            function_signature: "transfer(address,uint256)".to_string(),
            arguments: vec![
                serde_json::Value::String("0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string()),
                serde_json::Value::String("1000000000000000000".to_string()),
            ],
        };
        
        assert!(tool.check_input(request.clone()));
        
        let result = tool.call(request).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert!(response.encoded_data.starts_with("0x"));
        assert!(response.encoded_data.len() > 10);
    }

    #[tokio::test]
    async fn test_wallet_transaction_tool() {
        let tool = WalletTransactionTool::new();
        
        assert_eq!(tool.name(), "send_transaction_to_wallet");
        assert_eq!(tool.description(), "Send a crafted transaction to the user's wallet for approval and signing");
        
        let request = WalletTransactionRequest {
            to: "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            value: "1000000000000000000".to_string(),
            data: "0x".to_string(),
            gas_limit: None,
            description: "Send 1 ETH to recipient".to_string(),
        };
        
        assert!(tool.check_input(request.clone()));
        
        let result = tool.call(request).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert!(response.transaction.get("to").and_then(|v| v.as_str()).is_some());
        assert!(response.transaction.get("value").and_then(|v| v.as_str()).is_some());
        assert!(response.transaction.get("timestamp").and_then(|v| v.as_str()).is_some());
    }

    #[tokio::test]
    async fn test_time_tool() {
        let tool = TimeTool::new();
        
        assert_eq!(tool.name(), "get_current_time");
        assert_eq!(tool.description(), "Get the current Unix timestamp");
        
        let request = TimeRequest {};
        
        assert!(tool.check_input(request.clone()));
        
        let result = tool.call(request).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert!(response.timestamp.parse::<u64>().is_ok());
    }

    #[tokio::test]
    async fn test_trait_polymorphism() {
        // Test that we can use the trait polymorphically
        async fn call_any_api<T: AomiApiTool>(tool: &T, request: T::ApiRequest) -> Result<T::ApiResponse, String> {
            tool.call(request).await
        }
        
        let abi_tool = AbiEncoderTool::new();
        let abi_request = AbiEncoderRequest {
            function_signature: "balanceOf(address)".to_string(),
            arguments: vec![
                serde_json::Value::String("0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string()),
            ],
        };
        
        let result = call_any_api(&abi_tool, abi_request).await;
        assert!(result.is_ok());
        
        let wallet_tool = WalletTransactionTool::new();
        let wallet_request = WalletTransactionRequest {
            to: "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            value: "0".to_string(),
            data: "0x70a08231000000000000000000000000742d35cc6634c0532925a3b844bc9e7595f33749".to_string(),
            gas_limit: Some("50000".to_string()),
            description: "Check balance".to_string(),
        };
        
        let result = call_any_api(&wallet_tool, wallet_request).await;
        assert!(result.is_ok());
        
        let time_tool = TimeTool::new();
        let time_request = TimeRequest {};
        
        let result = call_any_api(&time_tool, time_request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_input_validation() {
        // Test ABI encoder validation
        let abi_tool = AbiEncoderTool::new();
        let invalid_abi_request = AbiEncoderRequest {
            function_signature: "invalid_function".to_string(), // Missing parentheses
            arguments: vec![],
        };
        assert!(!abi_tool.check_input(invalid_abi_request));
        
        // Test wallet validation
        let wallet_tool = WalletTransactionTool::new();
        let invalid_wallet_request = WalletTransactionRequest {
            to: "invalid_address".to_string(), // Invalid address format
            value: "1000000000000000000".to_string(),
            data: "0x".to_string(),
            gas_limit: None,
            description: "Test".to_string(),
        };
        assert!(!wallet_tool.check_input(invalid_wallet_request));
        
        // Test valid wallet request
        let valid_wallet_request = WalletTransactionRequest {
            to: "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            value: "1000000000000000000".to_string(),
            data: "0x".to_string(),
            gas_limit: Some("21000".to_string()),
            description: "Valid transaction".to_string(),
        };
        assert!(wallet_tool.check_input(valid_wallet_request));
    }
}
