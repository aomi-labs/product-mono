use alloy::network::AnyNetwork;
use alloy_provider::{DynProvider, ProviderBuilder};
use aomi_anvil::{default_endpoint, default_networks};
use aomi_baml::BamlClient;
use cast::Cast;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, RwLock};
use tokio::sync::OnceCell;
use tracing::warn;

async fn default_rpc_url() -> String {
    match default_endpoint().await {
        Ok(endpoint) => endpoint,
        Err(err) => {
            warn!("Failed to load providers.toml endpoint: {}", err);
            "http://127.0.0.1:8545".to_string()
        }
    }
}

pub(crate) const BRAVE_SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";
pub const ETHERSCAN_V2_URL: &str = "https://api.etherscan.io/v2/api";

/// Shared external clients used across tools. Initialized once via ToolScheduler.
pub struct ExternalClients {
    cast_clients: RwLock<HashMap<String, Arc<CastClient>>>, // NETWORK_JSON
    brave_builder: Option<Arc<reqwest::RequestBuilder>>,
    etherscan_client: Option<EtherscanClient>,
    baml_client: Option<Arc<BamlClient>>,
}

pub(crate) fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to create HTTP client")
}

impl ExternalClients {
    fn read_api_keys() -> (Option<String>, Option<String>) {
        let brave_api_key = env::var("BRAVE_SEARCH_API_KEY").ok();
        let etherscan_api_key = env::var("ETHERSCAN_API_KEY").ok();
        (brave_api_key, etherscan_api_key)
    }

    pub async fn new() -> Self {
        let cast_networks = match default_networks().await {
            Ok(networks) => networks,
            Err(err) => {
                warn!("Failed to load providers.toml networks: {}", err);
                HashMap::new()
            }
        };
        Self::new_with_networks(cast_networks).await
    }

    pub async fn new_with_networks(mut cast_networks: HashMap<String, String>) -> Self {
        let (brave_api_key, etherscan_api_key) = Self::read_api_keys();

        if !cast_networks.contains_key("testnet") {
            cast_networks.insert("testnet".to_string(), default_rpc_url().await);
        }
        let req_client = if brave_api_key.is_some() || etherscan_api_key.is_some() {
            Some(build_http_client())
        } else {
            None
        };

        let brave_builder = brave_api_key.as_ref().map(|key| {
            Arc::new(
                req_client
                    .clone()
                    .unwrap_or_else(build_http_client)
                    .get(BRAVE_SEARCH_URL)
                    .header("Accept", "application/json")
                    .header("Accept-Encoding", "gzip")
                    .header("X-Subscription-Token", key.clone()),
            )
        });

        let etherscan_client = etherscan_api_key.as_ref().map(|key| {
            let client = req_client.clone().unwrap_or_else(build_http_client);
            EtherscanClient::new(Arc::new(client.get(ETHERSCAN_V2_URL)), key.clone())
        });

        let baml_client = match BamlClient::new() {
            Ok(client) => Some(Arc::new(client)),
            Err(err) => {
                warn!("Failed to initialize BAML client: {}", err);
                None
            }
        };

        // Eagerly initialize Cast clients for all configured networks
        let mut cast_clients = HashMap::new();
        for (net, url) in cast_networks.iter() {
            match CastClient::connect(url).await {
                Ok(client) => {
                    cast_clients.insert(net.clone(), Arc::new(client));
                }
                Err(err) => {
                    warn!("Failed to init Cast client for {net} ({url}): {err}");
                }
            }
        }

        ExternalClients {
            cast_clients: RwLock::new(cast_clients),
            brave_builder,
            etherscan_client,
            baml_client,
        }
    }

    pub async fn new_empty() -> Self {
        ExternalClients {
            cast_clients: RwLock::new(HashMap::new()),
            brave_builder: None,
            etherscan_client: None,
            baml_client: None,
        }
    }

    pub fn brave_request(&self) -> Option<reqwest::RequestBuilder> {
        self.brave_builder.as_ref().and_then(|b| b.try_clone())
    }

    pub fn etherscan_client(&self) -> Option<EtherscanClient> {
        self.etherscan_client.clone()
    }

    pub async fn get_cast_client(
        &self,
        network_key: &str,
    ) -> Result<Arc<CastClient>, rig::tool::ToolError> {
        if let Some(existing) = self.cast_clients.read().unwrap().get(network_key) {
            return Ok(existing.clone());
        }

        Err(crate::cast::tool_error(format!(
            "Cast client for '{network_key}' not available. This usually means the internal testnet (anvil) is not running. Please restart the backend to reinitialize the blockchain connection."
        )))
    }

    pub fn baml_client(&self) -> Result<Arc<BamlClient>, rig::tool::ToolError> {
        self.baml_client
            .clone()
            .ok_or_else(|| crate::cast::tool_error("BAML client not initialized"))
    }
}

// Global holder seeded by ToolScheduler; lazily initialized for test contexts.
static EXTERNAL_CLIENTS: OnceCell<Arc<ExternalClients>> = OnceCell::const_new();

pub async fn external_clients() -> Arc<ExternalClients> {
    EXTERNAL_CLIENTS
        .get_or_init(|| async { Arc::new(ExternalClients::new().await) })
        .await
        .clone()
}

pub async fn init_external_clients(clients: Arc<ExternalClients>) {
    let _ = EXTERNAL_CLIENTS.set(clients);
}

pub struct CastClient {
    pub provider: DynProvider<AnyNetwork>,
    pub(crate) cast: Cast<DynProvider<AnyNetwork>>,
    pub(crate) rpc_url: String,
}

impl CastClient {
    pub(crate) async fn connect(rpc_url: &str) -> Result<Self, rig::tool::ToolError> {
        let provider = ProviderBuilder::<_, _, AnyNetwork>::default()
            .connect(rpc_url)
            .await
            .map_err(|e| {
                crate::cast::tool_error(format!(
                    "Failed to connect to blockchain at {rpc_url}: {e}. If this is the internal testnet, ensure anvil is running."
                ))
            })?;

        let provider_dyn = DynProvider::new(provider.clone());
        let cast = Cast::new(DynProvider::new(provider));

        Ok(Self {
            provider: provider_dyn,
            cast,
            rpc_url: rpc_url.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct EtherscanClient {
    pub(crate) builder: Arc<reqwest::RequestBuilder>,
    pub(crate) api_key: String,
}

impl EtherscanClient {
    pub fn new(builder: Arc<reqwest::RequestBuilder>, api_key: impl Into<String>) -> Self {
        Self {
            builder,
            api_key: api_key.into(),
        }
    }
}
