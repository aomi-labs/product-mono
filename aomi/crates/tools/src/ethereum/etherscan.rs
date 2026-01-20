use crate::cast::ERC20;
use crate::clients::ETHERSCAN_V2_URL;
pub use crate::clients::EtherscanClient;
#[cfg(any(test, feature = "eval-test"))]
use crate::clients::ExternalClients;
use crate::clients::external_clients;
use crate::db::{Contract, ContractStore, ContractStoreApi};
use crate::db_tools::run_sync;
#[cfg(any(test, feature = "eval-test"))]
use alloy::dyn_abi::{DynSolType, DynSolValue};
#[cfg(any(test, feature = "eval-test"))]
use alloy::eips::{BlockId, BlockNumberOrTag, RpcBlockHash};
#[cfg(any(test, feature = "eval-test"))]
use alloy::primitives::{Address, BlockHash, Bytes};
#[cfg(any(test, feature = "eval-test"))]
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use anyhow::{Context, Result};
#[cfg(any(test, feature = "eval-test"))]
use cast::SimpleCast;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use sqlx::any::AnyPoolOptions;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::{AomiTool, AomiToolArgs, ToolCallCtx, with_topic};
use tokio::sync::oneshot;

// Chain ID constants
pub const ETHEREUM_MAINNET: u32 = 1;
pub const GOERLI: u32 = 5;
pub const SEPOLIA: u32 = 11155111;
pub const POLYGON: u32 = 137;
pub const ARBITRUM: u32 = 42161;
pub const OPTIMISM: u32 = 10;
pub const BASE: u32 = 8453;

/// Supported networks for the unified Etherscan API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Goerli,
    Sepolia,
    Polygon,
    Arbitrum,
    Optimism,
    Base,
}

impl Network {
    pub fn chain_id(self) -> u32 {
        match self {
            Network::Mainnet => ETHEREUM_MAINNET,
            Network::Goerli => GOERLI,
            Network::Sepolia => SEPOLIA,
            Network::Polygon => POLYGON,
            Network::Arbitrum => ARBITRUM,
            Network::Optimism => OPTIMISM,
            Network::Base => BASE,
        }
    }

    pub fn canonical_name(self) -> &'static str {
        match self {
            Network::Mainnet => "mainnet",
            Network::Goerli => "goerli",
            Network::Sepolia => "sepolia",
            Network::Polygon => "polygon",
            Network::Arbitrum => "arbitrum",
            Network::Optimism => "optimism",
            Network::Base => "base",
        }
    }
}

impl FromStr for Network {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "mainnet" | "ethereum" | "eth" => Ok(Network::Mainnet),
            "goerli" => Ok(Network::Goerli),
            "sepolia" => Ok(Network::Sepolia),
            "polygon" | "matic" => Ok(Network::Polygon),
            "arbitrum" | "arb" => Ok(Network::Arbitrum),
            "optimism" | "op" => Ok(Network::Optimism),
            "base" => Ok(Network::Base),
            other => anyhow::bail!("Unsupported network: {}", other),
        }
    }
}

impl TryFrom<u32> for Network {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            ETHEREUM_MAINNET => Ok(Network::Mainnet),
            GOERLI => Ok(Network::Goerli),
            SEPOLIA => Ok(Network::Sepolia),
            POLYGON => Ok(Network::Polygon),
            ARBITRUM => Ok(Network::Arbitrum),
            OPTIMISM => Ok(Network::Optimism),
            BASE => Ok(Network::Base),
            _ => anyhow::bail!("Unsupported chain id: {}", value),
        }
    }
}

/// Maps chain ID to chain name for database storage
pub fn chain_id_to_name(chain_id: u32) -> String {
    Network::try_from(chain_id)
        .map(|network| match network {
            Network::Mainnet => "ethereum".to_string(),
            other => other.canonical_name().to_string(),
        })
        .unwrap_or_else(|_| format!("chain_{}", chain_id))
}

/// Convert a user-supplied network string (e.g. "mainnet") into a chain ID.
pub fn network_name_to_chain_id(name: &str) -> Result<u32> {
    Ok(Network::from_str(name)?.chain_id())
}

impl EtherscanClient {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ETHERSCAN_API_KEY")
            .context("ETHERSCAN_API_KEY environment variable not set")?;
        let builder = Arc::new(crate::clients::build_http_client().get(ETHERSCAN_V2_URL));
        Ok(Self::new(builder, api_key))
    }

    fn validate_address(address: &str) -> Result<()> {
        if address.starts_with("0x") && address.len() == 42 {
            Ok(())
        } else {
            anyhow::bail!(
                "Invalid address format. Must be a 42-character hex string starting with 0x"
            )
        }
    }

    fn build_params(
        &self,
        chain_id: u32,
        mut params: Vec<(String, String)>,
    ) -> Vec<(String, String)> {
        params.push(("chainid".to_string(), chain_id.to_string()));
        params.push(("apikey".to_string(), self.api_key.clone()));
        params
    }

    async fn send_request<T>(&self, params: Vec<(String, String)>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let base = self
            .builder
            .try_clone()
            .unwrap_or_else(|| crate::clients::build_http_client().get(ETHERSCAN_V2_URL));
        let response = base
            .query(&params)
            .send()
            .await
            .context("Failed to send request to Etherscan")?;

        let response = response
            .error_for_status()
            .context("Etherscan API request failed")?;

        response
            .json::<T>()
            .await
            .context("Failed to parse Etherscan response")
    }

    /// Fetch contract metadata (source + ABI) for the supplied chain ID.
    pub async fn fetch_contract_by_chain_id(
        &self,
        chain_id: u32,
        address: &str,
    ) -> Result<Contract> {
        Self::validate_address(address)?;

        let params = self.build_params(
            chain_id,
            vec![
                ("module".to_string(), "contract".to_string()),
                ("action".to_string(), "getsourcecode".to_string()),
                ("address".to_string(), address.to_string()),
            ],
        );

        let response: EtherscanResponse<Vec<ContractSourceCode>> =
            self.send_request(params).await?;

        if response.status != "1" {
            anyhow::bail!(
                "Etherscan API error for chain {}: status='{}', message='{}'",
                chain_id,
                response.status,
                response.message
            );
        }

        let contract_data = response
            .result
            .first()
            .context("No contract data returned from Etherscan")?;

        if contract_data.source_code.is_empty()
            || contract_data.source_code == "Contract source code not verified"
        {
            anyhow::bail!("Contract source code not verified on Etherscan");
        }

        if contract_data.abi.is_empty() || contract_data.abi == "Contract source code not verified"
        {
            anyhow::bail!("Contract ABI not available on Etherscan");
        }

        let abi: serde_json::Value =
            serde_json::from_str(&contract_data.abi).context("Failed to parse contract ABI")?;

        if !abi.is_array() || abi.as_array().is_none_or(|arr| arr.is_empty()) {
            anyhow::bail!("Contract ABI is empty or invalid");
        }

        // Parse proxy status - Etherscan returns "1" for proxy contracts
        let is_proxy = contract_data.proxy == "1";

        // Parse implementation address - only set if proxy and not empty
        let implementation_address = if is_proxy && !contract_data.implementation.is_empty() {
            Some(contract_data.implementation.to_lowercase())
        } else {
            None
        };

        Ok(Contract {
            address: address.to_lowercase(),
            chain: chain_id_to_name(chain_id),
            chain_id,
            source_code: contract_data.source_code.clone(),
            abi,
            description: None,
            name: Some(contract_data.contract_name.clone()),
            symbol: None,
            protocol: None,
            contract_type: None,
            version: None,
            is_proxy: Some(is_proxy),
            implementation_address,
            created_at: Some(chrono::Utc::now().timestamp()),
            updated_at: Some(chrono::Utc::now().timestamp()),
        })
    }

    pub async fn fetch_contract(&self, network: Network, address: &str) -> Result<Contract> {
        self.fetch_contract_by_chain_id(network.chain_id(), address)
            .await
    }

    pub async fn fetch_transaction_history_by_chain_id(
        &self,
        chain_id: u32,
        address: &str,
        sort: SortOrder,
    ) -> Result<Vec<Transaction>> {
        Self::validate_address(address)?;

        let params = self.build_params(
            chain_id,
            vec![
                ("module".to_string(), "account".to_string()),
                ("action".to_string(), "txlist".to_string()),
                ("address".to_string(), address.to_string()),
                ("startblock".to_string(), "0".to_string()),
                ("endblock".to_string(), "latest".to_string()),
                ("page".to_string(), "1".to_string()),
                ("offset".to_string(), "1000".to_string()),
                ("sort".to_string(), sort.as_str().to_string()),
            ],
        );

        let response: EtherscanResponse<Vec<Transaction>> = self.send_request(params).await?;

        if response.status != "1" {
            anyhow::bail!(
                "Etherscan API error for chain {}: status='{}', message='{}'",
                chain_id,
                response.status,
                response.message
            );
        }

        Ok(response.result)
    }

    pub async fn fetch_transaction_history(
        &self,
        network: Network,
        address: &str,
        sort: SortOrder,
    ) -> Result<Vec<Transaction>> {
        self.fetch_transaction_history_by_chain_id(network.chain_id(), address, sort)
            .await
    }

    pub async fn get_account_balance(&self, chain_id: u32, address: &str) -> Result<String> {
        Self::validate_address(address)?;

        let params = self.build_params(
            chain_id,
            vec![
                ("module".to_string(), "account".to_string()),
                ("action".to_string(), "balance".to_string()),
                ("address".to_string(), address.to_string()),
                ("tag".to_string(), "latest".to_string()),
            ],
        );

        let response: EtherscanResponse<String> = self.send_request(params).await?;

        if response.status != "1" {
            anyhow::bail!(
                "Etherscan API error for chain {}: status='{}', message='{}', result='{}'",
                chain_id,
                response.status,
                response.message,
                response.result
            );
        }

        Ok(response.result)
    }

    pub async fn get_erc20_balance(
        &self,
        chain_id: u32,
        contract_address: &str,
        holder_address: &str,
        tag: Option<&str>,
    ) -> Result<String> {
        Self::validate_address(contract_address)?;
        Self::validate_address(holder_address)?;

        let params = self.build_params(
            chain_id,
            vec![
                ("module".to_string(), "account".to_string()),
                ("action".to_string(), "tokenbalance".to_string()),
                ("contractaddress".to_string(), contract_address.to_string()),
                ("address".to_string(), holder_address.to_string()),
                (
                    "tag".to_string(),
                    tag.unwrap_or("latest").to_string().to_lowercase(),
                ),
            ],
        );

        let response: EtherscanResponse<String> = self.send_request(params).await?;

        if response.status != "1" {
            anyhow::bail!(
                "Etherscan API error for chain {}: status='{}', message='{}', result='{}'",
                chain_id,
                response.status,
                response.message,
                response.result
            );
        }

        Ok(response.result)
    }

    pub async fn get_transaction_count(&self, chain_id: u32, address: &str) -> Result<u64> {
        Self::validate_address(address)?;

        let params = self.build_params(
            chain_id,
            vec![
                ("module".to_string(), "proxy".to_string()),
                ("action".to_string(), "eth_getTransactionCount".to_string()),
                ("address".to_string(), address.to_string()),
                ("tag".to_string(), "latest".to_string()),
            ],
        );

        let response: JsonRpcResponse<String> = self.send_request(params).await?;
        let nonce_hex = response.result.trim_start_matches("0x");
        u64::from_str_radix(nonce_hex, 16).context("Failed to parse nonce from hex")
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        }
    }
}

// Shared Etherscan API response structure
#[derive(Debug, Deserialize)]
pub struct EtherscanResponse<T> {
    pub status: String,
    pub message: String,
    pub result: T,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[allow(dead_code)]
    id: Option<i32>,
    result: T,
}

// Contract-specific structures
#[derive(Debug, Deserialize)]
struct ContractSourceCode {
    #[serde(rename = "SourceCode")]
    pub source_code: String,
    #[serde(rename = "ABI")]
    pub abi: String,
    #[serde(rename = "ContractName")]
    pub contract_name: String,
    #[serde(rename = "Proxy", default)]
    pub proxy: String,
    #[serde(rename = "Implementation", default)]
    pub implementation: String,
}

// Account/Transaction structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    #[serde(rename = "blockNumber")]
    pub block_number: String,
    #[serde(rename = "timeStamp")]
    pub timestamp: String,
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: String,
    #[serde(rename = "gasPrice")]
    pub gas_price: String,
    #[serde(rename = "gasUsed")]
    pub gas_used: String,
    #[serde(rename = "isError")]
    pub is_error: String,
    pub input: String,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
}

// ============================================================================
// Convenience helpers that pull the API key from the environment
// ============================================================================

/// Fetches contract source code and ABI from Etherscan API and returns a Contract struct
/// API key is read from ETHERSCAN_API_KEY environment variable
pub async fn fetch_contract_from_etherscan(chainid: u32, address: String) -> Result<Contract> {
    external_clients()
        .await
        .etherscan_client()
        .context("ETHERSCAN_API_KEY environment variable not set")?
        .fetch_contract_by_chain_id(chainid, &address)
        .await
}

/// Fetches contract from Etherscan and saves it to the database
/// API key is read from ETHERSCAN_API_KEY environment variable
pub async fn fetch_and_store_contract(
    chainid: u32,
    address: String,
    store: &ContractStore,
) -> Result<Contract> {
    let mut contract = fetch_contract_from_etherscan(chainid, address.clone()).await?;

    // Try to enrich with ERC20 metadata (symbol, name) via RPC
    if contract.symbol.is_none() {
        let network_name = chain_id_to_name(chainid);

        match external_clients()
            .await
            .get_cast_client(&network_name)
            .await
        {
            Ok(cast_client) => {
                // Try to get symbol
                if let Some(symbol) = cast_client.get_symbol(&address).await {
                    contract.symbol = Some(symbol);
                }

                // Try to get name if we don't have one or only have the generic contract name
                let should_fetch_name = contract.name.is_none()
                    || contract
                        .name
                        .as_ref()
                        .map(|n| n == "Unknown")
                        .unwrap_or(false);

                if should_fetch_name && let Some(name) = cast_client.get_name(&address).await {
                    contract.name = Some(name);
                }
            }
            Err(e) => {
                warn!(
                    "Could not get RPC client for {} enrichment: {:?}",
                    network_name, e
                );
            }
        }
    }

    store
        .store_contract(contract.clone())
        .await
        .context("Failed to store contract in database")?;

    Ok(contract)
}

/// Fetches transaction history for an address from Etherscan API
/// API key is read from ETHERSCAN_API_KEY environment variable
///
/// Returns up to 1000 most recent transactions (Etherscan API limit per request)
pub async fn fetch_transaction_history(address: String, chainid: u32) -> Result<Vec<Transaction>> {
    external_clients()
        .await
        .etherscan_client()
        .context("ETHERSCAN_API_KEY environment variable not set")?
        .fetch_transaction_history_by_chain_id(chainid, &address, SortOrder::Desc)
        .await
}

fn default_latest_tag() -> String {
    "latest".to_string()
}

/// Parameters for fetching an ERC20 token balance via Etherscan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetErc20BalanceParameters {
    pub chain_id: u32,
    /// ERC20 contract address
    pub token_address: String,
    /// Address holding the tokens
    pub holder_address: String,
    /// Block tag to query, defaults to "latest"
    #[serde(default = "default_latest_tag")]
    pub tag: String,
}

impl AomiToolArgs for GetErc20BalanceParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "chain_id": {
                    "type": "number",
                    "description": "Numeric EVM chain ID (e.g., 1 for Ethereum mainnet, 137 for Polygon, 8453 for Base)"
                },
                "token_address": {
                    "type": "string",
                    "description": "ERC20 contract address"
                },
                "holder_address": {
                    "type": "string",
                    "description": "Address holding the tokens"
                },
                "tag": {
                    "type": "string",
                    "description": "Block tag to query, defaults to \"latest\""
                }
            },
            "required": ["chain_id", "token_address", "holder_address"]
        }))
    }
}

/// Result payload for ERC20 balance lookups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetErc20BalanceResult {
    pub chain_id: u32,
    pub token_address: String,
    pub holder_address: String,
    /// Balance in the token's smallest unit (raw on-chain value)
    pub balance: String,
    pub tag: String,
}

/// Tool entry point for ERC20 balance lookups.
#[derive(Debug, Clone)]
pub struct GetErc20Balance;

/// Fetch an ERC20 token balance for an address using the Etherscan API.
#[cfg(not(any(test, feature = "eval-test")))]
pub async fn execute_get_erc20_balance(
    args: GetErc20BalanceParameters,
) -> Result<GetErc20BalanceResult, ToolError> {
    run_sync(async move {
        let GetErc20BalanceParameters {
            chain_id,
            token_address,
            holder_address,
            tag,
        } = args;

        let client = external_clients().await.etherscan_client().ok_or_else(|| {
            ToolError::ToolCallError("ETHERSCAN_API_KEY environment variable not set".into())
        })?;

        let normalized_token_address = token_address.to_lowercase();
        let normalized_holder_address = holder_address.to_lowercase();
        let block_tag = tag.to_lowercase();

        info!(
            "Fetching ERC20 balance for token {} holder {} on chain {} (tag={})",
            normalized_token_address, normalized_holder_address, chain_id, block_tag
        );

        let balance = client
            .get_erc20_balance(
                chain_id,
                &normalized_token_address,
                &normalized_holder_address,
                Some(block_tag.as_str()),
            )
            .await
            .map_err(|e| {
                ToolError::ToolCallError(format!("Failed to fetch token balance: {}", e).into())
            })?;

        Ok(GetErc20BalanceResult {
            chain_id,
            token_address: normalized_token_address,
            holder_address: normalized_holder_address,
            balance,
            tag: block_tag,
        })
    })
}

/// Fetch an ERC20 token balance using cast/RPC for eval/test builds.
#[cfg(any(test, feature = "eval-test"))]
pub async fn execute_get_erc20_balance(
    args: GetErc20BalanceParameters,
) -> Result<GetErc20BalanceResult, ToolError> {
    run_sync(async move {
        let GetErc20BalanceParameters {
            chain_id,
            token_address,
            holder_address,
            tag,
        } = args;

        let clients = external_clients().await;
        let normalized_token_address = token_address.to_lowercase();
        let normalized_holder_address = holder_address.to_lowercase();
        let block_tag = tag.to_lowercase();

        let network_key = network_key_for_chain(chain_id).ok_or_else(|| {
            ToolError::ToolCallError(
                format!("Unsupported chain id {} for cast ERC20 balance", chain_id).into(),
            )
        })?;

        info!(
            "Fetching ERC20 balance for token {} holder {} on chain {} (tag={})",
            normalized_token_address, normalized_holder_address, chain_id, block_tag
        );

        let balance = erc20_balance_via_cast(
            clients.as_ref(),
            &network_key,
            chain_id,
            &normalized_token_address,
            &normalized_holder_address,
            &block_tag,
        )
        .await?;

        Ok(GetErc20BalanceResult {
            chain_id,
            token_address: normalized_token_address,
            holder_address: normalized_holder_address,
            balance,
            tag: block_tag,
        })
    })
}

#[cfg(any(test, feature = "eval-test"))]
fn network_key_for_chain(chain_id: u32) -> Option<String> {
    Network::try_from(chain_id)
        .ok()
        .map(|network| match network {
            Network::Mainnet => "ethereum".to_string(),
            other => other.canonical_name().to_string(),
        })
}

#[cfg(any(test, feature = "eval-test"))]
fn parse_block_tag(tag: &str) -> Result<Option<BlockId>, ToolError> {
    let normalized = tag.trim();
    if normalized.is_empty() || normalized.eq_ignore_ascii_case("latest") {
        return Ok(None);
    }

    if normalized.eq_ignore_ascii_case("earliest") {
        return Ok(Some(BlockId::Number(BlockNumberOrTag::Earliest)));
    }

    if normalized.eq_ignore_ascii_case("pending") {
        return Ok(Some(BlockId::Number(BlockNumberOrTag::Pending)));
    }

    if let Ok(num) = normalized.parse::<u64>() {
        return Ok(Some(BlockId::Number(BlockNumberOrTag::Number(num))));
    }

    if normalized.starts_with("0x") {
        let hash = normalized.parse::<BlockHash>().map_err(|e| {
            ToolError::ToolCallError(format!("Invalid block hash '{normalized}': {e}").into())
        })?;
        return Ok(Some(BlockId::Hash(RpcBlockHash::from_hash(hash, None))));
    }

    Err(ToolError::ToolCallError(
        format!("Invalid block tag '{tag}'").into(),
    ))
}

#[cfg(any(test, feature = "eval-test"))]
async fn erc20_balance_via_cast(
    clients: &ExternalClients,
    network_key: &str,
    chain_id: u32,
    token_address: &str,
    holder_address: &str,
    tag: &str,
) -> Result<String, ToolError> {
    let cast_client = clients.get_cast_client(network_key).await?;
    let contract_addr = Address::from_str(token_address).map_err(|e| {
        ToolError::ToolCallError(format!("Invalid token address '{token_address}': {e}").into())
    })?;
    let holder_addr = Address::from_str(holder_address).map_err(|e| {
        ToolError::ToolCallError(format!("Invalid holder address '{holder_address}': {e}").into())
    })?;

    let calldata = SimpleCast::calldata_encode("balanceOf(address)(uint256)", &[holder_address])
        .map_err(|e| {
            ToolError::ToolCallError(format!("Failed to encode balanceOf calldata: {e}").into())
        })?;

    let calldata_bytes = calldata.parse::<Bytes>().map_err(|e| {
        ToolError::ToolCallError(format!("Failed to parse calldata bytes: {}", e).into())
    })?;

    let tx = TransactionRequest::default()
        .to(contract_addr)
        .from(holder_addr)
        .input(TransactionInput::new(calldata_bytes))
        .with_input_and_data();

    let block_id = parse_block_tag(tag)?;
    let raw_result = cast_client
        .cast
        .call(&tx.into(), None, block_id, None, None)
        .await
        .map_err(|e| {
            ToolError::ToolCallError(
                format!(
                    "Failed to fetch token balance via {} (chain {}): {}",
                    network_key, chain_id, e
                )
                .into(),
            )
        })?;

    let bytes = hex::decode(raw_result.trim_start_matches("0x")).map_err(|e| {
        ToolError::ToolCallError(format!("Failed to decode balance bytes: {}", e).into())
    })?;

    let decoded = DynSolType::Uint(256).abi_decode(&bytes).map_err(|e| {
        ToolError::ToolCallError(format!("Failed to decode balanceOf result: {e}").into())
    })?;

    let balance = match decoded {
        DynSolValue::Uint(value, _) => value.to_string(),
        other => {
            return Err(ToolError::ToolCallError(
                format!("Unexpected balanceOf return type: {:?}", other).into(),
            ));
        }
    };

    info!(
        "Fetched ERC20 balance via {} RPC for chain {}: token={} holder={} balance={} tag={}",
        network_key, chain_id, token_address, holder_address, balance, tag
    );

    Ok(balance)
}

/// Parameters for fetching and storing a contract via Etherscan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchContractFromEtherscanParameters {
    pub chain_id: u32,
    pub address: String,
}

impl AomiToolArgs for FetchContractFromEtherscanParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "chain_id": {
                    "type": "number",
                    "description": "Numeric EVM chain ID (e.g., 1 for Ethereum mainnet, 137 for Polygon, 8453 for Base)"
                },
                "address": {
                    "type": "string",
                    "description": "Target contract address (42-character hex with 0x prefix)"
                }
            },
            "required": ["chain_id", "address"]
        }))
    }
}

/// Result payload returned after a contract is fetched and stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchContractFromEtherscanResult {
    pub address: String,
    pub chain: String,
    pub chain_id: u32,
    pub abi: serde_json::Value,
    pub source_code: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub is_proxy: Option<bool>,
    pub implementation_address: Option<String>,
    pub stored: bool,
}

#[derive(Debug, Clone)]
pub struct GetContractFromEtherscan;

/// Fetch a contract via the Etherscan API and persist it in the ContractStore.
pub async fn execute_fetch_contract_from_etherscan(
    args: FetchContractFromEtherscanParameters,
) -> Result<FetchContractFromEtherscanResult, ToolError> {
    run_sync(async move {
        let FetchContractFromEtherscanParameters { chain_id, address } = args;

        sqlx::any::install_default_drivers();
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://aomi@localhost:5432/chatbot".to_string());

        debug!(
            "Connecting to database {} before fetching contract {} on chain {}",
            database_url, address, chain_id
        );

        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .map_err(|e| {
                ToolError::ToolCallError(format!("Database connection error: {}", e).into())
            })?;

        let store = ContractStore::new(pool);
        let normalized_address = address.to_lowercase();

        info!(
            "Fetching contract {} on chain {} from Etherscan",
            normalized_address, chain_id
        );

        let contract = fetch_and_store_contract(chain_id, normalized_address.clone(), &store)
            .await
            .map_err(|e| {
                ToolError::ToolCallError(format!("Failed to fetch from Etherscan: {}", e).into())
            })?;

        Ok(FetchContractFromEtherscanResult {
            address: contract.address,
            chain: contract.chain,
            chain_id: contract.chain_id,
            abi: contract.abi,
            source_code: contract.source_code,
            name: contract.name,
            symbol: contract.symbol,
            is_proxy: contract.is_proxy,
            implementation_address: contract.implementation_address,
            stored: true,
        })
    })
}

impl AomiTool for GetErc20Balance {
    const NAME: &'static str = "get_erc20_balance";

    type Args = GetErc20BalanceParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Fetch ERC20 token balance via Etherscan."
    }

    fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<serde_json::Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let result = execute_get_erc20_balance(args)
                .await
                .and_then(|value| {
                    serde_json::to_value(value).map_err(|e| ToolError::ToolCallError(e.into()))
                })
                .map_err(|e| eyre::eyre!(e.to_string()));
            let _ = sender.send(result);
        }
    }
}

impl AomiTool for GetContractFromEtherscan {
    const NAME: &'static str = "get_contract_from_etherscan";

    type Args = FetchContractFromEtherscanParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Fetch and store a contract from Etherscan."
    }

    fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<serde_json::Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let result = execute_fetch_contract_from_etherscan(args)
                .await
                .and_then(|value| {
                    serde_json::to_value(value).map_err(|e| ToolError::ToolCallError(e.into()))
                })
                .map_err(|e| eyre::eyre!(e.to_string()));
            let _ = sender.send(result);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn skip_without_etherscan_api_key() -> bool {
        std::env::var("ETHERSCAN_API_KEY").is_err()
    }

    // Contract tests
    #[tokio::test]
    async fn test_fetch_usdc_from_etherscan() -> Result<()> {
        if skip_without_etherscan_api_key() {
            eprintln!("Skipping: ETHERSCAN_API_KEY not set");
            return Ok(());
        }
        let contract = fetch_contract_from_etherscan(
            ETHEREUM_MAINNET,
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
        )
        .await
        .unwrap();

        assert_eq!(
            contract.address,
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
        assert_eq!(contract.chain, "ethereum");
        assert!(!contract.source_code.is_empty());
        assert!(contract.abi.is_array());

        println!("Fetched contract: {} bytes", contract.source_code.len());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "Needs etherscan API key and database URL"]
    async fn test_fetch_and_store_usdc() -> Result<()> {
        use sqlx::any::AnyPoolOptions;

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://aomi@localhost:5432/chatbot".to_string());

        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let store = ContractStore::new(pool);

        let contract = fetch_and_store_contract(
            ETHEREUM_MAINNET,
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            &store,
        )
        .await?;

        let retrieved = store
            .get_contract(ETHEREUM_MAINNET, contract.address.clone())
            .await?;

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.address, contract.address);
        assert!(!retrieved.source_code.is_empty());

        println!("Successfully stored and retrieved contract");
        Ok(())
    }

    // Account tests
    #[tokio::test]
    async fn test_fetch_transaction_history() -> Result<()> {
        if skip_without_etherscan_api_key() {
            eprintln!("Skipping: ETHERSCAN_API_KEY not set");
            return Ok(());
        }
        let transactions = fetch_transaction_history(
            "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            ARBITRUM,
        )
        .await?;

        assert!(!transactions.is_empty());

        let first_tx = &transactions[0];
        assert!(!first_tx.hash.is_empty());
        assert!(!first_tx.from.is_empty());
        assert!(!first_tx.block_number.is_empty());

        println!("Fetched {} transactions", transactions.len());
        Ok(())
    }
}
