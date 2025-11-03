use eyre::{Context, Result};
use rmcp::{
    RoleClient, ServiceExt,
    model::{ClientCapabilities, ClientInfo, Implementation, Tool as RmcpTool},
    service::{RunningService, ServerSink},
    transport::StreamableHttpClientTransport,
};
use std::sync::{Arc, LazyLock};
use tokio::sync::{OnceCell, mpsc};


/// Lazily-evaluated MCP server host (defaults to localhost for local development).
pub static MCP_SERVER_HOST: LazyLock<String> =
    LazyLock::new(|| std::env::var("MCP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()));

/// Lazily-evaluated MCP server port (defaults to 5000 to match the dev server).
pub static MCP_SERVER_PORT: LazyLock<String> =
    LazyLock::new(|| std::env::var("MCP_SERVER_PORT").unwrap_or_else(|_| "5000".to_string()));

/// Type alias for the running MCP client service.
pub type McpClient = RunningService<RoleClient, ClientInfo>;

/// Global handle that keeps the MCP connection alive for the lifetime of the process.
pub static MCP_TOOLBOX: OnceCell<Arc<McpToolBox>> = OnceCell::const_new();

/// Convenience helper to build the MCP server URL from environment configuration.
pub fn server_url() -> String {
    format!("http://{}:{}", *MCP_SERVER_HOST, *MCP_SERVER_PORT)
}

/// Lightweight wrapper that owns the MCP client and exposes helper methods.
pub struct McpToolBox {
    mcp_client: Arc<McpClient>,
    tools: Vec<RmcpTool>,
}

impl McpToolBox {
    pub async fn connect() -> Result<Self> {
        let url = server_url();
        let transport = StreamableHttpClientTransport::from_uri(url.clone());
        let client_info = ClientInfo {
            protocol_version: Default::default(),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation::default(),
        };

        let client = client_info
            .serve(transport)
            .await
            .wrap_err_with(|| format!("Failed to connect to MCP server at {url}"))?;

        let tools = client
            .list_tools(Default::default())
            .await
            .wrap_err("Failed to list MCP tools")?
            .tools;

        Ok(Self {
            mcp_client: Arc::new(client),
            tools,
        })
    }

    /// Re-run a connectivity check against the server.
    pub async fn ensure_connected(&self) -> Result<()> {
        self.mcp_client
            .peer_info()
            .ok_or_else(|| eyre::eyre!("MCP client missing peer info"))?;
        Ok(())
    }

    /// Retrieve a clone of the server sink for registering tools.
    pub fn mcp_client(&self) -> ServerSink {
        self.mcp_client.peer().clone()
    }

    pub fn tools(&self) -> &[RmcpTool] {
        &self.tools
    }
}

/// Get a shared MCP toolbox, establishing the connection on first use.
pub async fn toolbox() -> Result<Arc<McpToolBox>> {
    MCP_TOOLBOX
        .get_or_try_init(|| async {
            let toolbox = McpToolBox::connect().await?;
            toolbox.ensure_connected().await?;
            Ok(Arc::new(toolbox))
        })
        .await
        .map(Arc::clone)
}



#[cfg(test)]
mod tests {
    use crate::server_url;

    #[tokio::test]
    #[ignore] // Test when MCP server is running
    async fn test_mcp_connection() {
        let url = server_url();
        println!("MCP URL: {}", url);

        let toolbox = crate::toolbox().await.unwrap();
        toolbox.ensure_connected().await.unwrap();

        let tools = toolbox.tools();
        println!("Tools: {:?}", tools);
    }
}