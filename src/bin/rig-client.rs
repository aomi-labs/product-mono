use rmcp::{service::RunningService, RoleClient, ServiceExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use anyhow::Result;
use toml;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub mcp: McpConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct McpConfig {
    pub server: Vec<McpServerConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(flatten)]
    pub transport: McpServerTransportConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "protocol", rename_all = "lowercase")]
pub enum McpServerTransportConfig {
    Sse { url: String },
    Stdio { command: String, args: Option<Vec<String>> },
}

impl McpServerTransportConfig {
    pub async fn start(&self) -> Result<RunningService<RoleClient, ()>> {
        match self {
            McpServerTransportConfig::Sse { url } => {
                let transport = rmcp::transport::SseTransport::start(url).await?;
                ().serve(transport).await.map_err(Into::into)
            }
            McpServerTransportConfig::Stdio { command, args } => {
                let mut cmd = tokio::process::Command::new(command);
                if let Some(args) = args {
                    cmd.args(args);
                }
                let transport = rmcp::transport::TokioChildProcess::new(&mut cmd)?;
                ().serve(transport).await.map_err(Into::into)
            }
        }
    }
}

impl Config {
    pub async fn retrieve(path: impl AsRef<Path>) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load config
    let config = Config::retrieve("config.toml").await?;
    // Start all MCP servers
    let mut clients = HashMap::new();
    for server in &config.mcp.server {
        let client = server.transport.start().await?;
        clients.insert(server.name.clone(), client);
    }
    println!("Connected to {} MCP server(s)", clients.len());
    // For demo, just use the first server
    let (_name, client) = clients.iter().next().expect("No MCP server configured");
    // List tools
    let tools = client.peer().list_all_tools().await?;
    println!("Available tools:");
    for tool in &tools {
        println!("- {}: {}", tool.name, tool.description.as_deref().unwrap_or(""));
    }
    // Simple CLI chat loop
    let mut output = BufWriter::new(tokio::io::stdout());
    let mut input = BufReader::new(tokio::io::stdin());
    output.write_all(b"Enter :q to quit\n").await?;
    loop {
        output.write_all(b"\x1b[32muser>\x1b[0m ").await?;
        output.flush().await?;
        let mut input_buf = String::new();
        input.read_line(&mut input_buf).await?;
        let input = input_buf.trim();
        if input == ":q" { break; }
        // For demo, just echo tool list or call a tool by name
        if input == ":tools" {
            for tool in &tools {
                output.write_all(format!("- {}\n", tool.name).as_bytes()).await?;
            }
            output.flush().await?;
            continue;
        }
        // Try to call a tool by name with empty args
        if let Some(tool) = tools.iter().find(|t| t.name == input) {
            let req = rmcp::model::CallToolRequestParam {
                name: tool.name.clone(),
                arguments: None,
            };
            let result = client.peer().call_tool(req).await;
            match result {
                Ok(res) => {
                    output.write_all(format!("\x1b[1;34mTool result:\x1b[0m {}\n", serde_json::to_string(&res).unwrap()).as_bytes()).await?;
                }
                Err(e) => {
                    output.write_all(format!("\x1b[1;31mError:\x1b[0m {}\n", e).as_bytes()).await?;
                }
            }
            output.flush().await?;
        } else {
            output.write_all(b"Unknown command or tool name. Type :tools to list tools.\n").await?;
            output.flush().await?;
        }
    }
    Ok(())
}
