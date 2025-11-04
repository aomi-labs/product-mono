// Environment variables
static MCP_SERVER_HOST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("MCP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
});
static MCP_SERVER_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("MCP_SERVER_PORT").unwrap_or_else(|_| "5000".to_string())
});

use eyre::Result;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};
use tracing_subscriber::{self, EnvFilter};

use crate::combined_tool::CombinedTool;

mod brave_search;
mod cast;
mod combined_tool;
mod etherscan;
mod zerox;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    // Parse command line arguments for network URLs
    let args: Vec<String> = std::env::args().collect();
    let network_urls_json = if args.len() > 1 {
        &args[1]
    } else {
        "{}" // Empty JSON if no argument provided
    };

    tracing::info!(
        "starting cast MCP server with network URLs: {}",
        network_urls_json
    );

    let tool = CombinedTool::new(network_urls_json).await?;
    let service = TowerToHyperService::new(StreamableHttpService::new(
        move || Ok(tool.clone()),
        LocalSessionManager::default().into(),
        Default::default(),
    ));

    // Get host and port from environment variables or use defaults
    let host = &*MCP_SERVER_HOST;
    let port = &*MCP_SERVER_PORT;
    let bind_addr = format!("{}:{}", host, port);

    tracing::info!("MCP server binding to {}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    loop {
        let io = tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            accept = listener.accept() => {
                TokioIo::new(accept?.0)
            }
        };
        let service = service.clone();
        tokio::spawn(async move {
            let _result = Builder::new(TokioExecutor::default())
                .serve_connection(io, service)
                .await;
        });
    }
    Ok(())
}
