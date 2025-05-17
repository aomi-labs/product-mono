use alloy_primitives::hex;
use alloy_provider::RootProvider;
use alloy_rpc_types::TransactionInput;
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use cast::*;
use rmcp::{
    Error as McpError, RoleServer, ServerHandler, model::*, schemars, service::RequestContext, tool,
};
use alloy_primitives::Address;
use alloy_primitives::U256;
use alloy_provider::{Provider, ProviderBuilder, network::AnyNetwork};
use alloy_transport_http::Http;

#[derive(Debug, Clone)]
pub struct CastService {
    provider: RootProvider<AnyNetwork>,
}

#[tool(tool_box)]
impl CastService {
    pub async fn new() -> Result<Self, McpError> {
        let provider = ProviderBuilder::<_, _, AnyNetwork>::default()
            .on_builtin("http://localhost:8545")
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to initialize provider: {}", e), None))?;
        
        Ok(Self { provider })
    }

    // Blockchain Interaction
    #[tool(description = "Get the balance of an account in wei")]
    async fn balance(
        &self,
        #[tool(param)]
        #[schemars(description = "The address to check balance for")]
        address: String,
    ) -> Result<CallToolResult, McpError> {
        let address = address.parse::<Address>()
            .map_err(|e| McpError::invalid_params(format!("Invalid address: {}", e), None))?;
        
        let balance = self.provider.get_balance(address)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get balance: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(balance.to_string())]))
    }

    #[tool(description = "Get the nonce for an account")]
    async fn nonce(
        &self,
        #[tool(param)]
        #[schemars(description = "The address to check nonce for")]
        address: String,
    ) -> Result<CallToolResult, McpError> {
        let address = address.parse::<Address>()
            .map_err(|e| McpError::invalid_params(format!("Invalid address: {}", e), None))?;
        
        let nonce = self.provider.get_transaction_count(address)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get nonce: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(nonce.to_string())]))
    }

    // Contract Interaction
    #[tool(description = "Perform a call on an account without publishing a transaction")]
    async fn call(
        &self,
        #[tool(param)]
        #[schemars(description = "The address to call")]
        address: String,
        #[tool(param)]
        #[schemars(description = "The calldata to send")]
        calldata: String,
    ) -> Result<CallToolResult, McpError> {
        let address = address.parse::<Address>()
            .map_err(|e| McpError::invalid_params(format!("Invalid address: {}", e), None))?;
        
        let calldata = hex::decode(calldata.trim_start_matches("0x"))
            .map_err(|e| McpError::invalid_params(format!("Invalid calldata: {}", e), None))?;

        let transaction_request = TransactionRequest::default()
            .to(address)
            .input(TransactionInput::new(calldata.into()));

        let result = self.provider.call(WithOtherFields::<TransactionRequest>::new(transaction_request))
        .await
        .map_err(|e| McpError::internal_error(format!("Failed to call contract: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(hex::encode(result))]))
    }

    // Data Conversion
    #[tool(description = "Convert wei into an ETH amount")]
    async fn from_wei(
        &self,
        #[tool(param)]
        #[schemars(description = "The amount in wei to convert")]
        wei: String,
    ) -> Result<CallToolResult, McpError> {
        let wei = wei.parse::<U256>()
            .map_err(|e| McpError::invalid_params(format!("Invalid wei amount: {}", e), None))?;
        
        let eth = wei.to_string();
        Ok(CallToolResult::success(vec![Content::text(eth)]))
    }

    #[tool(description = "Convert an ETH amount to wei")]
    async fn to_wei(
        &self,
        #[tool(param)]
        #[schemars(description = "The amount in ETH to convert")]
        eth: String,
    ) -> Result<CallToolResult, McpError> {
        let eth = eth.parse::<f64>()
            .map_err(|e| McpError::invalid_params(format!("Invalid ETH amount: {}", e), None))?;
        
        let wei = (eth * 1e18) as u128;
        Ok(CallToolResult::success(vec![Content::text(wei.to_string())]))
    }
}

#[tool(tool_box)]
impl ServerHandler for CastService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Cast service provides Ethereum blockchain interaction tools including balance checks, nonce queries, contract calls, and unit conversions.".to_string()),
        }
    }
}