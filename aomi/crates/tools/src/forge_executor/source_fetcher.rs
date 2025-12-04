use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::baml::ContractSource;
use crate::db_tools::get_or_fetch_contract;

use super::plan::OperationGroup;

/// Request to fetch a contract
#[derive(Clone, Debug)]
pub struct FetchRequest {
    pub chain_id: String,
    pub address: String,
    pub name: String,
}

/// Long-running source fetcher service (lives as long as ForgeExecutor)
pub struct SourceFetcher {
    cache: Arc<Mutex<HashMap<String, ContractSource>>>,
    fetch_tx: mpsc::UnboundedSender<FetchRequest>,
    task_handle: JoinHandle<()>,
}

impl SourceFetcher {
    /// Initialize new source fetcher with empty cache
    /// Starts long-running service that continuously processes fetch requests
    pub fn new() -> Self {
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<FetchRequest>();

        let cache_clone = cache.clone();

        // Long-running service task
        let task_handle = tokio::spawn(async move {
            while let Some(req) = fetch_rx.recv().await {
                let key = format!("{}:{}", req.chain_id, req.address);

                // Skip if already cached
                if cache_clone.lock().await.contains_key(&key) {
                    continue;
                }

                // Fetch using get_or_fetch_contract from db_tools.rs
                match Self::fetch_contract_data(&req).await {
                    Ok(source) => {
                        info!("Fetched and cached contract: {}", key);
                        cache_clone.lock().await.insert(key, source);
                    }
                    Err(e) => {
                        error!("Failed to fetch {}: {}", key, e);
                    }
                }
            }
        });

        Self {
            cache,
            fetch_tx,
            task_handle,
        }
    }

    /// Submit fetch requests for contracts (non-blocking)
    pub fn request_fetch(&self, contracts: Vec<(String, String, String)>) {
        tracing::debug!(
            "SourceFetcher request_fetch with contracts: {:?}",
            contracts
        );
        for (chain_id, address, name) in contracts {
            let _ = self.fetch_tx.send(FetchRequest {
                chain_id,
                address,
                name,
            });
        }
    }

    /// Get contracts for a group (checks cache only, returns immediately)
    pub async fn get_contracts_for_group(
        &self,
        group: &OperationGroup,
    ) -> Result<Vec<ContractSource>> {
        let cache = self.cache.lock().await;
        let mut result = Vec::new();

        for (chain_id, address, _) in &group.contracts {
            let key = format!("{}:{}", chain_id, address);

            if let Some(source) = cache.get(&key) {
                result.push(source.clone());
            } else {
                anyhow::bail!("Contract {} not yet cached", key);
            }
        }

        Ok(result)
    }

    /// Check if all contracts for groups are cached and ready
    pub async fn are_contracts_ready(&self, groups: &[&OperationGroup]) -> bool {
        let cache = self.cache.lock().await;

        for group in groups {
            for (chain_id, address, _) in &group.contracts {
                let key = format!("{}:{}", chain_id, address);
                if !cache.contains_key(&key) {
                    return false;
                }
            }
        }
        true
    }

    /// Return the list of contracts that are still missing from the cache.
    pub async fn missing_contracts(
        &self,
        groups: &[&OperationGroup],
    ) -> Vec<(String, String, String)> {
        let cache = self.cache.lock().await;
        let mut missing = Vec::new();

        for group in groups {
            for (chain_id, address, name) in &group.contracts {
                let key = format!("{}:{}", chain_id, address);
                if !cache.contains_key(&key) {
                    missing.push((chain_id.clone(), address.clone(), name.clone()));
                }
            }
        }

        missing
    }

    /// Stop the background worker.
    pub fn shutdown(&self) {
        self.task_handle.abort();
    }

    /// Helper to fetch contract data using db_tools::get_or_fetch_contract
    async fn fetch_contract_data(req: &FetchRequest) -> Result<ContractSource> {
        let chain_id_u32 = req
            .chain_id
            .parse::<u32>()
            .map_err(|e| anyhow!("Invalid chain_id: {}", e))?;

        let contract_data = get_or_fetch_contract(chain_id_u32, req.address.clone())
            .await
            .map_err(|e| anyhow!("Failed to fetch contract: {}", e))?;

        Ok(ContractSource {
            chain_id: req.chain_id.clone(),
            address: contract_data.address,
            name: req.name.clone(),
            abi: serde_json::to_string(&contract_data.abi)?,
            source_code: if contract_data.source_code.is_empty() {
                None
            } else {
                Some(contract_data.source_code)
            },
        })
    }
}

impl Default for SourceFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SourceFetcher {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_source_fetcher_basic() {
        let fetcher = SourceFetcher::new();

        // Request some contracts (this would fail in a real test without DB/API setup)
        let contracts = vec![
            ("1".to_string(), "0xabc".to_string(), "Test".to_string()),
            ("1".to_string(), "0xdef".to_string(), "Test2".to_string()),
        ];

        fetcher.request_fetch(contracts);

        // In a real test, we'd wait for fetching to complete
        // For now, this just tests that the API doesn't panic
    }

    #[tokio::test]
    async fn test_are_contracts_ready_empty() {
        let fetcher = SourceFetcher::new();

        let groups = vec![];
        assert!(fetcher.are_contracts_ready(&groups).await);
    }
}
