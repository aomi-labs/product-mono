//! Combined MCP tool exposing third-party API integrations (without native Cast/Brave tools).

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

use crate::zerox::ZeroXTool;

// Environment variables
static ZEROX_API_KEY: std::sync::LazyLock<Option<String>> =
    std::sync::LazyLock::new(|| std::env::var("ZEROX_API_KEY").ok());

#[derive(Clone)]
pub struct CombinedTool {
    zerox_tool: Option<ZeroXTool>,
    tool_router: ToolRouter<CombinedTool>,
}

#[tool_router]
impl CombinedTool {
    pub async fn new(_network_urls_json: &str) -> Result<Self> {
        let zerox_tool = ZEROX_API_KEY
            .as_ref()
            .as_ref()
            .map(|key| ZeroXTool::new(Some(key.to_string())));

        if zerox_tool.is_none() {
            tracing::warn!(
                "ZEROX_API_KEY not set, 0x swap pricing tool disabled. Get a free key at https://dashboard.0x.org"
            );
        }

        Ok(Self {
            zerox_tool,
            tool_router: Self::tool_router(),
        })
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
        let mut instructions =
            String::from("Third-party API tools available without native Cast/Brave integrations.");

        if self.zerox_tool.is_some() {
            instructions.push_str("\n\n0x API:");
            instructions.push_str("\n  • get_swap_price: Retrieve swap price quotes (cached 30s)");
        } else {
            instructions
                .push_str("\n\n0x API not configured (set ZEROX_API_KEY to enable swap pricing).");
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
