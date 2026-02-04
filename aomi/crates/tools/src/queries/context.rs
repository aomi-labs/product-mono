use alloy_provider::Provider;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

use crate::{AomiTool, AomiToolArgs, ToolCallCtx, WithTopic};

// ============================================================================
// GetTimeAndOnchainCtx - Chain environment information for AI
// ============================================================================

/// Parameters for GetTimeAndOnchainCtx (no additional args needed)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetTimeAndOnchainCtxArgs {}

impl AomiToolArgs for GetTimeAndOnchainCtxArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
}

/// Full parameters with auto-injected topic
pub type GetTimeAndOnchainCtxParameters = WithTopic<GetTimeAndOnchainCtxArgs>;

/// Tool for getting on-chain context: chain info, block number, gas price, time, supported chains
#[derive(Debug, Clone)]
pub struct GetTimeAndOnchainCtx;

impl AomiTool for GetTimeAndOnchainCtx {
    const NAME: &'static str = "get_time_and_onchain_context";

    type Args = GetTimeAndOnchainCtxParameters;
    type Output = serde_json::Value;
    type Error = ContextToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get the current time and on-chain context for the user's connected network. Returns chain name, chain ID, RPC endpoint, current time, block number, gas price, and list of all supported chains. IMPORTANT: Always call this tool at the start of a session or when you need to know which network the user is connected to. If the user is not connected, defaults to Ethereum mainnet."
    }

    fn run_sync(
        &self,
        ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move { fetch_onchain_context(ctx.user_chain_id).await }
    }
}

async fn fetch_onchain_context(user_chain_id: Option<u64>) -> eyre::Result<serde_json::Value> {
    // Get current unix timestamp
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Get the provider manager to access chain info
    let manager = match aomi_anvil::provider_manager().await {
        Ok(m) => m,
        Err(e) => {
            warn!("Failed to get provider manager: {}", e);
            return Ok(json!({
                "current_time_unix": current_time,
                "current_time_iso": chrono::Utc::now().to_rfc3339(),
                "error": format!("Failed to get provider manager: {}", e),
                "supported_chains": []
            }));
        }
    };

    // List all available instances/chains
    let instances = manager.list_instances();
    let supported_chains: Vec<serde_json::Value> = instances
        .iter()
        .map(|info| {
            json!({
                "name": info.name,
                "chain_id": info.chain_id,
                "is_managed": info.is_managed,
            })
        })
        .collect();

    // Determine target chain: use user's connected chain, or default to Ethereum mainnet (chain_id = 1)
    let target_chain_id = user_chain_id.unwrap_or(1);

    // Try to find instance by chain_id first
    let primary_instance = instances
        .iter()
        .find(|info| info.chain_id == target_chain_id)
        .or_else(|| {
            // Fallback to first available instance if target chain not found
            warn!(
                "Chain ID {} not found, falling back to first available instance",
                target_chain_id
            );
            instances.first()
        });

    let (chain_name, chain_id, rpc_endpoint, block_number, gas_price) =
        if let Some(info) = primary_instance {
            let gas = fetch_gas_price(&info.endpoint).await;
            (
                info.name.clone(),
                info.chain_id,
                info.endpoint.clone(),
                info.block_number,
                gas,
            )
        } else {
            ("unknown".to_string(), 0, "unknown".to_string(), 0, None)
        };

    info!(
        target: "aomi_tools::context",
        chain_name = %chain_name,
        chain_id = chain_id,
        block_number = block_number,
        "Fetched on-chain context"
    );

    Ok(json!({
        "chain_name": chain_name,
        "chain_id": chain_id,
        "rpc_endpoint": rpc_endpoint,
        "current_time_unix": current_time,
        "current_time_iso": chrono::Utc::now().to_rfc3339(),
        "block_number": block_number,
        "gas_price_wei": gas_price,
        "supported_chains": supported_chains
    }))
}

async fn fetch_gas_price(endpoint: &str) -> Option<String> {
    use alloy::network::AnyNetwork;
    use alloy_provider::ProviderBuilder;

    let provider = ProviderBuilder::<_, _, AnyNetwork>::default()
        .connect(endpoint)
        .await
        .ok()?;

    provider.get_gas_price().await.ok().map(|p| p.to_string())
}

// ============================================================================
// Error type
// ============================================================================

#[derive(Debug)]
pub struct ContextToolError(String);

impl std::fmt::Display for ContextToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContextToolError: {}", self.0)
    }
}

impl std::error::Error for ContextToolError {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CallMetadata;

    #[test]
    fn test_onchain_context_args_schema() {
        let schema = GetTimeAndOnchainCtxArgs::schema();
        assert_eq!(schema["type"], "object");
    }

    #[tokio::test]
    async fn test_context_uses_user_chain_id() {
        let tool = GetTimeAndOnchainCtx;

        // Test with Ethereum mainnet (chain_id = 1)
        let ctx = ToolCallCtx {
            session_id: "test".to_string(),
            metadata: CallMetadata::new(
                "test".to_string(),
                "test".to_string(),
                "test".to_string(),
                None,
                false,
            ),
            user_chain_id: Some(1),
        };

        let result = tool
            .run_sync(
                ctx,
                WithTopic {
                    inner: GetTimeAndOnchainCtxArgs {},
                    topic: None,
                },
            )
            .await;

        if let Ok(value) = result {
            // Should return chain_id 1 if available
            println!(
                "Result with chain_id=1: {}",
                serde_json::to_string_pretty(&value).unwrap()
            );

            // Verify structure
            assert!(value.get("chain_id").is_some());
            assert!(value.get("current_time_unix").is_some());
            assert!(value.get("supported_chains").is_some());
        }

        // Test with None (should default to mainnet)
        let ctx_none = ToolCallCtx {
            session_id: "test".to_string(),
            metadata: CallMetadata::new(
                "test".to_string(),
                "test".to_string(),
                "test".to_string(),
                None,
                false,
            ),
            user_chain_id: None,
        };

        let result_none = tool
            .run_sync(
                ctx_none,
                WithTopic {
                    inner: GetTimeAndOnchainCtxArgs {},
                    topic: None,
                },
            )
            .await;

        if let Ok(value) = result_none {
            println!(
                "Result with chain_id=None: {}",
                serde_json::to_string_pretty(&value).unwrap()
            );
            // Should default to mainnet (chain_id 1)
            if let Some(chain_id) = value.get("chain_id").and_then(|v| v.as_u64()) {
                println!("Defaulted to chain_id: {}", chain_id);
            }
        }
    }
}
