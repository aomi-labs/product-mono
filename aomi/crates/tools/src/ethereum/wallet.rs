//! Wallet transaction tool for sending crafted transactions to user's wallet
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info, warn};

use super::gateway::{WalletTransactionResult, get_gateway};
use crate::{AomiTool, AomiToolArgs, ToolCallCtx, with_topic};

/// Parameters for SendTransactionToWallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTransactionToWalletParameters {
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

impl AomiToolArgs for SendTransactionToWalletParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipient address (contract or EOA)"
                },
                "value": {
                    "type": "string",
                    "description": "Amount of ETH to send in wei (string). Use \"0\" for contract calls with no ETH transfer"
                },
                "data": {
                    "type": "string",
                    "description": "Encoded calldata (use encode_function_call). Use \"0x\" for simple ETH transfers"
                },
                "gas_limit": {
                    "type": "string",
                    "description": "Optional gas limit. If omitted, wallet estimates"
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description for user approval"
                }
            },
            "required": ["to", "value", "data", "description"]
        }))
    }
}

/// Tool for sending crafted transactions to the user's wallet for approval and signing
#[derive(Debug, Clone)]
pub struct SendTransactionToWallet;

/// Validate input parameters for the wallet transaction.
fn validate_params(args: &SendTransactionToWalletParameters) -> Result<(), ToolError> {
    // Validate the 'to' address format
    if !args.to.starts_with("0x") || args.to.len() != 42 {
        warn!(to = %args.to, "Invalid 'to' address provided to wallet tool");
        return Err(ToolError::ToolCallError(
            "Invalid 'to' address: must be a valid Ethereum address starting with 0x".into(),
        ));
    }

    // Validate the value format (should be a valid number string)
    if args.value.parse::<u128>().is_err() {
        warn!(value = %args.value, "Invalid 'value' provided to wallet tool");
        return Err(ToolError::ToolCallError(
            "Invalid 'value': must be a valid number in wei".into(),
        ));
    }

    // Validate the data format (should be valid hex)
    if !args.data.starts_with("0x") {
        warn!("Invalid calldata provided – missing 0x prefix");
        return Err(ToolError::ToolCallError(
            "Invalid 'data': must be valid hex data starting with 0x".into(),
        ));
    }
    let hex = args.data.trim_start_matches("0x");
    if !hex.is_empty() {
        if !hex.len().is_multiple_of(2) {
            warn!("Invalid calldata provided – odd-length hex");
            return Err(ToolError::ToolCallError(
                "Invalid 'data': hex length must be even. Use encode_function_call output verbatim."
                    .into(),
            ));
        }
        if let Some((idx, ch)) = hex.char_indices().find(|(_, ch)| !ch.is_ascii_hexdigit()) {
            warn!("Invalid calldata provided – non-hex character");
            return Err(ToolError::ToolCallError(
                format!(
                    "Invalid 'data': non-hex character '{}' at index {}. Use encode_function_call output verbatim.",
                    ch, idx
                )
                .into(),
            ));
        }
    }

    // Validate gas_limit if provided
    if let Some(ref gas) = args.gas_limit {
        debug!(gas_limit = %gas, "Validating provided gas limit");
        if gas.parse::<u64>().is_err() {
            warn!(gas_limit = %gas, "Invalid 'gas_limit' provided to wallet tool");
            return Err(ToolError::ToolCallError(
                "Invalid 'gas_limit': must be a valid number".into(),
            ));
        }
    }

    Ok(())
}

/// Execute the wallet transaction via the gateway.
///
/// In production mode, this returns a `PendingApproval` result for the frontend.
/// In eval-test mode with an autosign wallet, this executes the transaction directly.
pub async fn execute_call(
    ctx: ToolCallCtx,
    args: SendTransactionToWalletParameters,
) -> Result<serde_json::Value, ToolError> {
    // Validate input parameters
    validate_params(&args)?;

    let SendTransactionToWalletParameters {
        to,
        value,
        data,
        gas_limit,
        description,
    } = args;

    let has_data = data.as_str() != "0x";
    let gas_limit_display = gas_limit.as_deref().unwrap_or("auto");
    info!(
        to = %to,
        value = %value,
        has_data = has_data,
        gas_limit = %gas_limit_display,
        "Preparing wallet transaction request"
    );

    // Get user's wallet address from context
    let from = ctx.user_address.as_deref().unwrap_or("");
    if from.is_empty() {
        warn!("No wallet connected - cannot send transaction");
        return Err(ToolError::ToolCallError(
            "No wallet connected. Please connect your wallet first.".into(),
        ));
    }

    debug!(from = %from, description = %description, "Sending transaction via gateway");

    // Use the gateway to send the transaction
    let gateway = get_gateway()
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Failed to get gateway: {}", e).into()))?;

    let result = gateway
        .send_transaction_to_wallet(from, &to, &value, &data, gas_limit.as_deref(), &description)
        .await
        .map_err(|e| ToolError::ToolCallError(format!("Transaction failed: {}", e).into()))?;

    // Convert result to JSON
    match &result {
        WalletTransactionResult::Confirmed { tx_hash, .. } => {
            info!(tx_hash = %tx_hash, "Transaction auto-signed and confirmed");
        }
        WalletTransactionResult::PendingApproval { .. } => {
            info!("Transaction request created, pending user approval");
        }
    }

    serde_json::to_value(result)
        .map_err(|e| ToolError::ToolCallError(format!("Failed to serialize result: {}", e).into()))
}

/// Legacy execute_call without context (for backward compatibility in tests).
#[cfg(test)]
pub async fn execute_call_legacy(
    args: SendTransactionToWalletParameters,
) -> Result<serde_json::Value, ToolError> {
    // Validate input parameters
    validate_params(&args)?;

    let SendTransactionToWalletParameters {
        to,
        value,
        data,
        gas_limit,
        description,
    } = args;

    // Create the transaction request object (legacy format for tests)
    let tx_request = json!({
        "to": to,
        "value": value,
        "data": data,
        "gas": gas_limit,
        "description": description,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    Ok(tx_request)
}

impl AomiTool for SendTransactionToWallet {
    const NAME: &'static str = "send_transaction_to_wallet";

    type Args = SendTransactionToWalletParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Send a crafted transaction to the user's wallet for approval and signing. REQUIRED: Before sending, you MUST use simulate_contract_call with the same parameters to validate the transaction will succeed. Only send transactions after successful simulation. The data field must be 0x-prefixed hex (use encode_function_call first)."
    }

    fn run_sync(
        &self,
        ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_call(ctx, args)
                .await
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_eth_transfer() {
        let args = SendTransactionToWalletParameters {
            to: "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            value: "1000000000000000000".to_string(), // 1 ETH in wei
            data: "0x".to_string(),
            gas_limit: None,
            description: "Send 1 ETH to recipient".to_string(),
        };

        let result = execute_call_legacy(args).await.unwrap();

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
        let args = SendTransactionToWalletParameters {
            to: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(), // USDC contract
            value: "0".to_string(),
            data: "0xa9059cbb000000000000000000000000742d35cc6634c0532925a3b844bc9e7595f337490000000000000000000000000000000000000000000000000de0b6b3a7640000".to_string(),
            gas_limit: Some("100000".to_string()),
            description: "Transfer 1000 USDC to recipient".to_string(),
        };

        let result = execute_call_legacy(args).await.unwrap();

        assert_eq!(
            result.get("to").and_then(|v| v.as_str()),
            Some("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")
        );
        assert_eq!(result.get("value").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(
            result.get("data").and_then(|v| v.as_str()),
            Some(
                "0xa9059cbb000000000000000000000000742d35cc6634c0532925a3b844bc9e7595f337490000000000000000000000000000000000000000000000000de0b6b3a7640000"
            )
        );
        assert_eq!(result.get("gas").and_then(|v| v.as_str()), Some("100000"));
        assert_eq!(
            result.get("description").and_then(|v| v.as_str()),
            Some("Transfer 1000 USDC to recipient")
        );
        assert!(result.get("timestamp").and_then(|v| v.as_str()).is_some());
    }

    #[tokio::test]
    async fn test_invalid_address() {
        let args = SendTransactionToWalletParameters {
            to: "invalid_address".to_string(),
            value: "1000000000000000000".to_string(),
            data: "0x".to_string(),
            gas_limit: None,
            description: "Test invalid address handling".to_string(),
        };

        let result = execute_call_legacy(args).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid 'to' address"));
    }

    #[tokio::test]
    async fn test_invalid_value() {
        let args = SendTransactionToWalletParameters {
            to: "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            value: "not_a_number".to_string(),
            data: "0x".to_string(),
            gas_limit: None,
            description: "Test invalid value handling".to_string(),
        };

        let result = execute_call_legacy(args).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid 'value'"));
    }

    #[tokio::test]
    async fn test_invalid_data_prefix() {
        let args = SendTransactionToWalletParameters {
            to: "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            value: "0".to_string(),
            data: "1234".to_string(),
            gas_limit: None,
            description: "Test missing 0x prefix".to_string(),
        };

        let result = execute_call_legacy(args).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid 'data'"));
    }

    #[tokio::test]
    async fn test_invalid_gas_limit() {
        let args = SendTransactionToWalletParameters {
            to: "0x742d35Cc6634C0532925a3b844Bc9e7595f33749".to_string(),
            value: "0".to_string(),
            data: "0x".to_string(),
            gas_limit: Some("not_a_number".to_string()),
            description: "Test invalid gas limit".to_string(),
        };

        let result = execute_call_legacy(args).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid 'gas_limit'"));
    }
}
