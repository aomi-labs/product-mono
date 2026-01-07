use std::sync::Arc;

use anyhow::Result;

use aomi_tools::clients::external_clients;

use super::source_fetcher::SourceFetcher;

pub struct SharedForgeResources {
    source_fetcher: Arc<SourceFetcher>,
    baml_client: Arc<aomi_baml::BamlClient>,
}

impl SharedForgeResources {
    pub async fn new() -> Result<Self> {
        let source_fetcher = Arc::new(SourceFetcher::new());

        let clients = external_clients().await;
        let baml_client = clients
            .baml_client()
            .map_err(|e| anyhow::anyhow!("BAML client unavailable: {}", e))?;

        Ok(Self {
            source_fetcher,
            baml_client,
        })
    }

    pub fn source_fetcher(&self) -> Arc<SourceFetcher> {
        Arc::clone(&self.source_fetcher)
    }

    pub fn baml_client(&self) -> Arc<aomi_baml::BamlClient> {
        Arc::clone(&self.baml_client)
    }
}
