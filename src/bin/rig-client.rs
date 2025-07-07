use foundry_config::figment::value::Map;
use rmcp::{service::RunningService, RoleClient, ServiceExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use anyhow::Result;
use toml;


struct ContractCtx {
    name: String,
root: Address,
retrievals: Map<String, String>, // return values of all fields and view functions 
source: String,
}

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

pub struct RigClient<'a> {
    pub tools: Vec<rmcp::model::Tool>,
    pub prompts: Vec<rmcp::model::Prompt>,
    pub resources: Vec<rmcp::model::Resource>,
    pub client: &'a RunningService<RoleClient, ()>,
}

impl<'a> RigClient<'a> {
    pub async fn run(&self) -> Result<()> {
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
                for tool in &self.tools {
                    output.write_all(format!("- {}\n", tool.name).as_bytes()).await?;
                }
                output.flush().await?;
                continue;
            }
            // Try to call a tool by name with empty args
            if let Some(tool) = self.tools.iter().find(|t| t.name == input) {
                let req = rmcp::model::CallToolRequestParam {
                    name: tool.name.clone(),
                    arguments: None,
                };
                let result = self.client.peer().call_tool(req).await;
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
    // List resources
    let resources = client.peer().list_all_resources().await?;
    println!("Available resources:");
    for resource in &resources {
        println!("- {}: {}", resource.name, resource.description.as_deref().unwrap_or(""));
    }
    // List prompts
    let prompts = client.peer().list_all_prompts().await?;
    println!("Available prompts:");
    for prompt in &prompts {
        println!("- {}: {}", prompt.name, prompt.description.as_deref().unwrap_or(""));
    }
    
    let rig_client = RigClient {
        tools,
        prompts,
        resources,
        client,
    };
    rig_client.run().await
}

struct ProtocolStrucutre {
    root: Address,
    retrievals: Map<String, String>,
    source: String,
}

use alloy_primitives::{hex, keccak256, Address, U256};
use alloy_sol_types::{sol, SolCall, SolError, SolValue};

sol! {
   function foo(uint256 a, uint256 b) external view returns (uint256);

   // These will generate structs prefixed with `overloaded_0`, `overloaded_1`,
   // and `overloaded_2` by default, but each signature is calculated with
   // `overloaded` as the function name.
   function overloaded();
   function overloaded(uint256) returns (uint256);
   function overloaded(string);

   // State variables will generate getter functions just like in Solidity.
   mapping(uint k => bool v) public variableGetter;

   /// Implements [`SolError`].
   #[derive(Debug, PartialEq, Eq)]
   error MyError(uint256 a, uint256 b);
}

#[test]
fn function() {
   assert_call_signature::<fooCall>("foo(uint256,uint256)");

   let call = fooCall { a: U256::from(1), b: U256::from(2) };
   let _call_data = call.abi_encode();

   let _ = overloaded_0Call {};
   assert_call_signature::<overloaded_0Call>("overloaded()");

   let _ = overloaded_1Call(U256::from(1));
   assert_call_signature::<overloaded_1Call>("overloaded(uint256)");

   let _ = overloaded_2Call("hello".into());
   assert_call_signature::<overloaded_2Call>("overloaded(string)");

   // Exactly the same as `function variableGetter(uint256) returns (bool)`.
   let _ = variableGetterCall { k: U256::from(2) };
   assert_call_signature::<variableGetterCall>("variableGetter(uint256)");
   let _ = variableGetterReturn { v: false };
}

fn assert_call_signature<T: SolCall>(expected: &str) {
    assert_eq!(T::SIGNATURE, expected);
    assert_eq!(T::SELECTOR, keccak256(expected)[..4]);
 }
 
 fn assert_error_signature<T: SolError>(expected: &str) {
    assert_eq!(T::SIGNATURE, expected);
    assert_eq!(T::SELECTOR, keccak256(expected)[..4]);
 }