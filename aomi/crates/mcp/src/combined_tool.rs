//! Combined tool that includes network-aware Cast and other functionality

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// Environment variables
static BRAVE_SEARCH_API_KEY: std::sync::LazyLock<Option<String>> =
    std::sync::LazyLock::new(|| std::env::var("BRAVE_SEARCH_API_KEY").ok());
static ETHERSCAN_API_KEY: std::sync::LazyLock<Option<String>> =
    std::sync::LazyLock::new(|| std::env::var("ETHERSCAN_API_KEY").ok());
static ZEROX_API_KEY: std::sync::LazyLock<Option<String>> =
    std::sync::LazyLock::new(|| std::env::var("ZEROX_API_KEY").ok());

use eyre::Result;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::tool::{Parameters, ToolRouter},
    model::{
        CallToolResult, Content, Implementation, PaginatedRequestParam, ProtocolVersion,
        ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    brave_search::BraveSearchTool, cast::CastTool, etherscan::EtherscanTool, zerox::ZeroXTool,
};

/// Parameters for the set_network tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetNetworkParams {
    #[schemars(description = "Network to switch to (e.g., 'testnet', 'mainnet', 'polygon')")]
    pub network: String,
}

#[derive(Clone)]
pub struct CombinedTool {
    cast_tools: HashMap<String, CastTool>, // "mainnet" -> CastTool, "testnet" -> CastTool
    current_network: Arc<RwLock<String>>,  // Track active network
    brave_search_tool: Option<BraveSearchTool>,
    etherscan_tool: Option<EtherscanTool>,
    zerox_tool: Option<ZeroXTool>,
    tool_router: ToolRouter<CombinedTool>,
}

#[tool_router]
impl CombinedTool {
    pub async fn new(network_urls_json: &str) -> Result<Self> {
        // Parse network URLs from JSON
        let mut network_urls: HashMap<String, String> = serde_json::from_str(network_urls_json)
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "Failed to parse network URLs JSON: {}, using empty config",
                    e
                );
                HashMap::new()
            });
        if network_urls.is_empty() {
            tracing::warn!("No network URLs provided, using default testnet");
            network_urls.insert("testnet".to_string(), "http://127.0.0.1:8545".to_string());
        }

        tracing::info!(
            "Initializing networks: {:?}",
            network_urls.keys().collect::<Vec<_>>()
        );

        // Initialize CastTool for each network
        let mut cast_tools = HashMap::new();
        for (network_name, rpc_url) in network_urls {
            match CastTool::new_with_network(network_name.clone(), rpc_url).await {
                Ok(cast_tool) => {
                    cast_tools.insert(network_name.clone(), cast_tool);
                    tracing::info!("Successfully initialized {} network", network_name);
                }
                Err(e) => {
                    tracing::error!("Failed to initialize {} network: {}", network_name, e);
                    // Continue with other networks rather than failing completely
                }
            }
        }

        // Set default network (prefer testnet if available, otherwise first available)
        let default_network = if cast_tools.contains_key("testnet") {
            "testnet".to_string()
        } else {
            cast_tools
                .keys()
                .next()
                .cloned()
                .unwrap_or("testnet".to_string())
        };

        let current_network = Arc::new(RwLock::new(default_network));

        // Check if Brave API key is set
        let brave_search_tool = BRAVE_SEARCH_API_KEY
            .as_ref()
            .as_ref()
            .map(|key| BraveSearchTool::new(key.to_string()));

        if brave_search_tool.is_none() {
            tracing::warn!("BRAVE_SEARCH_API_KEY not set, Brave Search tool will not be available");
        }

        // Check if Etherscan API key is set
        let etherscan_tool = ETHERSCAN_API_KEY
            .as_ref()
            .as_ref()
            .map(|key| EtherscanTool::new(key.to_string()));

        if etherscan_tool.is_none() {
            tracing::warn!("ETHERSCAN_API_KEY not set, Etherscan tool will not be available");
        }

        // Check if 0x API key is set (required for 0x API)
        let zerox_api_key = ZEROX_API_KEY.as_ref();
        let zerox_tool = if let Some(key) = zerox_api_key {
            Some(ZeroXTool::new(Some(key.to_string())))
        } else {
            tracing::warn!(
                "ZEROX_API_KEY not set, 0x swap tools will not be available. Get a free API key at https://dashboard.0x.org"
            );
            None
        };

        Ok(Self {
            cast_tools,
            current_network,
            brave_search_tool,
            etherscan_tool,
            zerox_tool,
            tool_router: Self::tool_router(),
        })
    }

    fn get_current_cast_tool(&self) -> Result<&CastTool, ErrorData> {
        let current_network = self.current_network.read().unwrap();
        self.cast_tools.get(&*current_network).ok_or_else(|| {
            ErrorData::internal_error(
                format!("No cast tool available for network: {}", *current_network),
                None,
            )
        })
    }

    #[tool(description = "Switch the blockchain network for all subsequent operations")]
    pub async fn set_network(
        &self,
        params: Parameters<SetNetworkParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let network_name = params.0.network;

        // Check if the network exists
        if !self.cast_tools.contains_key(&network_name) {
            let available_networks: Vec<String> = self.cast_tools.keys().cloned().collect();
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Network '{}' not available. Available networks: {}",
                network_name,
                available_networks.join(", ")
            ))]));
        }

        // Switch to the new network
        {
            let mut current_network = self.current_network.write().unwrap();
            *current_network = network_name.clone();
        }

        tracing::info!("Switched to network: {}", network_name);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Successfully switched to {} network",
            network_name
        ))]))
    }

    #[tool(description = "Get the balance of an account in wei on current network")]
    pub async fn balance(
        &self,
        params: Parameters<crate::cast::BalanceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let cast_tool = self.get_current_cast_tool()?;

        let result = cast_tool.balance(params).await?;

        Ok(result)
    }

    #[tool(description = "Perform a call to an account or contract on current network")]
    pub async fn call(
        &self,
        params: Parameters<crate::cast::SendParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let cast_tool = self.get_current_cast_tool()?;

        let result = cast_tool.call(params).await?;

        Ok(result)
    }

    #[tool(description = "Sign and publish a transaction on current network")]
    pub async fn send(
        &self,
        params: Parameters<crate::cast::SendParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Safety check: Only allow send on testnet
        let current_network = self.current_network.read().unwrap().clone();
        if current_network == "mainnet" {
            return Ok(CallToolResult::error(vec![Content::text(
                "Sending transactions on mainnet is disabled for security. Use call for read-only operations."
                    .to_string(),
            )]));
        }

        let cast_tool = self.get_current_cast_tool()?;

        let result = cast_tool.send(params).await?;

        Ok(result)
    }

    #[tool(description = "Get the runtime bytecode of a contract on current network")]
    pub async fn code(
        &self,
        params: Parameters<crate::cast::CodeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let cast_tool = self.get_current_cast_tool()?;

        let result = cast_tool.code(params).await?;

        Ok(result)
    }

    #[tool(
        description = "Returns SIZE of the runtime bytecode of a contract in bytes on current network. Does NOT return the bytecode itself."
    )]
    pub async fn code_size(
        &self,
        params: Parameters<crate::cast::CodeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let cast_tool = self.get_current_cast_tool()?;

        let result = cast_tool.code_size(params).await?;

        Ok(result)
    }

    #[tool(
        description = "Get information about a transaction by its hash on current network. Can retrieve the full transaction data or a specific field."
    )]
    pub async fn tx(
        &self,
        params: Parameters<crate::cast::TxParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let cast_tool = self.get_current_cast_tool()?;

        let result = cast_tool.tx(params).await?;

        Ok(result)
    }

    #[tool(
        description = "Get information about a block by number or get the latest block on current network. Can retrieve specific fields like 'number' for block height or 'timestamp' for the block's Unix timestamp."
    )]
    pub async fn block(
        &self,
        params: Parameters<crate::cast::BlockParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let cast_tool = self.get_current_cast_tool()?;

        let result = cast_tool.block(params).await?;

        Ok(result)
    }

    #[tool(
        description = "Search the web using Brave Search API. Returns web search results including titles, URLs, and descriptions."
    )]
    pub async fn brave_search(
        &self,
        params: Parameters<crate::brave_search::BraveSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(ref brave_tool) = self.brave_search_tool {
            brave_tool.brave_search(params).await
        } else {
            Err(ErrorData::internal_error(
                "Brave Search tool not available. Please set BRAVE_SEARCH_API_KEY environment variable.",
                None,
            ))
        }
    }

    #[tool(
        description = "Get the ABI (Application Binary Interface) for a verified contract from Etherscan. Returns the full ABI JSON that can be used to interact with the contract."
    )]
    pub async fn get_contract_abi(
        &self,
        params: Parameters<crate::etherscan::GetAbiParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(ref etherscan_tool) = self.etherscan_tool {
            etherscan_tool.get_contract_abi(params).await
        } else {
            Err(ErrorData::internal_error(
                "Etherscan tool not available. Please set ETHERSCAN_API_KEY environment variable.",
                None,
            ))
        }
    }

    #[tool(
        description = "Get transaction history for an Ethereum address from Etherscan. Returns normal transactions and optionally internal transactions with pagination support."
    )]
    pub async fn get_transaction_history(
        &self,
        params: Parameters<crate::etherscan::GetTransactionHistoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(ref etherscan_tool) = self.etherscan_tool {
            etherscan_tool.get_transaction_history(params).await
        } else {
            Err(ErrorData::internal_error(
                "Etherscan tool not available. Please set ETHERSCAN_API_KEY environment variable.",
                None,
            ))
        }
    }

    #[tool(
        description = "Get a price estimate for swapping tokens using 0x API. This is fast and lightweight - use for displaying prices to users. Includes price impact and liquidity sources. Cached for 30 seconds. Works with Anvil/test environments."
    )]
    pub async fn get_swap_price(
        &self,
        params: Parameters<crate::zerox::SwapPriceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(ref zerox_tool) = self.zerox_tool {
            zerox_tool.get_swap_price(params).await
        } else {
            Err(ErrorData::internal_error(
                "0x swap tools not available. Please set ZEROX_API_KEY environment variable. Get a free API key at https://dashboard.0x.org",
                None,
            ))
        }
    }
}

#[tool_handler]
impl ServerHandler for CombinedTool {
    fn get_info(&self) -> ServerInfo {
        let available_networks: Vec<String> = self.cast_tools.keys().cloned().collect();
        let current_network = self.current_network.read().unwrap();

        let mut instructions = format!(
            r#"Network-aware tools for blockchain operations. Currently on: {}

Available networks: {}

Network Management:
- set_network: Switch between available networks
  - Testnet (local Anvil) allows all operations including transactions
  - Other networks may have restrictions for security

Tips:
- Use `code_size` to detect if a contract is deployed or not. 0 means not deployed.
- All tool responses include the current network for clarity
- Transaction sending may be restricted on certain networks for security

Available Blockchain tools:
- balance: Get the balance of an account in wei on current network
- call: Perform a call to an account or contract on current network (read-only)
- send: Sign and publish a transaction (restricted on some networks for security)
- code: Get the runtime bytecode of a contract on current network
- code_size: Get the size of runtime bytecode (useful for deployment checks)
- tx: Get transaction information by hash on current network
- block: Get block information by number or latest on current network
"#,
            *current_network,
            available_networks.join(", ")
        );

        if self.brave_search_tool.is_some() {
            instructions.push_str("\n\nBrave Search API is also available for web searches.");
        }

        if self.etherscan_tool.is_some() {
            instructions.push_str("\n\nEtherscan API is available:");
            instructions.push_str("\n  • get_contract_abi: Get verified contract ABIs");
            instructions.push_str(
                "\n  • get_transaction_history: Get address transaction history with pagination",
            );
        }

        if self.zerox_tool.is_some() {
            instructions.push_str("\n\n0x API is available:");
            instructions
                .push_str("\n  • get_swap_price: Get token swap prices (no taker required)");
            instructions.push_str("\n  Compatible with Anvil/test environments");
        }

        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(instructions),
        }
    }

    async fn initialize(
        &self,
        request: rmcp::model::InitializeRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::InitializeResult, rmcp::ErrorData> {
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        Ok(self.get_info())
    }

    async fn get_prompt(
        &self,
        _request: rmcp::model::GetPromptRequestParam,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::GetPromptResult, rmcp::ErrorData> {
        Ok(rmcp::model::GetPromptResult {
            description: None,
            messages: vec![],
        })
    }

    async fn list_prompts(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListPromptsResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListPromptsResult::default())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, ErrorData> {
        Ok(rmcp::model::ListResourcesResult::default())
    }

    async fn read_resource(
        &self,
        _request: rmcp::model::ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResult, ErrorData> {
        Ok(rmcp::model::ReadResourceResult { contents: vec![] })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListResourceTemplatesResult, ErrorData> {
        Ok(rmcp::model::ListResourceTemplatesResult::default())
    }
}
