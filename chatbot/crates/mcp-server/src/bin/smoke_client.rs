use eyre::Result;
use rmcp::{
    ServiceExt,
    model::{ClientCapabilities, ClientInfo, Implementation},
    transport::StreamableHttpClientTransport,
};
use std::process;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let url = match args.next() {
        Some(url) => url,
        None => {
            eprintln!("Usage: mcp-smoke-client <mcp-url> [network]");
            process::exit(1);
        }
    };
    let network = args.next();

    let transport = StreamableHttpClientTransport::from_uri(url.clone());
    let client_info = ClientInfo {
        protocol_version: Default::default(),
        capabilities: ClientCapabilities::default(),
        client_info: Implementation::default(),
    };

    let client = client_info.serve(transport).await?;
    let tools = client.list_tools(Default::default()).await?.tools;

    if let Some(network_name) = network {
        println!("✅ {network_name}: reachable ({} tools)", tools.len());
    } else {
        println!("✅ {url}: reachable ({} tools)", tools.len());
    }

    Ok(())
}
