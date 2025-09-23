// Environment variables
static MCP_SERVER_HOST: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("MCP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()));
static MCP_SERVER_PORT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("MCP_SERVER_PORT").unwrap_or_else(|_| "5000".to_string()));

use std::collections::HashMap;

use eyre::{Result, WrapErr};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use rmcp::transport::{StreamableHttpService, streamable_http_server::session::local::LocalSessionManager};
use tokio::sync::broadcast;
use tracing_subscriber::{self, EnvFilter};

use crate::combined_tool::CombinedTool;

mod brave_search;
mod cast;
mod combined_tool;
mod etherscan;
mod zerox;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).with_writer(std::io::stderr).init();

    // Parse command line arguments for network URLs
    let args: Vec<String> = std::env::args().collect();
    let network_urls_json = if args.len() > 1 {
        &args[1]
    } else {
        "{}" // Empty JSON if no argument provided
    };

    let mut network_urls: HashMap<String, String> = match serde_json::from_str(network_urls_json) {
        Ok(map) => map,
        Err(err) => {
            tracing::warn!("Failed to parse network URLs JSON: {}. Falling back to testnet only", err);
            HashMap::from([("testnet".to_string(), "http://127.0.0.1:8545".to_string())])
        }
    };

    if network_urls.is_empty() {
        tracing::warn!("No networks configured. Defaulting to local testnet");
        network_urls.insert("testnet".to_string(), "http://127.0.0.1:8545".to_string());
    }

    let mut networks: Vec<(String, String)> = network_urls.into_iter().collect();
    networks.sort_by(|a, b| a.0.cmp(&b.0));
    if let Some(pos) = networks.iter().position(|(name, _)| name == "testnet") {
        let entry = networks.remove(pos);
        networks.insert(0, entry);
    }

    let host = (*MCP_SERVER_HOST).clone();
    let base_port: u16 =
        (*MCP_SERVER_PORT).parse().wrap_err_with(|| format!("Invalid MCP_SERVER_PORT: {}", *MCP_SERVER_PORT))?;

    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut handles = Vec::new();

    tracing::info!("starting cast MCP servers with network URLs: {}", network_urls_json);

    for (index, (network_name, rpc_url)) in networks.into_iter().enumerate() {
        let port = base_port
            .checked_add(index as u16)
            .ok_or_else(|| eyre::eyre!("Port overflow while assigning port for network {}", network_name))?;

        let tool = if index == 0 {
            CombinedTool::new(network_name.clone(), rpc_url.clone())
                .await
                .wrap_err_with(|| format!("Failed to initialize primary network {}", network_name))?
        } else {
            match CombinedTool::new(network_name.clone(), rpc_url.clone()).await {
                Ok(tool) => tool,
                Err(err) => {
                    tracing::error!("Failed to initialize network {}: {}", network_name, err);
                    continue;
                }
            }
        };

        let shutdown_rx = shutdown_tx.subscribe();
        let host_clone = host.clone();
        tracing::info!("Launching {} MCP server on {}:{}", network_name, host_clone, port);

        let handle = tokio::spawn(async move {
            if let Err(err) = run_server(network_name.clone(), host_clone, port, tool, shutdown_rx).await {
                tracing::error!("MCP server for {} failed: {}", network_name, err);
            }
        });

        handles.push(handle);
    }

    if handles.is_empty() {
        return Err(eyre::eyre!("No MCP servers were started"));
    }

    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received for MCP servers");
    let _ = shutdown_tx.send(());

    for handle in handles {
        if let Err(err) = handle.await {
            tracing::error!("MCP server task panicked: {}", err);
        }
    }

    Ok(())
}

async fn run_server(
    network_name: String,
    host: String,
    port: u16,
    tool: CombinedTool,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<()> {
    let bind_addr = format!("{}:{}", host, port);

    let tool_for_service = tool.clone();
    let service = TowerToHyperService::new(StreamableHttpService::new(
        move || Ok(tool_for_service.clone()),
        LocalSessionManager::default().into(),
        Default::default(),
    ));

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .wrap_err_with(|| format!("Failed to bind MCP server for {} on {}", network_name, bind_addr))?;

    tracing::info!("MCP server ready for {} at {}", network_name, bind_addr);

    loop {
        tokio::select! {
            recv = shutdown_rx.recv() => {
                match recv {
                    Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("Shutting down MCP server for {} at {}", network_name, bind_addr);
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        let service = service.clone();
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            if let Err(err) = Builder::new(TokioExecutor::default())
                                .serve_connection(io, service)
                                .await
                            {
                                tracing::error!("Connection error in MCP server: {}", err);
                            }
                        });
                    }
                    Err(err) => {
                        tracing::error!("Failed to accept connection on {}: {}", bind_addr, err);
                    }
                }
            }
        }
    }

    Ok(())
}
