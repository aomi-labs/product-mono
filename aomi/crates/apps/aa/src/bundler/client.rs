use super::types::{GasEstimate, UserOperationReceipt};
use crate::user_operation::UserOperation;
use alloy_primitives::{Address, FixedBytes};
use eyre::{Context, Result};
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;
use tracing::debug;

pub struct BundlerClient {
    rpc_url: String,
    entry_point: Address,
    client: reqwest::Client,
}

impl BundlerClient {
    pub fn new(rpc_url: String, entry_point: Address) -> Self {
        Self {
            rpc_url,
            entry_point,
            client: reqwest::Client::new(),
        }
    }

    /// Check bundler health (eth_supportedEntryPoints)
    pub async fn supported_entry_points(&self) -> Result<Vec<Address>> {
        let response = self.call_rpc("eth_supportedEntryPoints", json!([])).await?;

        serde_json::from_value(response).context("Failed to parse supported entry points")
    }

    /// Estimate UserOperation gas
    pub async fn estimate_user_operation_gas(
        &self,
        user_op: &UserOperation,
    ) -> Result<GasEstimate> {
        // Send unpacked UserOperation to bundler RPC
        tracing::debug!(
            "Sending UserOperation to Alto: {}",
            serde_json::to_string_pretty(user_op)?
        );

        let response = self
            .call_rpc(
                "eth_estimateUserOperationGas",
                json!([user_op, self.entry_point]),
            )
            .await?;

        serde_json::from_value(response).context("Failed to parse gas estimate")
    }

    /// Send UserOperation
    pub async fn send_user_operation(&self, user_op: &UserOperation) -> Result<FixedBytes<32>> {
        // Send unpacked UserOperation to bundler RPC
        let response = self
            .call_rpc("eth_sendUserOperation", json!([user_op, self.entry_point]))
            .await?;

        serde_json::from_value(response).context("Failed to parse user operation hash")
    }

    /// Get UserOperation receipt
    pub async fn get_user_operation_receipt(
        &self,
        user_op_hash: FixedBytes<32>,
    ) -> Result<Option<UserOperationReceipt>> {
        let response = self
            .call_rpc("eth_getUserOperationReceipt", json!([user_op_hash]))
            .await?;

        if response.is_null() {
            return Ok(None);
        }

        serde_json::from_value(response)
            .map(Some)
            .context("Failed to parse user operation receipt")
    }

    /// Poll for receipt with timeout
    pub async fn wait_for_receipt(
        &self,
        user_op_hash: FixedBytes<32>,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<UserOperationReceipt> {
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                eyre::bail!("Timeout waiting for UserOperation receipt");
            }

            debug!("Polling for receipt: {:?}", user_op_hash);

            if let Some(receipt) = self.get_user_operation_receipt(user_op_hash).await? {
                return Ok(receipt);
            }

            sleep(poll_interval).await;
        }
    }

    /// Low-level RPC call
    async fn call_rpc(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        debug!("RPC request to {}: {}", self.rpc_url, method);

        let response = self
            .client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .context("Failed to send RPC request")?
            .json::<serde_json::Value>()
            .await
            .context("Failed to parse RPC response")?;

        if let Some(error) = response.get("error") {
            eyre::bail!("RPC error: {}", error);
        }

        response
            .get("result")
            .cloned()
            .ok_or_else(|| eyre::eyre!("Missing result in RPC response"))
    }
}
