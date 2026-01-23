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
        "Get the current time and on-chain context including chain name, chain ID, RPC endpoint, current time, block number, gas price, and list of supported chains. Use this to understand which network you're operating on."
    }

    fn run_sync(
        &self,
        result_sender: tokio::sync::oneshot::Sender<eyre::Result<serde_json::Value>>,
        _ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let result = fetch_onchain_context().await;
            let _ = result_sender.send(result);
        }
    }
}

async fn fetch_onchain_context() -> eyre::Result<serde_json::Value> {
    // Get current unix timestamp
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Get the provider manager to access chain info
    let manager = match aomi_anvil::default_manager().await {
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

    // Get the primary/default instance (testnet)
    let primary_instance = manager.get_instance_info_by_name("testnet");

    let (chain_name, chain_id, rpc_endpoint, block_number, gas_price) =
        if let Some(info) = primary_instance {
            let gas = fetch_gas_price(&info.endpoint).await;
            (
                info.name,
                info.chain_id,
                info.endpoint,
                info.block_number,
                gas,
            )
        } else if let Some(first) = instances.first() {
            let gas = fetch_gas_price(&first.endpoint).await;
            (
                first.name.clone(),
                first.chain_id,
                first.endpoint.clone(),
                first.block_number,
                gas,
            )
        } else {
            (
                "unknown".to_string(),
                0,
                "unknown".to_string(),
                0,
                None,
            )
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

    provider
        .get_gas_price()
        .await
        .ok()
        .map(|p| p.to_string())
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

    #[test]
    fn test_onchain_context_args_schema() {
        let schema = GetTimeAndOnchainCtxArgs::schema();
        assert_eq!(schema["type"], "object");
    }
}
