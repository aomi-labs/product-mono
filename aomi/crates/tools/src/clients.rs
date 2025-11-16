use crate::cast::{network_urls, CastClient};
use crate::etherscan::EtherscanClient;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Shared external clients used across tools. Initialized once via ToolScheduler.
pub struct ExternalClients {
    cast_clients: RwLock<HashMap<String, Arc<CastClient>>>,
    brave_client: Arc<reqwest::Client>,
    brave_api_key: Option<String>,
    etherscan_client: Option<EtherscanClient>,
}

impl ExternalClients {
    pub fn new() -> Self {
        let brave_api_key = std::env::var("BRAVE_SEARCH_API_KEY").ok();
        let etherscan_client = EtherscanClient::from_env().ok();

        ExternalClients {
            cast_clients: RwLock::new(HashMap::new()),
            brave_client: Arc::new(reqwest::Client::new()),
            brave_api_key,
            etherscan_client,
        }
    }

    pub fn brave_client(&self) -> Arc<reqwest::Client> {
        self.brave_client.clone()
    }

    pub fn brave_api_key(&self) -> Option<String> {
        self.brave_api_key.clone()
    }

    pub fn etherscan_client(&self) -> Option<EtherscanClient> {
        self.etherscan_client.clone()
    }

    pub(crate) async fn get_cast_client(
        &self,
        network_key: &str,
    ) -> Result<Arc<CastClient>, rig::tool::ToolError> {
        // Read cache first
        if let Some(existing) = self.cast_clients.read().unwrap().get(network_key) {
            return Ok(existing.clone());
        }

        // Resolve RPC URL for this network
        let networks = network_urls();
        let rpc_url = networks.get(network_key).ok_or_else(|| {
            crate::cast::tool_error(format!(
                "Unsupported network '{network_key}'. Configure CHAIN_NETWORK_URLS_JSON to include it."
            ))
        })?;

        let client = Arc::new(CastClient::connect(rpc_url).await?);

        // Insert into cache if still absent
        let mut write_guard = self.cast_clients.write().unwrap();
        Ok(write_guard
            .entry(network_key.to_string())
            .or_insert_with(|| client.clone())
            .clone())
    }
}

// Global holder seeded by ToolScheduler; lazily initialized for test contexts.
static EXTERNAL_CLIENTS: OnceCell<Arc<ExternalClients>> = OnceCell::new();

pub fn external_clients() -> Arc<ExternalClients> {
    EXTERNAL_CLIENTS
        .get_or_init(|| Arc::new(ExternalClients::new()))
        .clone()
}

pub fn init_external_clients(clients: Arc<ExternalClients>) {
    let _ = EXTERNAL_CLIENTS.set(clients);
}
