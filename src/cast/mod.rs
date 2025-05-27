use alloy_primitives::Address;
use alloy_primitives::hex;
use alloy_primitives::{B256, U256};
use alloy_provider::RootProvider;
use alloy_provider::{Provider, ProviderBuilder, network::AnyNetwork};
use alloy_rpc_types::BlockId;
use alloy_rpc_types::BlockNumberOrTag as BlockNumber;
use alloy_rpc_types::BlockNumberOrTag::Latest;
use alloy_rpc_types::TransactionInput;
use alloy_rpc_types::TransactionRequest;
use alloy_rpc_types::{Block, Transaction, TransactionReceipt};
use alloy_serde::WithOtherFields;
use alloy_transport_http::Http;
use cast::Cast;
use foundry_cli::opts::RpcOpts;
use foundry_cli::utils::LoadConfig;
use foundry_common::ens::NameOrAddress;
use foundry_config::Config;
use rmcp::{
    Error as McpError, RoleServer, ServerHandler, model::*, schemars, service::RequestContext, tool,
};
use std::str::FromStr;

// const CONFIG: &'static str = include_str!("../../foundry.toml");

#[derive(Debug, Clone)]
pub struct CastMCP {
    config: Config,
    provider: RootProvider<AnyNetwork>,
}

#[tool(tool_box)]
impl CastMCP {
    pub async fn new() -> Result<Self, McpError> {
        let mut config = RpcOpts::default().load_config().unwrap();
        // TODO: hacking cuz mcp desn't read env properly
        config.eth_rpc_url = Some(
            "https://eth-mainnet.g.alchemy.com/v2/4UjEl1ULr2lQYsGR5n7gGKd3pzgAzxKs".to_string(),
        );
        config.etherscan_api_key = Some("BYY29WWH6IHAB2KS8DXFG2S7YP9C5GQXT5".to_string());
        let provider = foundry_cli::utils::get_provider(&config).unwrap();
        Ok(Self { config, provider })
    }

    #[tool(description = "Get the balance of an account in wei")]
    async fn balance(
        &self,
        #[tool(param)]
        #[schemars(description = "The address or ENS name to check balance for")]
        who: String,
    ) -> Result<CallToolResult, McpError> {
        let address = NameOrAddress::from(who)
            .resolve(&self.provider)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to resolve address: {}", e), None)
            })?;
        let balance =
            self.provider.get_balance(address).await.map_err(|e| {
                McpError::internal_error(format!("Failed to get balance: {}", e), None)
            })?;

        Ok(CallToolResult::success(vec![Content::text(
            balance.to_string(),
        )]))
    }

    #[tool(description = "Get the nonce for an account")]
    async fn nonce(
        &self,
        #[tool(param)]
        #[schemars(description = "The address or ENS name to check nonce for")]
        who: String,
    ) -> Result<CallToolResult, McpError> {
        let address = NameOrAddress::from(who)
            .resolve(&self.provider)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to resolve address: {}", e), None)
            })?;

        let nonce = self
            .provider
            .get_transaction_count(address)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get nonce: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(
            nonce.to_string(),
        )]))
    }

    // Contract Interaction
    #[tool(description = "Perform a call on an account without publishing a transaction")]
    async fn call(
        &self,
        #[tool(param)]
        #[schemars(description = "The address to call")]
        address: String,
        #[tool(param)]
        #[schemars(description = "The calldata to send")]
        calldata: String,
    ) -> Result<CallToolResult, McpError> {
        let address = address
            .parse::<Address>()
            .map_err(|e| McpError::invalid_params(format!("Invalid address: {}", e), None))?;

        let calldata = hex::decode(calldata.trim_start_matches("0x"))
            .map_err(|e| McpError::invalid_params(format!("Invalid calldata: {}", e), None))?;

        let transaction_request = TransactionRequest::default()
            .to(address)
            .input(TransactionInput::new(calldata.into()));

        let result = self
            .provider
            .call(WithOtherFields::<TransactionRequest>::new(
                transaction_request,
            ))
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to call contract: {}", e), None)
            })?;

        Ok(CallToolResult::success(vec![Content::text(hex::encode(
            result,
        ))]))
    }

    // Data Conversion
    #[tool(description = "Convert wei into an ETH amount")]
    async fn from_wei(
        &self,
        #[tool(param)]
        #[schemars(description = "The amount in wei to convert")]
        wei: String,
    ) -> Result<CallToolResult, McpError> {
        let wei = wei
            .parse::<U256>()
            .map_err(|e| McpError::invalid_params(format!("Invalid wei amount: {}", e), None))?;

        let eth = wei.to_string();
        Ok(CallToolResult::success(vec![Content::text(eth)]))
    }

    #[tool(description = "Convert an ETH amount to wei")]
    async fn to_wei(
        &self,
        #[tool(param)]
        #[schemars(description = "The amount in ETH to convert")]
        eth: String,
    ) -> Result<CallToolResult, McpError> {
        let eth = eth
            .parse::<f64>()
            .map_err(|e| McpError::invalid_params(format!("Invalid ETH amount: {}", e), None))?;

        let wei = (eth * 1e18) as u128;
        Ok(CallToolResult::success(vec![Content::text(
            wei.to_string(),
        )]))
    }

    // Block and Transaction Data
    #[tool(description = "Get block information by number or hash")]
    async fn block(
        &self,
        #[tool(param)]
        #[schemars(description = "Block number or hash to get information for")]
        block: Option<String>,
        #[tool(param)]
        #[schemars(description = "Whether to include full transaction data")]
        full: Option<bool>,
        #[tool(param)]
        #[schemars(description = "Specific field to retrieve from the block")]
        field: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let block = block
            .map(|b| {
                if b.starts_with("0x") {
                    B256::from_str(&b).map(Into::into).ok()
                } else {
                    b.parse::<B256>().map(Into::into).ok()
                }
            })
            .flatten()
            .unwrap_or(BlockId::Number(Latest));

        let result = cast
            .block(block, full.unwrap_or(false), field)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get block: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get latest block number")]
    async fn block_number(&self) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let block_number = cast.block_number().await.map_err(|e| {
            McpError::internal_error(format!("Failed to get block number: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(
            block_number.to_string(),
        )]))
    }

    #[tool(description = "Get transaction information by hash")]
    async fn tx(
        &self,
        #[tool(param)]
        #[schemars(description = "Transaction hash to get information for")]
        tx_hash: Option<String>,
        #[tool(param)]
        #[schemars(description = "From address or ENS name")]
        from: Option<String>,
        #[tool(param)]
        #[schemars(description = "Nonce of the transaction")]
        nonce: Option<u64>,
        #[tool(param)]
        #[schemars(description = "Specific field to retrieve from the transaction")]
        field: Option<String>,
        #[tool(param)]
        #[schemars(description = "Whether to return raw transaction data")]
        raw: Option<bool>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let from = from.map(|f| NameOrAddress::from(f));

        let result = cast
            .transaction(tx_hash, from, nonce, field, raw.unwrap_or(false))
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to get transaction: {}", e), None)
            })?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get transaction receipt by hash")]
    async fn receipt(
        &self,
        #[tool(param)]
        #[schemars(description = "Transaction hash to get receipt for")]
        tx_hash: String,
        #[tool(param)]
        #[schemars(description = "Specific field to retrieve from the receipt")]
        field: Option<String>,
        #[tool(param)]
        #[schemars(description = "Number of confirmations to wait for")]
        confs: Option<u64>,
        #[tool(param)]
        #[schemars(description = "Timeout in seconds")]
        timeout: Option<u64>,
        #[tool(param)]
        #[schemars(description = "Whether to run asynchronously")]
        cast_async: Option<bool>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let result = cast
            .receipt(
                tx_hash,
                field,
                confs.unwrap_or(1),
                timeout,
                cast_async.unwrap_or(false),
            )
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to get transaction receipt: {}", e), None)
            })?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get block timestamp")]
    async fn age(
        &self,
        #[tool(param)]
        #[schemars(description = "Block number or hash to get timestamp for")]
        block: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let block = block
            .map(|b| {
                if b.starts_with("0x") {
                    B256::from_str(&b).map(Into::into).ok()
                } else {
                    b.parse::<B256>().map(Into::into).ok()
                }
            })
            .flatten()
            .unwrap_or(BlockId::Number(Latest));

        let result = cast.age(block).await.map_err(|e| {
            McpError::internal_error(format!("Failed to get block age: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get block base fee")]
    async fn base_fee(
        &self,
        #[tool(param)]
        #[schemars(description = "Block number or hash to get base fee for")]
        block: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let block = block
            .map(|b| {
                if b.starts_with("0x") {
                    B256::from_str(&b).map(Into::into).ok()
                } else {
                    b.parse::<B256>().map(Into::into).ok()
                }
            })
            .flatten()
            .unwrap_or(BlockId::Number(Latest));

        let result = cast.base_fee(block).await.map_err(|e| {
            McpError::internal_error(format!("Failed to get base fee: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    #[tool(description = "Get current gas price")]
    async fn gas_price(&self) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let gas_price = cast.gas_price().await.map_err(|e| {
            McpError::internal_error(format!("Failed to get gas price: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(
            gas_price.to_string(),
        )]))
    }

    #[tool(description = "Get raw value of contract's storage slot")]
    async fn storage(
        &self,
        #[tool(param)]
        #[schemars(description = "Contract address")]
        address: String,
        #[tool(param)]
        #[schemars(description = "Storage slot (as hex)")]
        slot: String,
        #[tool(param)]
        #[schemars(description = "Block number or hash (optional)")]
        block: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let address = address
            .parse::<Address>()
            .map_err(|e| McpError::invalid_params(format!("Invalid address: {}", e), None))?;
        let slot = B256::from_str(&slot)
            .map_err(|e| McpError::invalid_params(format!("Invalid slot: {}", e), None))?;
        let block = block
            .map(|b| {
                if b.starts_with("0x") {
                    B256::from_str(&b).map(Into::into).ok()
                } else {
                    b.parse::<B256>().map(Into::into).ok()
                }
            })
            .flatten();
        let result = cast
            .storage(address, slot, block)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get storage: {}", e), None))?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get contract's runtime bytecode")]
    async fn code(
        &self,
        #[tool(param)]
        #[schemars(description = "Contract address")]
        address: String,
        #[tool(param)]
        #[schemars(description = "Block number or hash (optional)")]
        block: Option<String>,
        #[tool(param)]
        #[schemars(description = "Disassemble bytecode")]
        disassemble: Option<bool>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let address = address
            .parse::<Address>()
            .map_err(|e| McpError::invalid_params(format!("Invalid address: {}", e), None))?;
        let block = block
            .map(|b| {
                if b.starts_with("0x") {
                    B256::from_str(&b).map(Into::into).ok()
                } else {
                    b.parse::<B256>().map(Into::into).ok()
                }
            })
            .flatten();
        let result = cast
            .code(address, block, disassemble.unwrap_or(false))
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get code: {}", e), None))?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get contract's bytecode size")]
    async fn codesize(
        &self,
        #[tool(param)]
        #[schemars(description = "Contract address")]
        address: String,
        #[tool(param)]
        #[schemars(description = "Block number or hash (optional)")]
        block: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let address = address
            .parse::<Address>()
            .map_err(|e| McpError::invalid_params(format!("Invalid address: {}", e), None))?;
        let block = block
            .map(|b| {
                if b.starts_with("0x") {
                    B256::from_str(&b).map(Into::into).ok()
                } else {
                    b.parse::<B256>().map(Into::into).ok()
                }
            })
            .flatten();
        let result = cast.codesize(address, block).await.map_err(|e| {
            McpError::internal_error(format!("Failed to get codesize: {}", e), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get contract's codehash")]
    async fn codehash(
        &self,
        #[tool(param)]
        #[schemars(description = "Contract address")]
        address: String,
        #[tool(param)]
        #[schemars(description = "Block number or hash (optional)")]
        block: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let address = address
            .parse::<Address>()
            .map_err(|e| McpError::invalid_params(format!("Invalid address: {}", e), None))?;
        let block = block
            .map(|b| {
                if b.starts_with("0x") {
                    B256::from_str(&b).map(Into::into).ok()
                } else {
                    b.parse::<B256>().map(Into::into).ok()
                }
            })
            .flatten();
        let result = cast.codehash(address, vec![], block).await.map_err(|e| {
            McpError::internal_error(format!("Failed to get codehash: {}", e), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get EIP-1967 implementation address")]
    async fn implementation(
        &self,
        #[tool(param)]
        #[schemars(description = "Contract address")]
        address: String,
        #[tool(param)]
        #[schemars(description = "Is beacon proxy?")]
        is_beacon: Option<bool>,
        #[tool(param)]
        #[schemars(description = "Block number or hash (optional)")]
        block: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let cast = Cast::new(&self.provider);
        let address = address
            .parse::<Address>()
            .map_err(|e| McpError::invalid_params(format!("Invalid address: {}", e), None))?;
        let block = block
            .map(|b| {
                if b.starts_with("0x") {
                    B256::from_str(&b).map(Into::into).ok()
                } else {
                    b.parse::<B256>().map(Into::into).ok()
                }
            })
            .flatten();
        let result = cast
            .implementation(address, is_beacon.unwrap_or(false), block)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to get implementation: {}", e), None)
            })?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }
}

#[tool(tool_box)]
impl ServerHandler for CastMCP {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Cast service provides Ethereum blockchain interaction tools including balance checks, nonce queries, contract calls, and unit conversions.".to_string()),
        }
    }
}
