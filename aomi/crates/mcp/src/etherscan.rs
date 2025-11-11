//! MCP tool for Etherscan API integration
use rmcp::{
    ErrorData,
    handler::server::tool::Parameters,
    model::{CallToolResult, Content},
    tool,
};
use serde::Deserialize;

/// Parameters for the Etherscan getabi tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAbiParams {
    #[schemars(
        description = "The contract address to get the ABI for (must be verified on Etherscan)"
    )]
    pub address: String,

    #[schemars(
        description = "The network to query: 'ethereum', 'goerli', 'sepolia', 'polygon', 'arbitrum', 'optimism', 'base' (default: 'ethereum')"
    )]
    pub network: Option<String>,
}

/// Parameters for the Etherscan transaction history tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTransactionHistoryParams {
    #[schemars(description = "The address to get transaction history for")]
    pub address: String,

    #[schemars(
        description = "Chain ID (1 for ethereum, 5 for goerli, 11155111 for sepolia, 137 for polygon, etc.)"
    )]
    pub chainid: u32,
}

#[derive(Clone)]
pub struct EtherscanTool {
    api_key: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct EtherscanResponse {
    status: String,
    message: String,
    result: serde_json::Value,
}

impl EtherscanTool {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    fn get_api_url(network: &str) -> Result<&'static str, String> {
        match network {
            "ethereum" => Ok("https://api.etherscan.io/api"),
            "goerli" => Ok("https://api-goerli.etherscan.io/api"),
            "sepolia" => Ok("https://api-sepolia.etherscan.io/api"),
            "polygon" => Ok("https://api.polygonscan.com/api"),
            "arbitrum" => Ok("https://api.arbiscan.io/api"),
            "optimism" => Ok("https://api-optimistic.etherscan.io/api"),
            "base" => Ok("https://api.basescan.org/api"),
            _ => Err(format!(
                "Unsupported network: {network}. Supported networks: ethereum, goerli, sepolia, polygon, arbitrum, optimism, base"
            )),
        }
    }

    /// Get the ABI for a verified contract from Etherscan
    #[tool(
        description = "Get the ABI (Application Binary Interface) for a verified contract from Etherscan. Returns the full ABI JSON that can be used to interact with the contract."
    )]
    pub async fn get_contract_abi(
        &self,
        Parameters(params): Parameters<GetAbiParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let network = params.network.as_deref().unwrap_or("ethereum");
        let api_url = Self::get_api_url(network).map_err(|e| ErrorData::invalid_params(e, None))?;

        // Validate address format
        if !params.address.starts_with("0x") || params.address.len() != 42 {
            return Err(ErrorData::invalid_params(
                "Invalid address format. Must be a 42-character hex string starting with 0x",
                None,
            ));
        }

        let response = self
            .client
            .get(api_url)
            .query(&[
                ("module", "contract"),
                ("action", "getabi"),
                ("address", &params.address),
                ("apikey", &self.api_key),
            ])
            .send()
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to send request: {e}"), None))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ErrorData::internal_error(
                format!("Etherscan API error: {status} - {error_text}"),
                None,
            ));
        }

        let etherscan_response: EtherscanResponse = response.json().await.map_err(|e| {
            ErrorData::internal_error(format!("Failed to parse response: {e}"), None)
        })?;

        // Check if the API call was successful
        if etherscan_response.status != "1" {
            // Common error messages
            let error_msg = match etherscan_response.message.as_str() {
                "NOTOK" => {
                    if etherscan_response.result.as_str()
                        == Some("Contract source code not verified")
                    {
                        format!(
                            "Contract at {} is not verified on Etherscan",
                            params.address
                        )
                    } else {
                        format!("Etherscan error: {}", etherscan_response.result)
                    }
                }
                _ => format!("Etherscan error: {}", etherscan_response.message),
            };
            return Err(ErrorData::internal_error(error_msg, None));
        }

        // The result should be a JSON string containing the ABI
        let abi_string = etherscan_response
            .result
            .as_str()
            .ok_or_else(|| ErrorData::internal_error("Invalid ABI format in response", None))?;

        // Parse the ABI to validate it and pretty-print
        let abi: serde_json::Value = serde_json::from_str(abi_string)
            .map_err(|e| ErrorData::internal_error(format!("Invalid ABI JSON: {e}"), None))?;

        // Format the output
        let mut output = format!("Contract ABI for {} on {}:\n\n", params.address, network);

        // Add a summary of available functions
        if let Some(abi_array) = abi.as_array() {
            output.push_str("Available functions:\n");
            for item in abi_array {
                if let (Some("function"), Some(name)) = (
                    item.get("type").and_then(|t| t.as_str()),
                    item.get("name").and_then(|n| n.as_str()),
                ) {
                    // Build function signature
                    let mut signature = format!("- {name}(");
                    if let Some(inputs) = item.get("inputs").and_then(|i| i.as_array()) {
                        let param_types: Vec<String> = inputs
                            .iter()
                            .filter_map(|input| input.get("type").and_then(|t| t.as_str()))
                            .map(|t| t.to_string())
                            .collect();
                        signature.push_str(&param_types.join(","));
                    }
                    signature.push_str(")\n");
                    output.push_str(&signature);
                }
            }
            output.push('\n');
        }

        // Add the full ABI
        output.push_str("Full ABI:\n");
        output.push_str(&serde_json::to_string_pretty(&abi).unwrap_or(abi_string.to_string()));

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Get transaction history for an address from Etherscan
    #[tool(
        description = "Get transaction history for an Ethereum address from Etherscan using chain ID"
    )]
    pub async fn get_transaction_history(
        &self,
        Parameters(params): Parameters<GetTransactionHistoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Validate address format
        if !params.address.starts_with("0x") || params.address.len() != 42 {
            return Err(ErrorData::invalid_params(
                "Invalid address format. Must be a 42-character hex string starting with 0x",
                None,
            ));
        }

        let chainid_str = params.chainid.to_string();

        let response = self
            .client
            .get("https://api.etherscan.io/v2/api")
            .query(&[
                ("chainid", chainid_str.as_str()),
                ("module", "account"),
                ("action", "txlist"),
                ("address", params.address.as_str()),
                ("startblock", "0"),
                ("endblock", "latest"),
                ("page", "1"),
                ("offset", "1000"),
                ("sort", "asc"),
                ("apikey", self.api_key.as_str()),
            ])
            .send()
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to send request: {e}"), None))?;

        if !response.status().is_success() {
            return Err(ErrorData::internal_error("API request failed", None));
        }

        let etherscan_response: EtherscanResponse = response.json().await.map_err(|e| {
            ErrorData::internal_error(format!("Failed to parse response: {e}"), None)
        })?;

        if etherscan_response.status != "1" {
            return Err(ErrorData::internal_error(
                format!("Etherscan error: {}", etherscan_response.message),
                None,
            ));
        }

        let output = format!(
            "Transaction history for {} on chain {}:\n\n{}",
            params.address,
            params.chainid,
            serde_json::to_string_pretty(&etherscan_response.result)
                .unwrap_or("Failed to format".to_string())
        );

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_transaction_history() {
        let tool = EtherscanTool::new("test_api_key".to_string());

        let params = GetTransactionHistoryParams {
            address: "0x742d35Cc6637C0532e6CE449B0f01B63C1C31138".to_string(),
            chainid: 1,
        };

        // This will fail without a real API key, but tests the function structure
        let result = tool
            .get_transaction_history(rmcp::handler::server::tool::Parameters(params))
            .await;

        // Should return an error due to invalid API key, but function should execute
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_address_format() {
        let tool = EtherscanTool::new("test_api_key".to_string());

        let params = GetTransactionHistoryParams {
            address: "invalid_address".to_string(),
            chainid: 1,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            tool.get_transaction_history(rmcp::handler::server::tool::Parameters(params))
                .await
        });

        assert!(result.is_err());
        if let Err(error) = result {
            assert!(error.message.contains("Invalid address format"));
        }
    }

    #[test]
    fn test_valid_address_format() {
        let params = GetTransactionHistoryParams {
            address: "0x742d35Cc6637C0532e6CE449B0f01B63C1C31138".to_string(),
            chainid: 1,
        };

        // Valid address should pass basic validation
        assert!(params.address.starts_with("0x"));
        assert_eq!(params.address.len(), 42);
    }
}
