use once_cell::sync::Lazy;
use reqwest::Client;
use rig_derive::rig_tool;
use serde::Deserialize;
use std::future::Future;
use tokio::task;
use tracing::{info, warn};

static ETHERSCAN_API_KEY: Lazy<Option<String>> =
    Lazy::new(|| std::env::var("ETHERSCAN_API_KEY").ok());
static HTTP_CLIENT: Lazy<Client> = Lazy::new(Client::new);

fn tool_error(message: impl Into<String>) -> rig::tool::ToolError {
    rig::tool::ToolError::ToolCallError(message.into().into())
}

fn run_async<F, T>(future: F) -> Result<T, rig::tool::ToolError>
where
    F: Future<Output = Result<T, rig::tool::ToolError>> + Send + 'static,
    T: Send + 'static,
{
    task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

fn require_api_key() -> Result<&'static str, rig::tool::ToolError> {
    ETHERSCAN_API_KEY
        .as_ref()
        .map(|s| s.as_str())
        .ok_or_else(|| {
            warn!(
                target: "aomi_tools::etherscan",
                "ETHERSCAN_API_KEY is not set; explorer requests will fail"
            );
            tool_error("ETHERSCAN_API_KEY is not set in the environment")
        })
}

fn api_url_for_network(network: &str) -> Result<&'static str, rig::tool::ToolError> {
    match network {
        "mainnet" => Ok("https://api.etherscan.io/api"),
        "goerli" => Ok("https://api-goerli.etherscan.io/api"),
        "sepolia" => Ok("https://api-sepolia.etherscan.io/api"),
        "polygon" => Ok("https://api.polygonscan.com/api"),
        "arbitrum" => Ok("https://api.arbiscan.io/api"),
        "optimism" => Ok("https://api-optimistic.etherscan.io/api"),
        "base" => Ok("https://api.basescan.org/api"),
        other => {
            warn!(
                target: "aomi_tools::etherscan",
                network = %other,
                "Unsupported Etherscan-compatible network requested"
            );
            Err(tool_error(format!(
                "Unsupported network '{other}'. Supported: mainnet, goerli, sepolia, polygon, arbitrum, optimism, base"
            )))
        }
    }
}

fn validate_address(address: &str) -> Result<(), rig::tool::ToolError> {
    if address.starts_with("0x") && address.len() == 42 {
        Ok(())
    } else {
        Err(tool_error(
            "Invalid address format. Must be a 42-character hex string starting with 0x",
        ))
    }
}

#[derive(Deserialize)]
struct EtherscanResponse {
    status: String,
    message: String,
    result: serde_json::Value,
}

#[rig_tool(
    description = "Fetch the ABI for a verified contract using the configured Etherscan-compatible API.",
    params(
        address = "Contract address (must be verified on the selected explorer)",
        network = "Explorer network (mainnet, goerli, sepolia, polygon, arbitrum, optimism, base). Defaults to mainnet."
    ),
    required(address)
)]
pub fn get_contract_abi(
    address: String,
    network: Option<String>,
) -> Result<String, rig::tool::ToolError> {
    run_async(async move {
        if let Err(err) = validate_address(&address) {
            warn!(
                target: "aomi_tools::etherscan",
                address = %address,
                error = %err,
                "Rejected contract ABI request due to invalid address"
            );
            return Err(err);
        }

        let network = network.unwrap_or_else(|| "mainnet".to_string());
        info!(
            target: "aomi_tools::etherscan",
            address = %address,
            network = %network,
            "Fetching contract ABI from explorer"
        );

        let api_url = api_url_for_network(&network)?;
        let api_key = require_api_key()?;

        let response = HTTP_CLIENT
            .get(api_url)
            .query(&[
                ("module", "contract"),
                ("action", "getabi"),
                ("address", address.as_str()),
                ("apikey", api_key),
            ])
            .send()
            .await
            .map_err(|e| {
                warn!(
                    target: "aomi_tools::etherscan",
                    address = %address,
                    network = %network,
                    error = %e,
                    "Explorer request failed"
                );
                tool_error(format!("Failed to contact Etherscan: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            warn!(
                target: "aomi_tools::etherscan",
                address = %address,
                network = %network,
                status = %status,
                body = %body,
                "Explorer returned non-success response"
            );
            return Err(tool_error(format!("Etherscan API error {status}: {body}")));
        }

        let payload: EtherscanResponse = response.json().await.map_err(|e| {
            warn!(
                target: "aomi_tools::etherscan",
                address = %address,
                network = %network,
                error = %e,
                "Explorer response JSON parse failed"
            );
            tool_error(format!("Failed to parse Etherscan response: {e}"))
        })?;

        if payload.status != "1" {
            let msg = match payload.result.as_str() {
                Some("Contract source code not verified") => {
                    format!("Contract at {} is not verified on {}", address, network)
                }
                Some(detail) => format!("Etherscan error: {detail}"),
                None => format!("Etherscan error: {}", payload.message),
            };
            warn!(
                target: "aomi_tools::etherscan",
                address = %address,
                network = %network,
                message = %msg,
                "Explorer signaled failure"
            );
            return Err(tool_error(msg));
        }

        let abi_raw = payload
            .result
            .as_str()
            .ok_or_else(|| tool_error("Invalid ABI payload in response"))?;

        let abi: serde_json::Value = serde_json::from_str(abi_raw)
            .map_err(|e| tool_error(format!("Invalid ABI JSON: {e}")))?;

        let mut summary = format!("Contract ABI for {address} on {network}:\n\n");
        if let Some(items) = abi.as_array() {
            summary.push_str("Available functions:\n");
            for item in items {
                if let (Some("function"), Some(name)) = (
                    item.get("type").and_then(|t| t.as_str()),
                    item.get("name").and_then(|n| n.as_str()),
                ) {
                    let params = item
                        .get("inputs")
                        .and_then(|inputs| inputs.as_array())
                        .map(|inputs| {
                            inputs
                                .iter()
                                .filter_map(|input| input.get("type").and_then(|t| t.as_str()))
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .unwrap_or_default();
                    summary.push_str(&format!("- {name}({params})\n"));
                }
            }
            summary.push('\n');
        }

        summary.push_str("Full ABI:\n");
        summary
            .push_str(&serde_json::to_string_pretty(&abi).unwrap_or_else(|_| abi_raw.to_string()));

        if let Some(items) = abi.as_array() {
            info!(
                target: "aomi_tools::etherscan",
                address = %address,
                network = %network,
                function_count = items.len(),
                "Explorer ABI fetch complete"
            );
        } else {
            info!(
                target: "aomi_tools::etherscan",
                address = %address,
                network = %network,
                "Explorer ABI fetch complete (non-array payload)"
            );
        }

        Ok(summary)
    })
}

impl_rig_tool_clone!(GetContractAbi, GetContractAbiParameters, [address, network]);

#[rig_tool(
    description = "Retrieve recent transaction history for an address via Etherscan v2 API (up to the first 1000 records).",
    params(
        address = "Account address to inspect",
        chainid = "Chain ID (e.g., 1 for mainnet, 137 for polygon, 42161 for arbitrum)"
    ),
    required(address, chainid)
)]
pub fn get_transaction_history(
    address: String,
    chainid: u32,
) -> Result<String, rig::tool::ToolError> {
    run_async(async move {
        if let Err(err) = validate_address(&address) {
            warn!(
                target: "aomi_tools::etherscan",
                address = %address,
                chain_id = chainid,
                error = %err,
                "Rejected transaction history request due to invalid address"
            );
            return Err(err);
        }

        info!(
            target: "aomi_tools::etherscan",
            address = %address,
            chain_id = chainid,
            "Fetching transaction history from explorer"
        );

        let api_key = require_api_key()?;

        let response = HTTP_CLIENT
            .get("https://api.etherscan.io/v2/api")
            .query(&[
                ("chainid", chainid.to_string()),
                ("module", "account".to_string()),
                ("action", "txlist".to_string()),
                ("address", address.clone()),
                ("startblock", "0".to_string()),
                ("endblock", "latest".to_string()),
                ("page", "1".to_string()),
                ("offset", "1000".to_string()),
                ("sort", "asc".to_string()),
                ("apikey", api_key.to_string()),
            ])
            .send()
            .await
            .map_err(|e| {
                warn!(
                    target: "aomi_tools::etherscan",
                    address = %address,
                    chain_id = chainid,
                    error = %e,
                    "Explorer request for transaction history failed"
                );
                tool_error(format!("Failed to contact Etherscan: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            warn!(
                target: "aomi_tools::etherscan",
                address = %address,
                chain_id = chainid,
                status = %status,
                "Explorer returned non-success response"
            );
            return Err(tool_error(format!("Etherscan API error: HTTP {}", status)));
        }

        let payload: EtherscanResponse = response.json().await.map_err(|e| {
            warn!(
                target: "aomi_tools::etherscan",
                address = %address,
                chain_id = chainid,
                error = %e,
                "Explorer response JSON parse failed"
            );
            tool_error(format!("Failed to parse Etherscan response: {e}"))
        })?;

        if payload.status != "1" {
            let message = payload
                .result
                .as_str()
                .or(payload.message.as_str().into())
                .unwrap_or("Unknown error");
            warn!(
                target: "aomi_tools::etherscan",
                address = %address,
                chain_id = chainid,
                message = %message,
                "Explorer returned failure status"
            );
            return Err(tool_error(format!("Etherscan error: {}", message)));
        }

        let formatted = serde_json::to_string_pretty(&payload.result)
            .unwrap_or_else(|_| payload.result.to_string());

        let entry_count = payload.result.as_array().map(|arr| arr.len());
        info!(
            target: "aomi_tools::etherscan",
            address = %address,
            chain_id = chainid,
            entry_count = entry_count.unwrap_or(0),
            "Explorer transaction history fetch complete"
        );

        Ok(format!(
            "Transaction history for {address} on chain {chainid}:\n\n{formatted}"
        ))
    })
}

impl_rig_tool_clone!(
    GetTransactionHistory,
    GetTransactionHistoryParameters,
    [address, chainid]
);
