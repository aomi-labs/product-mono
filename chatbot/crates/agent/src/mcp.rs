use eyre::Result;
use rmcp::{
    RoleClient, ServiceExt,
    model::{ClientCapabilities, ClientInfo, Implementation, Tool as RmcpTool},
    service::{RunningService, ServerSink},
    transport::StreamableHttpClientTransport,
};
use std::sync::{Arc, LazyLock};
use tokio::sync::{OnceCell, mpsc};

use crate::ChatCommand;

/// Lazily-evaluated MCP server host (defaults to localhost for local development).
pub static MCP_SERVER_HOST: LazyLock<String> =
    LazyLock::new(|| std::env::var("MCP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()));

/// Lazily-evaluated MCP server port (defaults to 5000 to match the dev server).
pub static MCP_SERVER_PORT: LazyLock<String> =
    LazyLock::new(|| std::env::var("MCP_SERVER_PORT").unwrap_or_else(|_| "5000".to_string()));

/// Type alias for the running MCP client service.
pub type McpClient = RunningService<RoleClient, ClientInfo>;

/// Global handle that keeps the MCP connection alive for the lifetime of the process.
static MCP_TOOLBOX: OnceCell<Arc<McpToolBox>> = OnceCell::const_new();

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
    async fn connect() -> Result<Self> {
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
            .map_err(|e| eyre::eyre!("Failed to connect to MCP server at {}: {:?}", url, e))?;

        let tools = client
            .list_tools(Default::default())
            .await
            .map_err(|e| eyre::eyre!("Failed to list MCP tools: {:?}", e))?
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

/// Attempt to obtain the toolbox with retry feedback for the UI path.
pub async fn toolbox_with_retry(
    sender_to_ui: mpsc::Sender<ChatCommand>,
) -> Result<Arc<McpToolBox>> {
    if let Some(existing) = MCP_TOOLBOX.get() {
        return Ok(existing.clone());
    }

    let mut attempt = 1;
    let max_attempts = 12; // About 1 minute of retries
    let mut delay = std::time::Duration::from_millis(500);

    loop {
        let _ = sender_to_ui
            .send(ChatCommand::BackendConnecting(format!(
                "Connecting to MCP server (attempt {attempt}/{max_attempts})"
            )))
            .await;

        match McpToolBox::connect().await {
            Ok(toolbox) => {
                if let Err(e) = toolbox.ensure_connected().await {
                    let _ = sender_to_ui
                        .send(ChatCommand::Error(format!(
                            "MCP connection failed validation: {e}"
                        )))
                        .await;
                    return Err(e);
                }

                let arc = Arc::new(toolbox);
                if MCP_TOOLBOX.set(arc.clone()).is_err() {
                    if let Some(existing) = MCP_TOOLBOX.get() {
                        return Ok(existing.clone());
                    }
                }

                let _ = sender_to_ui
                    .send(ChatCommand::System(
                        "✓ MCP server connection successful".to_string(),
                    ))
                    .await;
                return Ok(arc);
            }
            Err(e) => {
                if attempt >= max_attempts {
                    let mcp_url = server_url();
                    let _ = sender_to_ui.send(ChatCommand::Error(
                        format!("Failed to connect to MCP server after {max_attempts} attempts: {e}. Please make sure it's running at {mcp_url}")
                    )).await;
                    return Err(e.into());
                }

                let _ = sender_to_ui
                    .send(ChatCommand::BackendConnecting(format!(
                        "Connection failed, retrying in {:.1}s...",
                        delay.as_secs_f32()
                    )))
                    .await;

                tokio::time::sleep(delay).await;
                delay = std::cmp::min(delay * 2, std::time::Duration::from_secs(5)); // Max 5 second delay
                attempt += 1;
            }
        }
    }
}
