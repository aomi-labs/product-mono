//! Wallet transaction tool for sending crafted transactions to user's wallet
use rig_derive::rig_tool;
use serde_json::json;

#[rig_tool(
    description = "Send a crafted transaction to the user's wallet for approval and signing. This triggers a wallet popup in the frontend.",
    params(
        to = "The recipient address (contract or EOA) - must be a valid Ethereum address",
        value = "Amount of ETH to send in wei (as string). Use '0' for contract calls with no ETH transfer",
        data = "The encoded function call data (from encode_function_call tool). Use '0x' for simple ETH transfers",
        gas_limit = "Optional gas limit for the transaction. If not provided, the wallet will estimate",
        description = "Human-readable description of what this transaction does, for user approval"
    ),
    required(to, value, data, description)
)]
pub(crate) fn send_transaction_to_wallet(
    to: String,
    value: String,
    data: String,
    gas_limit: Option<String>,
    description: String,
) -> Result<serde_json::Value, rig::tool::ToolError> {
    // Validate the 'to' address format
    if !to.starts_with("0x") || to.len() != 42 {
        return Err(rig::tool::ToolError::ToolCallError(
            "Invalid 'to' address: must be a valid Ethereum address starting with 0x".into(),
        ));
    }

    // Validate the value format (should be a valid number string)
    if value.parse::<u128>().is_err() {
        return Err(rig::tool::ToolError::ToolCallError(
            "Invalid 'value': must be a valid number in wei".into(),
        ));
    }

    // Validate the data format (should be valid hex)
    if !data.starts_with("0x") {
        return Err(rig::tool::ToolError::ToolCallError(
            "Invalid 'data': must be valid hex data starting with 0x".into(),
        ));
    }

    // Validate gas_limit if provided
    if let Some(ref gas) = gas_limit
        && gas.parse::<u64>().is_err()
    {
        return Err(rig::tool::ToolError::ToolCallError(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_eth_transfer() {
        let result = send_transaction_to_wallet(
            "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            "1000000000000000000".to_string(), // 1 ETH in wei
            "0x".to_string(),
            None,
            "Send 1 ETH to recipient".to_string(),
        )
        .unwrap();

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

    #[test]
    fn test_contract_call() {
        let result = send_transaction_to_wallet(
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(), // USDC contract
            "0".to_string(),
            "0xa9059cbb000000000000000000000000742d35cc6634c0532925a3b844bc9e7595f337490000000000000000000000000000000000000000000000000de0b6b3a7640000".to_string(),
            Some("100000".to_string()),
            "Transfer 1000 USDC to recipient".to_string(),
        ).unwrap();

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

    #[test]
    fn test_invalid_address() {
        let result = send_transaction_to_wallet(
            "invalid_address".to_string(),
            "1000000000000000000".to_string(),
            "0x".to_string(),
            None,
            "Test transaction".to_string(),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid 'to' address")
        );
    }

    #[test]
    fn test_invalid_value() {
        let result = send_transaction_to_wallet(
            "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            "not_a_number".to_string(),
            "0x".to_string(),
            None,
            "Test transaction".to_string(),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid 'value'"));
    }
}

// Manual Clone implementations for the generated structs
impl Clone for SendTransactionToWallet {
    fn clone(&self) -> Self {
        Self
    }
}

impl Clone for SendTransactionToWalletParameters {
    fn clone(&self) -> Self {
        Self {
            to: self.to.clone(),
            value: self.value.clone(),
            data: self.data.clone(),
            gas_limit: self.gas_limit.clone(),
            description: self.description.clone(),
        }
    }
}
