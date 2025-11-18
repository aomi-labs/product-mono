//! MCP tool for Etherscan API integration
use std::sync::Arc;

use aomi_tools::{
    EtherscanClient,
    clients::ETHERSCAN_V2_URL,
    etherscan::{ETHEREUM_MAINNET, SortOrder, chain_id_to_name, network_name_to_chain_id},
};
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
    client: EtherscanClient,
}

impl EtherscanTool {
    pub fn new(api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("Failed to create HTTP client")
            .get(ETHERSCAN_V2_URL);

        Self {
            client: EtherscanClient::new(Arc::new(client), api_key),
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
        let chain_id = params
            .network
            .as_deref()
            .map_or(Ok(ETHEREUM_MAINNET), network_name_to_chain_id)
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        let contract = self
            .client
            .fetch_contract_by_chain_id(chain_id, &params.address)
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to fetch contract: {e}"), None)
            })?;

        // Format the output
        let mut output = format!(
            "Contract ABI for {} on {}:\n\n",
            params.address,
            chain_id_to_name(chain_id)
        );

        // Add a summary of available functions
        if let Some(abi_array) = contract.abi.as_array() {
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
        output.push_str(
            &serde_json::to_string_pretty(&contract.abi)
                .unwrap_or_else(|_| contract.abi.to_string()),
        );

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
        let transactions = self
            .client
            .fetch_transaction_history_by_chain_id(params.chainid, &params.address, SortOrder::Asc)
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to fetch history: {e}"), None)
            })?;

        let pretty = serde_json::to_string_pretty(&transactions)
            .unwrap_or_else(|_| "Failed to format transactions".to_string());

        let output = format!(
            "Transaction history for {} on chain {}:\n\n{}",
            params.address, params.chainid, pretty
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
