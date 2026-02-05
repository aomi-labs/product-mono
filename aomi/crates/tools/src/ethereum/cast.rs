use crate::clients::CastClient;
use alloy::{
    eips::{BlockId, BlockNumberOrTag, RpcBlockHash},
    primitives::{Address, B256, BlockHash, Bytes, U256},
    rpc::types::{TransactionInput, TransactionRequest},
};
use alloy_ens::NameOrAddress;
use alloy_provider::Provider;
use async_trait::async_trait;
use serde_json::json;
use std::{future::Future, str::FromStr, sync::Arc};
// use crate::impl_rig_tool_clone; // removed, explicit Tool impls instead
use crate::{AomiTool, AomiToolArgs, ToolCallCtx, with_topic};
use tokio::task;
use tracing::{debug, info, warn};

pub(crate) fn tool_error(message: impl Into<String>) -> rig::tool::ToolError {
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
                    .map_err(|e| tool_error(format!("Invalid block hash '{value}': {e:#?}")))?;
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
            .map_err(|e| tool_error(format!("Invalid address '{value}': {e:#?}")))?;
        Ok(NameOrAddress::Address(address))
    } else {
        Ok(NameOrAddress::Name(value.to_string()))
    }
}

fn parse_u256(value: &str) -> Result<U256, rig::tool::ToolError> {
    U256::from_str(value)
        .map_err(|e| tool_error(format!("Invalid numeric value '{value}': {e:#?}")))
}

fn parse_bytes(value: &str) -> Result<Bytes, rig::tool::ToolError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(Bytes::default());
    }

    let normalized = if trimmed.starts_with("0x") {
        trimmed.to_string()
    } else if trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        format!("0x{trimmed}")
    } else {
        trimmed.to_string()
    };

    normalized
        .parse::<Bytes>()
        .map_err(|_| tool_error("Calldata must be a 0x-prefixed hex string"))
}

fn run_async<F, T>(future: F) -> Result<T, rig::tool::ToolError>
where
    F: Future<Output = Result<T, rig::tool::ToolError>> + Send + 'static,
    T: Send + 'static,
{
    task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

/// Trait for ERC20 token metadata operations
#[async_trait]
pub trait ERC20 {
    /// Fetch the token symbol by calling symbol() on the contract
    async fn get_symbol(&self, address: &str) -> Option<String>;

    /// Fetch the token name by calling name() on the contract
    async fn get_name(&self, address: &str) -> Option<String>;
}

#[async_trait]
impl ERC20 for CastClient {
    async fn get_symbol(&self, address: &str) -> Option<String> {
        use alloy::dyn_abi::{DynSolType, DynSolValue};
        use cast::SimpleCast;

        // Encode symbol() function call
        let calldata = SimpleCast::calldata_encode("symbol()(string)", &[] as &[&str]).ok()?;
        let calldata_bytes = calldata.parse::<Bytes>().ok()?;
        let contract_addr = address.parse::<Address>().ok()?;

        // Build transaction request
        let tx = TransactionRequest::default()
            .to(contract_addr)
            .input(TransactionInput::new(calldata_bytes))
            .with_input_and_data();

        // Make the call using cast.call
        let result = self
            .cast
            .call(&tx.into(), None, None, None, None)
            .await
            .ok()?;

        // Decode the result using alloy's ABI decoder
        let bytes = result
            .strip_prefix("0x")
            .and_then(|hex_str| hex::decode(hex_str).ok())?;

        let string_type = DynSolType::String;
        match string_type.abi_decode(&bytes) {
            Ok(DynSolValue::String(s)) => {
                info!("Enriched contract {} with symbol: {}", address, s);
                Some(s)
            }
            _ => None,
        }
    }

    async fn get_name(&self, address: &str) -> Option<String> {
        use alloy::dyn_abi::{DynSolType, DynSolValue};
        use cast::SimpleCast;

        // Encode name() function call
        let calldata = SimpleCast::calldata_encode("name()(string)", &[] as &[&str]).ok()?;
        let calldata_bytes = calldata.parse::<Bytes>().ok()?;
        let contract_addr = address.parse::<Address>().ok()?;

        // Build transaction request
        let tx = TransactionRequest::default()
            .to(contract_addr)
            .input(TransactionInput::new(calldata_bytes))
            .with_input_and_data();

        // Make the call using cast.call
        let result = self
            .cast
            .call(&tx.into(), None, None, None, None)
            .await
            .ok()?;

        // Decode the result using alloy's ABI decoder
        let bytes = result
            .strip_prefix("0x")
            .and_then(|hex_str| hex::decode(hex_str).ok())?;

        let string_type = DynSolType::String;
        match string_type.abi_decode(&bytes) {
            Ok(DynSolValue::String(s)) => {
                info!("Enriched contract {} with name: {}", address, s);
                Some(s)
            }
            _ => None,
        }
    }
}

impl CastClient {
    async fn resolve_address(&self, value: &str) -> Result<Address, rig::tool::ToolError> {
        let parsed = parse_name_or_address(value)?;
        parsed.resolve(&self.provider).await.map_err(|e| {
            tool_error(format!(
                "Failed to resolve '{value}' via {}: {e:#?}",
                self.rpc_url
            ))
        })
    }

    pub(crate) async fn balance(
        &self,
        address: String,
        block: Option<String>,
    ) -> Result<String, rig::tool::ToolError> {
        let account = self.resolve_address(&address).await?;
        let block_id = parse_block_identifier(block)?;
        let balance = self.cast.balance(account, block_id).await.map_err(|e| {
            tool_error(format!(
                "Failed to fetch balance via {}: {e:#?}",
                self.rpc_url
            ))
        })?;
        Ok(balance.to_string())
    }

    pub(crate) async fn eth_call(
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
            .map_err(|e| {
                tool_error(format!(
                    "eth_call execution failed via {}: {e:#?}",
                    self.rpc_url
                ))
            })
    }

    pub(crate) async fn send_transaction(
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

        let result = self.cast.send(tx.into()).await.map_err(|e| {
            tool_error(format!(
                "Transaction submission failed via {}: {e:#?}",
                self.rpc_url
            ))
        })?;

        Ok(result.tx_hash().to_string())
    }

    pub(crate) async fn contract_code(
        &self,
        address: String,
    ) -> Result<String, rig::tool::ToolError> {
        let addr = self.resolve_address(&address).await?;
        self.cast
            .code(addr, None, false)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch contract code: {e:#?}")))
    }

    pub(crate) async fn contract_code_size(
        &self,
        address: String,
    ) -> Result<String, rig::tool::ToolError> {
        let addr = self.resolve_address(&address).await?;
        let size = self
            .cast
            .codesize(addr, None)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch contract code size: {e:#?}")))?;
        Ok(size.to_string())
    }

    pub(crate) async fn transaction_details(
        &self,
        tx_hash: String,
        field: Option<String>,
    ) -> Result<String, rig::tool::ToolError> {
        let hash = tx_hash
            .parse::<B256>()
            .map_err(|e| tool_error(format!("Invalid transaction hash '{tx_hash}': {e:#?}")))?;

        let tx = self
            .provider
            .get_transaction_by_hash(hash)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch transaction: {e:#?}")))?
            .ok_or_else(|| tool_error("Transaction not found"))?;

        let receipt = self
            .provider
            .get_transaction_receipt(hash)
            .await
            .map_err(|e| tool_error(format!("Failed to fetch transaction receipt: {e:#?}")))?;

        let tx_json = serde_json::to_value(&tx)
            .map_err(|e| tool_error(format!("Failed to serialize transaction: {e:#?}")))?;
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

    pub(crate) async fn block_details(
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
            .map_err(|e| tool_error(format!("Failed to fetch block: {e:#?}")))?
            .ok_or_else(|| tool_error("Block not found"))?;

        let block_json = serde_json::to_value(&block)
            .map_err(|e| tool_error(format!("Failed to serialize block: {e:#?}")))?;

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
            .map_err(|e| tool_error(format!("Failed to format block JSON: {e:#?}")))
    }
}

async fn get_client(network: Option<String>) -> Result<Arc<CastClient>, rig::tool::ToolError> {
    let network_key = network.unwrap_or_else(|| "testnet".to_string());
    debug!(
        target: "aomi_tools::cast",
        network = %network_key,
        "Retrieving Cast client"
    );

    let clients = crate::clients::external_clients().await;
    clients.get_cast_client(&network_key).await
}

use rig::tool::ToolError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAccountBalanceParameters {
    pub address: String,
    pub block: Option<String>,
    pub network: Option<String>,
}

impl AomiToolArgs for GetAccountBalanceParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "address": { "type": "string" },
                "block": { "type": "string" },
                "network": { "type": "string" }
            },
            "required": ["address"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct GetAccountBalance;

pub async fn execute_get_account_balance(
    args: GetAccountBalanceParameters,
) -> Result<String, ToolError> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallViewFunctionParameters {
    pub from: String,
    pub to: String,
    pub value: String,
    pub input: Option<String>,
    pub network: Option<String>,
}

impl AomiToolArgs for CallViewFunctionParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "value": { "type": "string" },
                "input": {
                    "type": "string",
                    "description": "0x-prefixed calldata hex. Use EncodeFunctionCall output."
                },
                "network": { "type": "string" }
            },
            "required": ["from", "to", "value"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct CallViewFunction;

pub async fn execute_call_view_function(
    args: CallViewFunctionParameters,
) -> Result<String, ToolError> {
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
        let value = args.value.clone();
        let input = args.input.clone();
        let network = args.network.clone();
        let client = get_client(args.network).await?;
        let result = client
            .eth_call(args.from, args.to, value.clone(), input.clone())
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
        let tx_payload = json!({
            "from": from_log,
            "to": to_log,
            "value": value,
            "input": input,
            "network": network,
            "gas_limit": 10_000_000u64
        });

        Ok(format_simulation_output(result, tx_payload))
    })
}

fn format_simulation_output(
    result: Result<String, ToolError>,
    tx_payload: serde_json::Value,
) -> String {
    match result {
        Ok(output) => json!({
            "success": true,
            "result": output,
            "revert_reason": null,
            "tx": tx_payload
        })
        .to_string(),
        Err(err) => json!({
            "success": false,
            "result": null,
            "revert_reason": err.to_string(),
            "tx": tx_payload
        })
        .to_string(),
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

impl AomiToolArgs for SimulateContractCallParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "value": { "type": "string" },
                "input": {
                    "type": "string",
                    "description": "0x-prefixed calldata hex. Use EncodeFunctionCall output."
                },
                "network": { "type": "string" }
            },
            "required": ["from", "to", "value"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct SimulateContractCall;

pub async fn execute_simulate_contract_call(
    args: SimulateContractCallParameters,
) -> Result<String, ToolError> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTransactionParameters {
    pub from: String,
    pub to: String,
    pub value: String,
    pub input: Option<String>,
    pub network: Option<String>,
}

impl AomiToolArgs for SendTransactionParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "value": { "type": "string" },
                "input": {
                    "type": "string",
                    "description": "0x-prefixed calldata hex. Use EncodeFunctionCall output."
                },
                "network": { "type": "string" }
            },
            "required": ["from", "to", "value"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct SendTransaction;

pub async fn execute_send_transaction(
    args: SendTransactionParameters,
) -> Result<String, ToolError> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContractCodeParameters {
    pub address: String,
    pub network: Option<String>,
}

impl AomiToolArgs for GetContractCodeParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "address": { "type": "string" },
                "network": { "type": "string" }
            },
            "required": ["address"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct GetContractCode;

pub async fn execute_get_contract_code(
    args: GetContractCodeParameters,
) -> Result<String, ToolError> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContractCodeSizeParameters {
    pub address: String,
    pub network: Option<String>,
}

impl AomiToolArgs for GetContractCodeSizeParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "address": { "type": "string" },
                "network": { "type": "string" }
            },
            "required": ["address"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct GetContractCodeSize;

pub async fn execute_get_contract_code_size(
    args: GetContractCodeSizeParameters,
) -> Result<String, ToolError> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTransactionDetailsParameters {
    pub tx_hash: String,
    pub field: Option<String>,
    pub network: Option<String>,
}

impl AomiToolArgs for GetTransactionDetailsParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "tx_hash": { "type": "string" },
                "field": { "type": "string" },
                "network": { "type": "string" }
            },
            "required": ["tx_hash"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct GetTransactionDetails;

pub async fn execute_get_transaction_details(
    args: GetTransactionDetailsParameters,
) -> Result<String, ToolError> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBlockDetailsParameters {
    pub block: Option<String>,
    pub field: Option<String>,
    pub network: Option<String>,
}

impl AomiToolArgs for GetBlockDetailsParameters {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "block": { "type": "string" },
                "field": { "type": "string" },
                "network": { "type": "string" }
            },
            "required": []
        }))
    }
}

#[derive(Debug, Clone)]
pub struct GetBlockDetails;

pub async fn execute_get_block_details(
    args: GetBlockDetailsParameters,
) -> Result<String, ToolError> {
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

impl AomiTool for GetAccountBalance {
    const NAME: &'static str = "get_account_balance";

    type Args = GetAccountBalanceParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Get account balance using Cast."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_get_account_balance(args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for CallViewFunction {
    const NAME: &'static str = "call_view_function";

    type Args = CallViewFunctionParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Call a view function (read-only) using Cast. This performs an eth_call to read contract state without sending a transaction. Use this to validate calldata format and test if calls would succeed. The input must be 0x-prefixed hex calldata (use encode_function_call first)."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_call_view_function(args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for SimulateContractCall {
    const NAME: &'static str = "simulate_contract_call";

    type Args = SimulateContractCallParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Simulate a contract call using Cast to test if a transaction would succeed before sending it. IMPORTANT: Always simulate state-changing transactions with this tool before using send_transaction_to_wallet. This validates calldata format, checks for reverts, and estimates gas. Returns JSON with success/result/revert_reason and the transaction payload. The input must be 0x-prefixed hex calldata (use encode_function_call first)."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_simulate_contract_call(args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for SendTransaction {
    const NAME: &'static str = "send_transaction";

    type Args = SendTransactionParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Send a transaction using Cast."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_send_transaction(args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for GetContractCode {
    const NAME: &'static str = "get_contract_code";

    type Args = GetContractCodeParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Get contract bytecode using Cast."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_get_contract_code(args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for GetContractCodeSize {
    const NAME: &'static str = "get_contract_code_size";

    type Args = GetContractCodeSizeParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Get contract code size using Cast."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_get_contract_code_size(args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{format_simulation_output, tool_error};
    use serde_json::json;

    #[test]
    fn simulation_output_includes_success_payload() {
        let tx_payload = json!({
            "from": "0xabc",
            "to": "0xdef",
            "value": "0",
            "input": "0x1234",
            "network": "ethereum",
            "gas_limit": 10_000_000u64
        });
        let output = format_simulation_output(Ok("0xdeadbeef".to_string()), tx_payload.clone());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["result"], "0xdeadbeef");
        assert!(parsed["revert_reason"].is_null());
        assert_eq!(parsed["tx"], tx_payload);
    }

    #[test]
    fn simulation_output_includes_revert_details_on_error() {
        let tx_payload = json!({
            "from": "0xabc",
            "to": "0xdef",
            "value": "0",
            "input": null,
            "network": null,
            "gas_limit": 10_000_000u64
        });
        let err = tool_error("execution reverted");
        let output = format_simulation_output(Err(err), tx_payload.clone());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(parsed["success"], false);
        assert!(parsed["result"].is_null());
        assert!(
            parsed["revert_reason"]
                .as_str()
                .unwrap_or("")
                .contains("execution reverted")
        );
        assert_eq!(parsed["tx"], tx_payload);
    }
}

impl AomiTool for GetTransactionDetails {
    const NAME: &'static str = "get_transaction_details";

    type Args = GetTransactionDetailsParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Get transaction details using Cast."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_get_transaction_details(args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

impl AomiTool for GetBlockDetails {
    const NAME: &'static str = "get_block_details";

    type Args = GetBlockDetailsParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Get block details using Cast."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            execute_get_block_details(args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()))
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Needs etherscan API key"]
async fn test_arbitrum_balance_check() {
    // Test parameters
    let params = GetAccountBalanceParameters {
        address: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_string(),
        block: None,
        network: Some("arbitrum".to_string()),
    };

    println!("Testing Arbitrum balance query...");
    println!("  Address: {}", params.address);
    println!("  Network: arbitrum");

    // Execute the call
    let result = execute_get_account_balance(params).await;

    match result {
        Ok(balance) => {
            println!("✓ Successfully queried balance");
            println!("  Balance: {} wei", balance);
            println!("SUCCESS: Arbitrum RPC is working correctly!");
        }
        Err(e) => {
            eprintln!("✗ Failed to query balance");
            eprintln!("  Error: {:#?}", e);
            panic!("Balance query failed - see error above for details");
        }
    }
}
