use alloy::{
    eips::{BlockId, BlockNumberOrTag, RpcBlockHash},
    network::AnyNetwork,
    primitives::{Address, B256, BlockHash, Bytes, U256},
    rpc::types::{TransactionInput, TransactionRequest},
};
use alloy_ens::NameOrAddress;
use alloy_provider::{DynProvider, Provider, ProviderBuilder};
use cast::Cast;
use once_cell::sync::Lazy;
// use rig_derive::rig_tool; // removed, explicit Tool impls instead
use std::{
    collections::{HashMap, hash_map::Entry},
    future::Future,
    str::FromStr,
    sync::{Arc, RwLock},
};
// use crate::impl_rig_tool_clone; // removed, explicit Tool impls instead
use tokio::task;
use tracing::{debug, info, warn};

const DEFAULT_RPC_URL: &str = "http://127.0.0.1:8545";

fn tool_error(message: impl Into<String>) -> rig::tool::ToolError {
    rig::tool::ToolError::ToolCallError(message.into().into())
}

fn network_label(network: &Option<String>) -> String {
    network.as_deref().unwrap_or("testnet").to_string()
}

fn parse_block_identifier(input: Option<String>) -> Result<Option<BlockId>, rig::tool::ToolError> {
    match input {
        None => Ok(None),
        Some(ref value) if value.eq_ignore_ascii_case("latest") => {
            Ok(Some(BlockId::Number(BlockNumberOrTag::Latest)))
        }
        Some(ref value) if value.eq_ignore_ascii_case("earliest") => {
            Ok(Some(BlockId::Number(BlockNumberOrTag::Earliest)))
        }
        Some(ref value) if value.eq_ignore_ascii_case("pending") => {
            Ok(Some(BlockId::Number(BlockNumberOrTag::Pending)))
        }
        Some(ref value) => {
            if let Ok(num) = value.parse::<u64>() {
                Ok(Some(BlockId::Number(BlockNumberOrTag::Number(num))))
            } else if value.starts_with("0x") {
                let hash = value
                    .parse::<BlockHash>()
                    .map_err(|e| tool_error(format!("Invalid block hash '{value}': {e}")))?;
                Ok(Some(BlockId::Hash(RpcBlockHash::from_hash(hash, None))))
            } else {
                Err(tool_error(format!(
                    "Invalid block identifier '{value}'. Use a number, a 0x-prefixed hash, or 'latest'"
                )))
            }
        }
    }
}

fn parse_name_or_address(value: &str) -> Result<NameOrAddress, rig::tool::ToolError> {
    if value.starts_with("0x") {
        let address = value
            .parse::<Address>()
            .map_err(|e| tool_error(format!("Invalid address '{value}': {e}")))?;
        Ok(NameOrAddress::Address(address))
    } else {
        Ok(NameOrAddress::Name(value.to_string()))
    }
}

fn parse_u256(value: &str) -> Result<U256, rig::tool::ToolError> {
    U256::from_str(value).map_err(|e| tool_error(format!("Invalid numeric value '{value}': {e}")))
}

fn parse_bytes(value: &str) -> Result<Bytes, rig::tool::ToolError> {
    if value.trim().is_empty() {
        return Ok(Bytes::default());
    }

    value
        .parse::<Bytes>()
        .map_err(|_| tool_error("Calldata must be a 0x-prefixed hex string"))
}

fn network_urls() -> &'static HashMap<String, String> {
    static NETWORKS: Lazy<HashMap<String, String>> = Lazy::new(|| {
        let mut defaults = HashMap::new();
        defaults.insert("testnet".to_string(), DEFAULT_RPC_URL.to_string());

        match std::env::var("MCP_NETWORK_URLS_JSON") {
            Ok(json) => match serde_json::from_str::<HashMap<String, String>>(&json) {
                Ok(mut parsed) => {
                    if !parsed.contains_key("testnet") {
                        parsed.insert("testnet".to_string(), DEFAULT_RPC_URL.to_string());
                    }
                    parsed
                }
                Err(err) => {
                    warn!(
                        "Failed to parse MCP_NETWORK_URLS_JSON ({}). Falling back to defaults.",
                        err
                    );
                    defaults
                }
            },
            Err(_) => {
                warn!("No MCP_NETWORK_URLS_JSON found. Falling back to defaults.");
                defaults
            }
        }
    });

    &NETWORKS
}

fn client_singletons() -> &'static RwLock<HashMap<String, Arc<CastClient>>> {
    static CLIENT_SINGLETONS: Lazy<RwLock<HashMap<String, Arc<CastClient>>>> =
        Lazy::new(|| RwLock::new(HashMap::new()));
    &CLIENT_SINGLETONS
}

fn run_async<F, T>(future: F) -> Result<T, rig::tool::ToolError>
where
    F: Future<Output = Result<T, rig::tool::ToolError>> + Send + 'static,
    T: Send + 'static,
{
    task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

struct CastClient {
    provider: DynProvider<AnyNetwork>,
    cast: Cast<DynProvider<AnyNetwork>>,
    rpc_url: String,
}

impl CastClient {
    async fn connect(rpc_url: &str) -> Result<Self, rig::tool::ToolError> {
        info!(
            target: "aomi_tools::cast",
            rpc = %rpc_url,
            "Initializing Cast client connection"
        );
        let provider = ProviderBuilder::<_, _, AnyNetwork>::default()
            .connect(rpc_url)
            .await
            .map_err(|e| tool_error(format!("Failed to connect to RPC {rpc_url}: {e}")))?;

        let provider_dyn = DynProvider::new(provider.clone());
        let cast = Cast::new(DynProvider::new(provider));

        info!(
            target: "aomi_tools::cast",
            rpc = %rpc_url,
            "Cast client ready"
        );

        Ok(Self {
            provider: provider_dyn,
            cast,
            rpc_url: rpc_url.to_string(),
        })
    }

    async fn resolve_address(&self, value: &str) -> Result<Address, rig::tool::ToolError> {
        let parsed = parse_name_or_address(value)?;
        parsed.resolve(&self.provider).await.map_err(|e| {
            tool_error(format!(
                "Failed to resolve '{value}' via {}: {e}",
                self.rpc_url
            ))
        })
    }

    async fn balance(
        &self,
        address: String,
        block: Option<String>,
    ) -> Result<String, rig::tool::ToolError> {
        let account = self.resolve_address(&address).await?;
        let block_id = parse_block_identifier(block)?;
        let balance = self
            .cast
            .balance(account, block_id)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch balance: {e}")))?;
        Ok(balance.to_string())
    }

    async fn eth_call(
        &self,
        from: String,
        to: String,
        value: String,
        input: Option<String>,
    ) -> Result<String, rig::tool::ToolError> {
        let from_addr = self.resolve_address(&from).await?;
        let to_addr = self.resolve_address(&to).await?;
        let value = parse_u256(&value)?;

        let mut tx = TransactionRequest::default()
            .to(to_addr)
            .from(from_addr)
            .value(value)
            .gas_limit(10_000_000u64);

        if let Some(ref calldata) = input {
            let bytes = parse_bytes(calldata)?;
            let tx_input = TransactionInput::new(bytes);
            tx = tx.input(tx_input).with_input_and_data();
        }

        self.cast
            .call(&tx.into(), None, None, None, None)
            .await
            .map_err(|e| tool_error(format!("eth_call execution failed: {e}")))
    }

    async fn send_transaction(
        &self,
        from: String,
        to: String,
        value: String,
        input: Option<String>,
    ) -> Result<String, rig::tool::ToolError> {
        let from_addr = self.resolve_address(&from).await?;
        let to_addr = self.resolve_address(&to).await?;
        let value = parse_u256(&value)?;

        let mut tx = TransactionRequest::default()
            .to(to_addr)
            .from(from_addr)
            .value(value)
            .gas_limit(10_000_000u64);

        if let Some(ref calldata) = input {
            let bytes = parse_bytes(calldata)?;
            let tx_input = TransactionInput::new(bytes);
            tx = tx.input(tx_input).with_input_and_data();
        }

        info!(
            "Submitting transaction via {} from {} to {} with value {}",
            self.rpc_url, from_addr, to_addr, value
        );

        let result = self
            .cast
            .send(tx.into())
            .await
            .map_err(|e| tool_error(format!("Transaction submission failed: {e}")))?;

        Ok(result.tx_hash().to_string())
    }

    async fn contract_code(&self, address: String) -> Result<String, rig::tool::ToolError> {
        let addr = self.resolve_address(&address).await?;
        self.cast
            .code(addr, None, false)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch contract code: {e}")))
    }

    async fn contract_code_size(&self, address: String) -> Result<String, rig::tool::ToolError> {
        let addr = self.resolve_address(&address).await?;
        let size = self
            .cast
            .codesize(addr, None)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch contract code size: {e}")))?;
        Ok(size.to_string())
    }

    async fn transaction_details(
        &self,
        tx_hash: String,
        field: Option<String>,
    ) -> Result<String, rig::tool::ToolError> {
        let hash = tx_hash
            .parse::<B256>()
            .map_err(|e| tool_error(format!("Invalid transaction hash '{tx_hash}': {e}")))?;

        let tx = self
            .provider
            .get_transaction_by_hash(hash)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch transaction: {e}")))?
            .ok_or_else(|| tool_error("Transaction not found"))?;

        let receipt = self
            .provider
            .get_transaction_receipt(hash)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch transaction receipt: {e}")))?;

        let tx_json = serde_json::to_value(&tx)
            .map_err(|e| tool_error(format!("Failed to serialize transaction: {e}")))?;
        let receipt_json = receipt.as_ref().and_then(|r| serde_json::to_value(r).ok());

        if let Some(field) = field {
            if let Some(value) = tx_json.get(&field) {
                return Ok(value.to_string());
            }
            if let Some(receipt) = &receipt_json
                && let Some(value) = receipt.get(&field)
            {
                return Ok(value.to_string());
            }
            return Err(tool_error(format!(
                "Field '{field}' not found in transaction or receipt"
            )));
        }

        let mut output = format!("Transaction {tx_hash}:\n\nTransaction Data:\n");
        output.push_str(
            &serde_json::to_string_pretty(&tx_json).unwrap_or_else(|_| tx_json.to_string()),
        );

        if let Some(receipt) = receipt_json {
            output.push_str("\n\nReceipt Data:\n");
            output.push_str(
                &serde_json::to_string_pretty(&receipt).unwrap_or_else(|_| receipt.to_string()),
            );
        }

        Ok(output)
    }

    async fn block_details(
        &self,
        block: Option<String>,
        field: Option<String>,
    ) -> Result<String, rig::tool::ToolError> {
        let block_id =
            parse_block_identifier(block)?.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let block = self
            .provider
            .get_block(block_id)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch block: {e}")))?
            .ok_or_else(|| tool_error("Block not found"))?;

        let block_json = serde_json::to_value(&block)
            .map_err(|e| tool_error(format!("Failed to serialize block: {e}")))?;

        if let Some(field) = field {
            if let Some(value) = block_json.get(&field) {
                return Ok(value.to_string());
            }
            return Err(tool_error(format!(
                "Field '{field}' not found in block data"
            )));
        }

        serde_json::to_string_pretty(&block_json)
            .or_else(|_| serde_json::to_string(&block_json))
            .map_err(|e| tool_error(format!("Failed to format block JSON: {e}")))
    }
}

async fn get_client(network: Option<String>) -> Result<Arc<CastClient>, rig::tool::ToolError> {
    let network_key = network.unwrap_or_else(|| "testnet".to_string());
    let networks = network_urls();
    let rpc_url = networks.get(&network_key).ok_or_else(|| {
        tool_error(format!(
            "Unsupported network '{network_key}'. Configure MCP_NETWORK_URLS_JSON to include it."
        ))
    })?;

    debug!(
        target: "aomi_tools::cast",
        network = %network_key,
        rpc = %rpc_url,
        "Retrieving Cast client"
    );

    {
        let singletons_read = client_singletons().read().unwrap();
        if let Some(client) = singletons_read.get(&network_key) {
            debug!(
                target: "aomi_tools::cast",
                network = %network_key,
                "Using cached Cast client"
            );
            return Ok(client.clone());
        }
    }

    let client = Arc::new(CastClient::connect(rpc_url).await?);

    info!(
        target: "aomi_tools::cast",
        network = %network_key,
        rpc = %rpc_url,
        "Caching new Cast client"
    );

    let mut singletons_write = client_singletons().write().unwrap();
    match singletons_write.entry(network_key) {
        Entry::Occupied(entry) => Ok(entry.get().clone()),
        Entry::Vacant(entry) => {
            entry.insert(client.clone());
            Ok(client)
        }
    }
}

use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAccountBalanceParameters {
    pub address: String,
    pub block: Option<String>,
    pub network: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GetAccountBalance;

impl Tool for GetAccountBalance {
    const NAME: &'static str = "get_account_balance";
    type Args = GetAccountBalanceParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Get the balance of an account (address or ENS) in wei on the specified network."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "address": {"type": "string", "description": "Account address or ENS name to query"},
                    "block": {"type": "string", "description": "Optional block number/hash tag (e.g., 'latest', '12345', or block hash)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["address"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let network_name = network_label(&args.network);
        let block_label = args.block.as_deref().unwrap_or("latest").to_string();
        let address_for_log = args.address.clone();

        info!(
            target: "aomi_tools::cast",
            tool = "get_account_balance",
            address = %address_for_log,
            block = %block_label,
            network = %network_name,
            "Invoking Cast tool"
        );

        
        run_async(async move {
            let client = get_client(args.network).await?;
            let result = client.balance(args.address, args.block).await;
            match &result {
                Ok(balance) => info!(
                    target: "aomi_tools::cast",
                    tool = "get_account_balance",
                    address = %address_for_log,
                    block = %block_label,
                    network = %network_name,
                    balance = %balance,
                    "Cast tool succeeded"
                ),
                Err(err) => warn!(
                    target: "aomi_tools::cast",
                    tool = "get_account_balance",
                    address = %address_for_log,
                    block = %block_label,
                    network = %network_name,
                    error = %err,
                    "Cast tool failed"
                ),
            }
            result
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallViewFunctionParameters {
    pub from: String,
    pub to: String,
    pub value: String,
    pub input: Option<String>,
    pub network: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CallViewFunction;

impl Tool for CallViewFunction {
    const NAME: &'static str = "call_view_function";
    type Args = CallViewFunctionParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Call a view function against a contract with optional calldata."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "from": {"type": "string", "description": "Sender address or ENS name"},
                    "to": {"type": "string", "description": "Target contract or account address/ENS"},
                    "value": {"type": "string", "description": "Amount of ETH to send in wei (as decimal string)"},
                    "input": {"type": "string", "description": "Optional calldata (0x-prefixed hex)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["from", "to", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let network_name = network_label(&args.network);
        let from_log = args.from.clone();
        let to_log = args.to.clone();
        let input_len = args.input.as_ref().map(|s| s.len()).unwrap_or(0);
        let input_preview = args
            .input
            .as_ref()
            .map(|s| {
                if s.len() > 66 {
                    format!("{}…", &s[..66])
                } else {
                    s.clone()
                }
            })
            .unwrap_or_else(|| "None".to_string());

        info!(
            target: "aomi_tools::cast",
            tool = "call_view_function",
            from = %from_log,
            to = %to_log,
            value_wei = %args.value,
            input_len,
            network = %network_name,
            preview = %input_preview,
            "Invoking Cast tool"
        );

        
        run_async(async move {
            let client = get_client(args.network).await?;
            let result = client
                .eth_call(args.from, args.to, args.value, args.input)
                .await;
            match &result {
                Ok(_) => info!(
                    target: "aomi_tools::cast",
                    tool = "call_view_function",
                    from = %from_log,
                    to = %to_log,
                    network = %network_name,
                    "Cast tool succeeded"
                ),
                Err(err) => warn!(
                    target: "aomi_tools::cast",
                    tool = "call_view_function",
                    from = %from_log,
                    to = %to_log,
                    network = %network_name,
                    error = %err,
                    "Cast tool failed"
                ),
            }
            result
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateContractCallParameters {
    pub from: String,
    pub to: String,
    pub value: String,
    pub input: Option<String>,
    pub network: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SimulateContractCall;

impl Tool for SimulateContractCall {
    const NAME: &'static str = "simulate_contract_call";
    type Args = SimulateContractCallParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Simulate a non-view function call against a contract with optional calldata."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "from": {"type": "string", "description": "Sender address or ENS name"},
                    "to": {"type": "string", "description": "Target contract or account address/ENS"},
                    "value": {"type": "string", "description": "Amount of ETH to send in wei (as decimal string)"},
                    "input": {"type": "string", "description": "Optional calldata (0x-prefixed hex)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["from", "to", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let network_name = network_label(&args.network);
        let from_log = args.from.clone();
        let to_log = args.to.clone();
        let input_len = args.input.as_ref().map(|s| s.len()).unwrap_or(0);
        let input_preview = args
            .input
            .as_ref()
            .map(|s| {
                if s.len() > 66 {
                    format!("{}…", &s[..66])
                } else {
                    s.clone()
                }
            })
            .unwrap_or_else(|| "None".to_string());

        info!(
            target: "aomi_tools::cast",
            tool = "simulate_contract_call",
            from = %from_log,
            to = %to_log,
            value_wei = %args.value,
            input_len,
            network = %network_name,
            preview = %input_preview,
            "Invoking Cast tool"
        );

        
        run_async(async move {
            let client = get_client(args.network).await?;
            let result = client
                .eth_call(args.from, args.to, args.value, args.input)
                .await;
            match &result {
                Ok(_) => info!(
                    target: "aomi_tools::cast",
                    tool = "simulate_contract_call",
                    from = %from_log,
                    to = %to_log,
                    network = %network_name,
                    "Cast tool succeeded"
                ),
                Err(err) => warn!(
                    target: "aomi_tools::cast",
                    tool = "simulate_contract_call",
                    from = %from_log,
                    to = %to_log,
                    network = %network_name,
                    error = %err,
                    "Cast tool failed"
                ),
            }
            result
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTransactionParameters {
    pub from: String,
    pub to: String,
    pub value: String,
    pub input: Option<String>,
    pub network: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SendTransaction;

impl Tool for SendTransaction {
    const NAME: &'static str = "send_transaction";
    type Args = SendTransactionParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Broadcast a transaction using the connected RPC (intended for testnets)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "from": {"type": "string", "description": "Sender address or ENS name (must have signing capability on the RPC)"},
                    "to": {"type": "string", "description": "Recipient address or ENS name"},
                    "value": {"type": "string", "description": "Amount of ETH to send in wei (as decimal string)"},
                    "input": {"type": "string", "description": "Optional calldata (0x-prefixed hex)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["from", "to", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let network_name = network_label(&args.network);
        let from_log = args.from.clone();
        let to_log = args.to.clone();
        let input_len = args.input.as_ref().map(|s| s.len()).unwrap_or(0);

        info!(
            target: "aomi_tools::cast",
            tool = "send_transaction",
            from = %from_log,
            to = %to_log,
            value_wei = %args.value,
            input_len,
            network = %network_name,
            "Invoking Cast tool"
        );

        
        run_async(async move {
            let client = get_client(args.network).await?;
            let result = client
                .send_transaction(args.from, args.to, args.value, args.input)
                .await;
            match &result {
                Ok(tx_hash) => info!(
                    target: "aomi_tools::cast",
                    tool = "send_transaction",
                    from = %from_log,
                    to = %to_log,
                    network = %network_name,
                    tx_hash = %tx_hash,
                    "Cast tool succeeded"
                ),
                Err(err) => warn!(
                    target: "aomi_tools::cast",
                    tool = "send_transaction",
                    from = %from_log,
                    to = %to_log,
                    network = %network_name,
                    error = %err,
                    "Cast tool failed"
                ),
            }
            result
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContractCodeParameters {
    pub address: String,
    pub network: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GetContractCode;

impl Tool for GetContractCode {
    const NAME: &'static str = "get_contract_code";
    type Args = GetContractCodeParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetch the runtime bytecode for a deployed contract.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "address": {"type": "string", "description": "Contract address (or ENS name resolving to contract)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["address"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let network_name = network_label(&args.network);
        let address_for_log = args.address.clone();

        info!(
            target: "aomi_tools::cast",
            tool = "get_contract_code",
            address = %address_for_log,
            network = %network_name,
            "Invoking Cast tool"
        );

        
        run_async(async move {
            let client = get_client(args.network).await?;
            let result = client.contract_code(args.address).await;
            match &result {
                Ok(code) => info!(
                    target: "aomi_tools::cast",
                    tool = "get_contract_code",
                    address = %address_for_log,
                    network = %network_name,
                    byte_length = code.len(),
                    "Cast tool succeeded"
                ),
                Err(err) => warn!(
                    target: "aomi_tools::cast",
                    tool = "get_contract_code",
                    address = %address_for_log,
                    network = %network_name,
                    error = %err,
                    "Cast tool failed"
                ),
            }
            result
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContractCodeSizeParameters {
    pub address: String,
    pub network: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GetContractCodeSize;

impl Tool for GetContractCodeSize {
    const NAME: &'static str = "get_contract_code_size";
    type Args = GetContractCodeSizeParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Return the runtime bytecode size (bytes) for a contract.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "address": {"type": "string", "description": "Contract address or ENS name"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["address"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let network_name = network_label(&args.network);
        let address_for_log = args.address.clone();

        info!(
            target: "aomi_tools::cast",
            tool = "get_contract_code_size",
            address = %address_for_log,
            network = %network_name,
            "Invoking Cast tool"
        );

        
        run_async(async move {
            let client = get_client(args.network).await?;
            let result = client.contract_code_size(args.address).await;
            match &result {
                Ok(size) => info!(
                    target: "aomi_tools::cast",
                    tool = "get_contract_code_size",
                    address = %address_for_log,
                    network = %network_name,
                    size_bytes = %size,
                    "Cast tool succeeded"
                ),
                Err(err) => warn!(
                    target: "aomi_tools::cast",
                    tool = "get_contract_code_size",
                    address = %address_for_log,
                    network = %network_name,
                    error = %err,
                    "Cast tool failed"
                ),
            }
            result
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTransactionDetailsParameters {
    pub tx_hash: String,
    pub field: Option<String>,
    pub network: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GetTransactionDetails;

impl Tool for GetTransactionDetails {
    const NAME: &'static str = "get_transaction_details";
    type Args = GetTransactionDetailsParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Retrieve transaction (and optional receipt) data by hash.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tx_hash": {"type": "string", "description": "Transaction hash (0x-prefixed)"},
                    "field": {"type": "string", "description": "Optional specific field to extract from the transaction/receipt JSON"},
                    "network": {"type": "string", "description": "Optional network key defined in MCP_NETWORK_URLS_JSON (defaults to 'testnet')"}
                },
                "required": ["tx_hash"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let network_name = network_label(&args.network);
        let tx_for_log = args.tx_hash.clone();
        let field_label = args.field.clone().unwrap_or_else(|| "all".to_string());

        info!(
            target: "aomi_tools::cast",
            tool = "get_transaction_details",
            tx_hash = %tx_for_log,
            field = %field_label,
            network = %network_name,
            "Invoking Cast tool"
        );

        
        run_async(async move {
            let client = get_client(args.network).await?;
            let result = client.transaction_details(args.tx_hash, args.field).await;
            match &result {
                Ok(_) => info!(
                    target: "aomi_tools::cast",
                    tool = "get_transaction_details",
                    tx_hash = %tx_for_log,
                    network = %network_name,
                    "Cast tool succeeded"
                ),
                Err(err) => warn!(
                    target: "aomi_tools::cast",
                    tool = "get_transaction_details",
                    tx_hash = %tx_for_log,
                    network = %network_name,
                    error = %err,
                    "Cast tool failed"
                ),
            }
            result
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBlockDetailsParameters {
    pub block: Option<String>,
    pub field: Option<String>,
    pub network: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GetBlockDetails;

impl Tool for GetBlockDetails {
    const NAME: &'static str = "get_block_details";
    type Args = GetBlockDetailsParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Inspect a block by number/hash or fetch the latest block if not specified."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "block": {"type": "string", "description": "Optional block identifier ('latest', number, or hash). Defaults to latest."},
                    "field": {"type": "string", "description": "Optional field to pull from the block JSON (e.g., 'timestamp', 'miner')"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": []
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let network_name = network_label(&args.network);
        let block_label = args.block.as_deref().unwrap_or("latest").to_string();
        let field_label = args.field.clone().unwrap_or_else(|| "all".to_string());

        info!(
            target: "aomi_tools::cast",
            tool = "get_block_details",
            block = %block_label,
            field = %field_label,
            network = %network_name,
            "Invoking Cast tool"
        );

        
        run_async(async move {
            let client = get_client(args.network).await?;
            let result = client.block_details(args.block, args.field).await;
            match &result {
                Ok(_) => info!(
                    target: "aomi_tools::cast",
                    tool = "get_block_details",
                    block = %block_label,
                    network = %network_name,
                    "Cast tool succeeded"
                ),
                Err(err) => warn!(
                    target: "aomi_tools::cast",
                    tool = "get_block_details",
                    block = %block_label,
                    network = %network_name,
                    error = %err,
                    "Cast tool failed"
                ),
            }
            result
        })
    }
}
