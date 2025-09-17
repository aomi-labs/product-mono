//! MCP tools for common `cast` operations and more!

// Environment variables
static ANVIL_HOST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("ANVIL_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
});
static ANVIL_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("ANVIL_PORT").unwrap_or_else(|_| "8545".to_string())
});

use std::sync::Arc;

use alloy::{
    eips::{BlockId, BlockNumberOrTag, RpcBlockHash},
    network::AnyNetwork,
    primitives::{Address, BlockHash, Bytes, U256},
    rpc::types::{TransactionInput, TransactionRequest},
};
use alloy_ens::NameOrAddress;
use alloy_provider::{DynProvider, Provider, ProviderBuilder};
use cast::Cast;
use eyre::Result;
use rmcp::{
    ErrorData,
    handler::server::tool::Parameters,
    model::{CallToolResult, Content},
    tool,
};
use tracing::debug;

/// Parameters for the `balance` tool
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BalanceParams {
    #[schemars(description = "The account address or ENS name to get the balance for")]
    pub(crate) who: String,
    #[schemars(
        description = "Optional block number (integer) or block hash (0x-prefixed hex) to query. Defaults to latest block if not specified"
    )]
    pub(crate) block: Option<String>,
}

impl BalanceParams {
    pub(crate) fn who(&self) -> Result<NameOrAddress> {
        // TODO: This code is repeated in other params so need to refactor it into a helper function
        if self.who.starts_with("0x") {
            Ok(NameOrAddress::Address(self.who.parse()?))
        } else {
            Ok(NameOrAddress::Name(self.who.clone()))
        }
    }

    pub(crate) fn block(&self) -> Result<Option<BlockId>> {
        match &self.block {
            None => Ok(None),
            Some(block) => {
                if let Ok(num) = block.parse::<u64>() {
                    Ok(Some(BlockId::Number(BlockNumberOrTag::Number(num))))
                } else if block.starts_with("0x") {
                    let hash = block.parse::<BlockHash>()?;
                    Ok(Some(BlockId::Hash(RpcBlockHash::from_hash(hash, None))))
                } else {
                    eyre::bail!("invalid block ID: {}", block);
                }
            }
        }
    }
}

/// Parameters for the `send` and `call` tools
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SendParams {
    #[schemars(description = "The account or contract address to send the transaction to")]
    pub(crate) to: String,
    #[schemars(description = "The account address to send the transaction from")]
    pub(crate) from: String,
    #[schemars(description = "The amount of ETH to send in wei")]
    pub(crate) value: String,
    #[schemars(description = "The calldata to send when making a contract call")]
    pub(crate) input: Option<String>,
}

impl SendParams {
    pub(crate) fn to(&self) -> Result<NameOrAddress> {
        if self.to.starts_with("0x") {
            Ok(NameOrAddress::Address(self.to.parse()?))
        } else {
            Ok(NameOrAddress::Name(self.to.clone()))
        }
    }

    pub(crate) fn from(&self) -> Result<NameOrAddress> {
        if self.from.starts_with("0x") {
            Ok(NameOrAddress::Address(self.from.parse()?))
        } else {
            Ok(NameOrAddress::Name(self.from.clone()))
        }
    }

    pub(crate) fn value(&self) -> Result<U256> {
        Ok(self.value.parse()?)
    }
}

/// Parameters for the `code` and `code_size` tools
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CodeParams {
    #[schemars(description = "The contract address")]
    pub(crate) address: String,
}

impl CodeParams {
    pub(crate) fn address(&self) -> Result<Address> {
        Ok(self.address.parse()?)
    }
}

/// Parameters for the `tx` tool
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TxParams {
    #[schemars(description = "The transaction hash to look up")]
    pub(crate) tx_hash: String,
    #[schemars(
        description = "Optional: If specified, only get the given field of the transaction"
    )]
    pub(crate) field: Option<String>,
}

/// Parameters for the `block` tool
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BlockParams {
    #[schemars(description = "Optional: Block number or 'latest' (default: 'latest')")]
    pub(crate) block: Option<String>,
    #[schemars(description = "Optional: If specified, only get the given field of the block")]
    pub(crate) field: Option<String>,
}

#[derive(Clone)]
pub struct CastTool {
    provider: DynProvider<AnyNetwork>,
    cast: Arc<Cast<DynProvider<AnyNetwork>>>,
}

impl CastTool {
    pub async fn new() -> Result<Self> {
        // Get Anvil URL from environment variables or use default
        let anvil_host = &*ANVIL_HOST;
        let anvil_port = &*ANVIL_PORT;
        let anvil_url = format!("http://{}:{}", anvil_host, anvil_port);
        
        let provider = ProviderBuilder::<_, _, AnyNetwork>::default()
            .connect(&anvil_url)
            .await?;

        tracing::info!("Connected to Anvil at {}", anvil_url);

        // Test the connection with a simpler method that should be more widely supported
        match provider.get_block_number().await {
            Ok(block_number) => {
                tracing::info!("Current block number: {}", block_number);
            }
            Err(e) => {
                tracing::warn!("Could not get block number (this may be normal for some Anvil versions): {}", e);
                // Continue anyway - the provider connection itself is established
            }
        }

        Ok(Self {
            provider: DynProvider::new(provider.clone()),
            cast: Arc::new(Cast::new(DynProvider::new(provider))),
        })
    }

    /// Get the balance of an account in wei
    #[tool(description = "Get the balance of an account in wei")]
    pub(crate) async fn balance(
        &self,
        Parameters(params): Parameters<BalanceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let who = params
            .who()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?
            .resolve(&self.provider)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let block = params
            .block()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        let balance = self
            .cast
            .balance(who, block)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(
            balance.to_string(),
        )]))
    }

    /// Perform a call on an account without publishing a transaction
    #[tool(description = "Perform a call on an account without publishing a transaction")]
    pub(crate) async fn call(
        &self,
        Parameters(params): Parameters<SendParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let to = params
            .to()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?
            .resolve(&self.provider)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let from = params
            .from()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?
            .resolve(&self.provider)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let value = params
            .value()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        let mut tx = TransactionRequest::default()
            .to(to)
            .from(from)
            .value(value)
            .gas_limit(10000000);

        // is a contract call
        if params.input.is_some() {
            let input: Bytes = params
                .input
                .unwrap()
                .parse()
                .map_err(|_| ErrorData::invalid_params("invalid input data", None))?;
            let input = TransactionInput::new(input);
            tx = tx.input(input).with_input_and_data();
        }

        let tx = self
            .cast
            .call(&tx.into(), None, None, None, None) // Doesn't support state overrides right now
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(tx)]))
    }

    /// Sign and publish a transaction
    #[tool(description = "Sign and publish a transaction")]
    pub(crate) async fn send(
        &self,
        Parameters(params): Parameters<SendParams>,
    ) -> Result<CallToolResult, ErrorData> {
        tracing::info!(
            "Send transaction request: to={}, from={}, value={:?}, input={:?}",
            params.to,
            params.from,
            params.value,
            params.input.as_ref().map(|i| &i[..20.min(i.len())])
        );

        let to = params
            .to()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?
            .resolve(&self.provider)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let from = params
            .from()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?
            .resolve(&self.provider)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let value = params
            .value()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        let mut tx = TransactionRequest::default()
            .to(to)
            .from(from)
            .value(value)
            .gas_limit(10000000);

        // is a contract call
        if params.input.is_some() {
            let input: Bytes = params
                .input
                .unwrap()
                .parse()
                .map_err(|_| ErrorData::invalid_params("invalid input data", None))?;
            let input = TransactionInput::new(input);
            tx = tx.input(input).with_input_and_data();
        }

        tracing::info!(
            "Submitting transaction to Anvil: from={:?}, to={:?}, value={:?}",
            from,
            to,
            value
        );

        let tx = self
            .cast
            .send(tx.into())
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None));

        match tx {
            Ok(tx) => {
                let tx_hash = tx.tx_hash().to_string();
                tracing::info!("Transaction submitted successfully: {}", tx_hash);
                Ok(CallToolResult::success(vec![Content::text(tx_hash)]))
            }
            Err(e) => {
                tracing::error!("Transaction failed: {}", e);
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    /// Get the runtime bytecode of a contract
    #[tool(description = "Get the runtime bytecode of a contract")]
    pub(crate) async fn code(
        &self,
        Parameters(params): Parameters<CodeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let address = params
            .address()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        let code = self
            .cast
            .code(address, None, false) // TODO: support block
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(code)]))
    }

    /// Get the size of the runtime bytecode of a contract in bytes
    #[tool(description = "Returns SIZE of the runtime bytecode of a contract in bytes.")]
    pub(crate) async fn code_size(
        &self,
        Parameters(params): Parameters<CodeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let address = params
            .address()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        let code = self
            .cast
            .codesize(address, None) // TODO: support block
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        debug!("code size: {}", code);

        Ok(CallToolResult::success(vec![Content::text(
            code.to_string(),
        )]))
    }

    /// Get information about a transaction
    #[tool(
        description = "Get information about a transaction by its hash. Returns the transaction data as JSON."
    )]
    pub(crate) async fn tx(
        &self,
        Parameters(params): Parameters<TxParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Parse the transaction hash
        let tx_hash = params
            .tx_hash
            .parse::<alloy::primitives::B256>()
            .map_err(|e| {
                ErrorData::invalid_params(format!("Invalid transaction hash: {e}"), None)
            })?;

        // Get the transaction
        let tx = self
            .provider
            .get_transaction_by_hash(tx_hash)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .ok_or_else(|| ErrorData::internal_error("Transaction not found", None))?;

        // Get the transaction receipt if available
        let receipt = self
            .provider
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        // Convert to JSON for easy access
        let tx_json = serde_json::to_value(&tx).map_err(|e| {
            ErrorData::internal_error(format!("Failed to serialize transaction: {e}"), None)
        })?;

        let receipt_json = receipt.as_ref().and_then(|r| serde_json::to_value(r).ok());

        // If a specific field is requested, try to extract it from JSON
        if let Some(field) = params.field {
            if let Some(value) = tx_json.get(&field) {
                Ok(CallToolResult::success(vec![Content::text(
                    value.to_string(),
                )]))
            } else if let Some(receipt) = &receipt_json {
                if let Some(value) = receipt.get(&field) {
                    Ok(CallToolResult::success(vec![Content::text(
                        value.to_string(),
                    )]))
                } else {
                    Err(ErrorData::invalid_params(
                        format!("Field '{field}' not found in transaction or receipt"),
                        None,
                    ))
                }
            } else {
                Err(ErrorData::invalid_params(
                    format!("Field '{field}' not found in transaction"),
                    None,
                ))
            }
        } else {
            // Return full transaction info as formatted JSON
            let mut output = format!("Transaction {tx_hash}:\n\n");
            output.push_str("Transaction Data:\n");
            output.push_str(
                &serde_json::to_string_pretty(&tx_json).unwrap_or_else(|_| tx_json.to_string()),
            );

            if let Some(receipt) = receipt_json {
                output.push_str("\n\nReceipt Data:\n");
                output.push_str(
                    &serde_json::to_string_pretty(&receipt).unwrap_or_else(|_| receipt.to_string()),
                );
            }

            Ok(CallToolResult::success(vec![Content::text(output)]))
        }
    }

    /// Get information about a block
    #[tool(
        description = "Get information about a block by number or get the latest block. Can retrieve specific fields like 'number' for block height or 'timestamp' for the block's Unix timestamp."
    )]
    pub(crate) async fn block(
        &self,
        Parameters(params): Parameters<BlockParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Parse block identifier
        let block_id = match params.block.as_deref() {
            None | Some("latest") => BlockId::Number(BlockNumberOrTag::Latest),
            Some(block_str) => {
                if let Ok(num) = block_str.parse::<u64>() {
                    BlockId::Number(BlockNumberOrTag::Number(num))
                } else if block_str.starts_with("0x") {
                    let hash = block_str.parse::<BlockHash>().map_err(|e| {
                        ErrorData::invalid_params(format!("Invalid block hash: {e}"), None)
                    })?;
                    BlockId::Hash(RpcBlockHash::from_hash(hash, None))
                } else {
                    return Err(ErrorData::invalid_params(
                        format!(
                            "Invalid block identifier: {block_str}. Use a number, hash, or 'latest'"
                        ),
                        None,
                    ));
                }
            }
        };

        // Get the block
        let block = self
            .provider
            .get_block(block_id)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .ok_or_else(|| ErrorData::internal_error("Block not found", None))?;

        // Convert to JSON for easy field access
        let block_json = serde_json::to_value(&block).map_err(|e| {
            ErrorData::internal_error(format!("Failed to serialize block: {e}"), None)
        })?;

        // If a specific field is requested, extract it
        if let Some(field) = params.field {
            if let Some(value) = block_json.get(&field) {
                // Special handling for 'number' field to return just the number
                if field == "number" {
                    if let Some(num) = value.as_u64() {
                        Ok(CallToolResult::success(vec![Content::text(
                            num.to_string(),
                        )]))
                    } else {
                        Ok(CallToolResult::success(vec![Content::text(
                            value.to_string(),
                        )]))
                    }
                } else {
                    Ok(CallToolResult::success(vec![Content::text(
                        value.to_string(),
                    )]))
                }
            } else {
                Err(ErrorData::invalid_params(
                    format!("Field '{field}' not found in block"),
                    None,
                ))
            }
        } else {
            // Return full block info
            let output = serde_json::to_string_pretty(&block_json)
                .unwrap_or_else(|_| block_json.to_string());
            Ok(CallToolResult::success(vec![Content::text(output)]))
        }
    }
}

// ServerHandler implementation moved to CombinedTool
// CastTool now functions as a pure tool provider without the ServerHandler trait
