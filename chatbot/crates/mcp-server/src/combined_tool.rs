//! Combined tool that includes both Cast and Brave Search functionality

// Environment variables
static BRAVE_SEARCH_API_KEY: std::sync::LazyLock<Option<String>> = std::sync::LazyLock::new(|| {
    std::env::var("BRAVE_SEARCH_API_KEY").ok()
});
static ETHERSCAN_API_KEY: std::sync::LazyLock<Option<String>> = std::sync::LazyLock::new(|| {
    std::env::var("ETHERSCAN_API_KEY").ok()
});
static ZEROX_API_KEY: std::sync::LazyLock<Option<String>> = std::sync::LazyLock::new(|| {
    std::env::var("ZEROX_API_KEY").ok()
});

use eyre::Result;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::tool::{Parameters, ToolRouter},
    model::{
        CallToolResult, Implementation, PaginatedRequestParam, ProtocolVersion, ServerCapabilities,
        ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};

use crate::{
    brave_search::BraveSearchTool, cast::CastTool, etherscan::EtherscanTool, zerox::ZeroXTool,
};

#[derive(Clone)]
pub struct CombinedTool {
    cast_tool: CastTool,
    brave_search_tool: Option<BraveSearchTool>,
    etherscan_tool: Option<EtherscanTool>,
    zerox_tool: Option<ZeroXTool>,
    tool_router: ToolRouter<CombinedTool>,
}

#[tool_router]
impl CombinedTool {
    pub async fn new() -> Result<Self> {
        let cast_tool = CastTool::new().await?;

        // Check if Brave API key is set
        let brave_search_tool = BRAVE_SEARCH_API_KEY.as_ref()
            .as_ref()
            .map(|key| BraveSearchTool::new(key.to_string()));

        if brave_search_tool.is_none() {
            tracing::warn!("BRAVE_SEARCH_API_KEY not set, Brave Search tool will not be available");
        }

        // Check if Etherscan API key is set
        let etherscan_tool = ETHERSCAN_API_KEY.as_ref()
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
            cast_tool,
            brave_search_tool,
            etherscan_tool,
            zerox_tool,
            tool_router: Self::tool_router(),
        })
    }

    #[tool(description = "Get the balance of an account in wei")]
    pub async fn balance(
        &self,
        params: Parameters<crate::cast::BalanceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.cast_tool.balance(params).await
    }

    #[tool(description = "Perform a call to an account or contract")]
    pub async fn call(
        &self,
        params: Parameters<crate::cast::SendParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.cast_tool.call(params).await
    }

    #[tool(description = "Sign and publish a transaction")]
    pub async fn send(
        &self,
        params: Parameters<crate::cast::SendParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.cast_tool.send(params).await
    }

    #[tool(description = "Get the runtime bytecode of a contract")]
    pub async fn code(
        &self,
        params: Parameters<crate::cast::CodeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.cast_tool.code(params).await
    }

    #[tool(
        description = "Returns SIZE of the runtime bytecode of a contract in bytes. Does NOT return the bytecode itself."
    )]
    pub async fn code_size(
        &self,
        params: Parameters<crate::cast::CodeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.cast_tool.code_size(params).await
    }

    #[tool(
        description = "Get information about a transaction by its hash. Can retrieve the full transaction data or a specific field."
    )]
    pub async fn tx(
        &self,
        params: Parameters<crate::cast::TxParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.cast_tool.tx(params).await
    }

    #[tool(
        description = "Get information about a block by number or get the latest block. Can retrieve specific fields like 'number' for block height or 'timestamp' for the block's Unix timestamp."
    )]
    pub async fn block(
        &self,
        params: Parameters<crate::cast::BlockParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.cast_tool.block(params).await
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
        let mut instructions = r#"Tools for making RPC calls to an ethereum network via cast.

Tips:
- Use `code_size` to detect if a contract is deployed or not. 0 means not deployed.
    - Example: if `code_size` returns 12345, the size is 12345 bytes, not 5 bytes (the length of the string).

Available Cast tools:
- balance: Get the balance of an account in wei
- call: Perform a call to an account or contract without publishing
- send: Sign and publish a transaction
- code: Get the runtime bytecode of a contract
- code_size: Get the size of runtime bytecode (useful for deployment checks)
- tx: Get transaction information by hash
- block: Get block information by number or latest
"#.to_string();

        if self.brave_search_tool.is_some() {
            instructions.push_str("\n\nBrave Search API is also available for web searches.");
        }

        if self.etherscan_tool.is_some() {
            instructions
                .push_str("\n\nEtherscan API is available for retrieving verified contract ABIs.");
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
