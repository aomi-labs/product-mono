//! Wallet transaction tool for sending crafted transactions to user's wallet
use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Parameters for SendTransactionToWallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTransactionToWalletParameters {
    /// One-line note on what this transaction does
    pub topic: String,
    /// The recipient address (contract or EOA) - must be a valid Ethereum address
    pub to: String,
    /// Amount of ETH to send in wei (as string). Use '0' for contract calls with no ETH transfer
    pub value: String,
    /// The encoded function call data (from encode_function_call tool). Use '0x' for simple ETH transfers
    pub data: String,
    /// Optional gas limit for the transaction. If not provided, the wallet will estimate
    pub gas_limit: Option<String>,
    /// Human-readable description of what this transaction does, for user approval
    pub description: String,
}

/// Tool for sending crafted transactions to the user's wallet for approval and signing
#[derive(Debug, Clone)]
pub struct SendTransactionToWallet;

impl Tool for SendTransactionToWallet {
    const NAME: &'static str = "send_transaction_to_wallet";
    type Args = SendTransactionToWalletParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Send a crafted transaction to the user's wallet for approval and signing. This triggers a wallet popup in the frontend."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this transaction does"
                    },
                    "to": {
                        "type": "string",
                        "description": "The recipient address (contract or EOA) - must be a valid Ethereum address"
                    },
                    "value": {
                        "type": "string",
                        "description": "Amount of ETH to send in wei (as string). Use '0' for contract calls with no ETH transfer"
                    },
                    "data": {
                        "type": "string",
                        "description": "The encoded function call data (from encode_function_call tool). Use '0x' for simple ETH transfers"
                    },
                    "gas_limit": {
                        "type": "string",
                        "description": "Optional gas limit for the transaction. If not provided, the wallet will estimate"
                    },
                    "description": {
                        "type": "string",
                        "description": "Human-readable description of what this transaction does, for user approval"
                    }
                },
                "required": ["topic", "to", "value", "data", "description"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let SendTransactionToWalletParameters {
            topic: _topic,
            to,
            value,
            data,
            gas_limit,
            description,
        } = args;

        // Validate the 'to' address format
        if !to.starts_with("0x") || to.len() != 42 {
            return Err(ToolError::ToolCallError(
                "Invalid 'to' address: must be a valid Ethereum address starting with 0x".into(),
            ));
        }

        // Validate the value format (should be a valid number string)
        if value.parse::<u128>().is_err() {
            return Err(ToolError::ToolCallError(
                "Invalid 'value': must be a valid number in wei".into(),
            ));
        }

        // Validate the data format (should be valid hex)
        if !data.starts_with("0x") {
            return Err(ToolError::ToolCallError(
                "Invalid 'data': must be valid hex data starting with 0x".into(),
            ));
        }

        // Validate gas_limit if provided
        if let Some(ref gas) = gas_limit
            && gas.parse::<u64>().is_err()
        {
            return Err(ToolError::ToolCallError(
                "Invalid 'gas_limit': must be a valid number".into(),
            ));
        }

        // Create the transaction request object that will be sent to frontend
        let tx_request = json!({
            "to": to,
            "value": value,
            "data": data,
            "gas": gas_limit,
            "description": description,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        // Return a marker that the backend will detect and convert to SSE event
        // The backend will parse this and send it as a WalletTransactionRequest event
        Ok(tx_request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::tool::Tool;

    #[tokio::test]
    async fn test_simple_eth_transfer() {
        let tool = SendTransactionToWallet;
        let args = SendTransactionToWalletParameters {
            topic: "Send 1 ETH to recipient".to_string(),
            to: "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            value: "1000000000000000000".to_string(), // 1 ETH in wei
            data: "0x".to_string(),
            gas_limit: None,
            description: "Send 1 ETH to recipient".to_string(),
        };

        let result = tool.call(args).await.unwrap();

        assert_eq!(
            result.get("to").and_then(|v| v.as_str()),
            Some("0x742d35Cc6634C0532925a3b844Bc9e7595f33749")
        );
        assert_eq!(
            result.get("value").and_then(|v| v.as_str()),
            Some("1000000000000000000")
        );
        assert_eq!(result.get("data").and_then(|v| v.as_str()), Some("0x"));
        assert_eq!(result.get("gas").and_then(|v| v.as_str()), None); // None when not provided
        assert_eq!(
            result.get("description").and_then(|v| v.as_str()),
            Some("Send 1 ETH to recipient")
        );
        assert!(result.get("timestamp").and_then(|v| v.as_str()).is_some());
    }

    #[tokio::test]
    async fn test_contract_call() {
        let tool = SendTransactionToWallet;
        let args = SendTransactionToWalletParameters {
            topic: "Transfer 1000 USDC to recipient".to_string(),
            to: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(), // USDC contract
            value: "0".to_string(),
            data: "0xa9059cbb000000000000000000000000742d35cc6634c0532925a3b844bc9e7595f337490000000000000000000000000000000000000000000000000de0b6b3a7640000".to_string(),
            gas_limit: Some("100000".to_string()),
            description: "Transfer 1000 USDC to recipient".to_string(),
        };

        let result = tool.call(args).await.unwrap();

        assert_eq!(
            result.get("to").and_then(|v| v.as_str()),
            Some("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")
        );
        assert_eq!(result.get("value").and_then(|v| v.as_str()), Some("0"));
        let data = result.get("data").and_then(|v| v.as_str()).unwrap_or("");
        assert!(data.starts_with("0xa9059cbb"));
        assert_eq!(result.get("gas").and_then(|v| v.as_str()), Some("100000"));
        assert_eq!(
            result.get("description").and_then(|v| v.as_str()),
            Some("Transfer 1000 USDC to recipient")
        );
        assert!(result.get("timestamp").and_then(|v| v.as_str()).is_some());
    }

    #[tokio::test]
    async fn test_invalid_address() {
        let tool = SendTransactionToWallet;
        let args = SendTransactionToWalletParameters {
            topic: "Test transaction".to_string(),
            to: "invalid_address".to_string(),
            value: "1000000000000000000".to_string(),
            data: "0x".to_string(),
            gas_limit: None,
            description: "Test transaction".to_string(),
        };

        let result = tool.call(args).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid 'to' address")
        );
    }

    #[tokio::test]
    async fn test_invalid_value() {
        let tool = SendTransactionToWallet;
        let args = SendTransactionToWalletParameters {
            topic: "Test transaction".to_string(),
            to: "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            value: "not_a_number".to_string(),
            data: "0x".to_string(),
            gas_limit: None,
            description: "Test transaction".to_string(),
        };

        let result = tool.call(args).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid 'value'"));
    }
}
