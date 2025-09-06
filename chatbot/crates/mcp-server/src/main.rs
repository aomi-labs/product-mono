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

    tracing::info!("starting cast MCP server");

    let tool = CombinedTool::new().await?;
    let service = TowerToHyperService::new(StreamableHttpService::new(
        move || Ok(tool.clone()),
        LocalSessionManager::default().into(),
        Default::default(),
    ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

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
