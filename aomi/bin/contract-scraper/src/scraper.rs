use anyhow::Result;
use crate::clients::{DefiLlamaClient, CoinGeckoClient, EtherscanClient};
use crate::db::ContractStore;
use crate::models::{Contract, DataSource};

pub struct ContractScraper {
    defillama: DefiLlamaClient,
    _coingecko: CoinGeckoClient,
    etherscan: EtherscanClient,
    db: ContractStore,
}

impl ContractScraper {
    pub fn new(
        defillama: DefiLlamaClient,
        coingecko: CoinGeckoClient,
        etherscan: EtherscanClient,
        db: ContractStore,
    ) -> Self {
        Self {
            defillama,
            _coingecko: coingecko,
            etherscan,
            db,
        }
    }

    /// Scrape top contracts from DeFi Llama
    pub async fn scrape_top_contracts(
        &self,
        limit: usize,
        chains: &[String],
    ) -> Result<Vec<Contract>> {
        tracing::info!("Starting contract scraping with limit={}, chains={:?}", limit, chains);

        // 1. Get protocols from DeFi Llama
        tracing::info!("Fetching protocols from DeFi Llama...");
        let protocols = self.defillama.get_protocols().await?;
        tracing::info!("Retrieved {} protocols", protocols.len());

        // 2. Filter by TVL and sort
        let mut filtered = if chains.is_empty() {
            DefiLlamaClient::filter_by_tvl_and_chains(protocols, 0.0, &[])
        } else {
            DefiLlamaClient::filter_by_tvl_and_chains(protocols, 0.0, chains)
        };
        filtered = DefiLlamaClient::sort_by_tvl(filtered);

        // Take only top N protocols
        filtered.truncate(limit);
        tracing::info!("Filtered to {} top protocols", filtered.len());

        let mut contracts = Vec::new();

        // 3. For each protocol, get detailed information
        for (idx, protocol) in filtered.iter().enumerate() {
            tracing::info!("[{}/{}] Processing protocol: {}", idx + 1, filtered.len(), protocol.name);

            // Get protocol details
            match self.defillama.get_protocol(&protocol.id).await {
                Ok(detail) => {
                    tracing::debug!("Got details for {}: {} contracts", protocol.name, detail.contracts.len());

                    // Process each chain's contracts
                    for (chain, addresses) in detail.contracts.iter() {
                        // Convert chain name to chain_id
                        let chain_id = match crate::clients::etherscan::chain_to_chain_id(chain) {
                            Ok(id) => id,
                            Err(_) => {
                                tracing::warn!("Unsupported chain: {}", chain);
                                continue;
                            }
                        };

                        // Process each address
                        for address in addresses.iter().take(1) { // Take only first address per chain to avoid overwhelming APIs
                            tracing::debug!("Fetching source code for {} on chain {}", address, chain);

                            match self.etherscan.get_contract_source(chain_id, address).await {
                                Ok(source) => {
                                    // Get transaction count
                                    let tx_count = self.etherscan.get_transaction_count(chain_id, address)
                                        .await
                                        .ok();

                                    // Get last activity
                                    let last_activity = self.etherscan.get_last_activity(chain_id, address)
                                        .await
                                        .ok()
                                        .flatten();

                                    // Detect proxy
                                    let (is_proxy, impl_addr) = self.etherscan.detect_proxy(chain_id, address)
                                        .await
                                        .unwrap_or((false, None));

                                    let contract = Contract::new(
                                        address.clone(),
                                        chain.clone(),
                                        chain_id,
                                        detail.name.clone(),
                                        source.source_code.clone(),
                                        source.abi.clone(),
                                        DataSource::DefiLlama,
                                    )
                                    .with_symbol(detail.symbol.clone().unwrap_or_default())
                                    .with_description(detail.description.clone().unwrap_or_default())
                                    .with_tvl(protocol.tvl)
                                    .with_proxy(is_proxy, impl_addr);

                                    let contract = if let Some(count) = tx_count {
                                        contract.with_transaction_count(count as i64)
                                    } else {
                                        contract
                                    };

                                    let contract = if let Some(ts) = last_activity {
                                        contract.with_last_activity(ts)
                                    } else {
                                        contract
                                    };

                                    contracts.push(contract);
                                    tracing::info!("✓ Successfully scraped {} ({}) on {}", detail.name, address, chain);
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to get source for {}: {}", address, e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get details for {}: {}", protocol.name, e);
                }
            }
        }

        tracing::info!("Scraped {} contracts total", contracts.len());
        Ok(contracts)
    }

    /// Update existing contracts in the database
    pub async fn update_existing_contracts(&self, chain_id: Option<i32>) -> Result<()> {
        tracing::info!("Updating existing contracts...");

        let contracts = if let Some(id) = chain_id {
            self.db.get_contracts_by_chain(id).await?
        } else {
            self.db.get_stale_contracts(7).await?
        };

        tracing::info!("Found {} contracts to update", contracts.len());

        for (idx, contract) in contracts.iter().enumerate() {
            tracing::info!(
                "[{}/{}] Updating contract {} on chain {}",
                idx + 1,
                contracts.len(),
                contract.address,
                contract.chain_id
            );

            // Refresh contract data
            match self.etherscan.get_contract_source(contract.chain_id, &contract.address).await {
                Ok(source) => {
                    let mut updated = contract.clone();
                    updated.source_code = source.source_code;
                    updated.abi = source.abi;

                    // Update transaction count
                    if let Ok(count) = self.etherscan.get_transaction_count(contract.chain_id, &contract.address).await {
                        updated.transaction_count = Some(count as i64);
                    }

                    // Update last activity
                    if let Ok(Some(ts)) = self.etherscan.get_last_activity(contract.chain_id, &contract.address).await {
                        updated.last_activity_at = Some(ts);
                    }

                    self.db.upsert_contract(&updated).await?;
                    tracing::info!("✓ Updated contract {}", contract.address);
                }
                Err(e) => {
                    tracing::warn!("Failed to update {}: {}", contract.address, e);
                }
            }
        }

        tracing::info!("Update complete");
        Ok(())
    }

    /// Verify a specific contract
    pub async fn verify_contract(&self, chain_id: i32, address: &str) -> Result<()> {
        tracing::info!("Verifying contract {} on chain {}...", address, chain_id);

        // Check if contract exists in database
        if let Some(contract) = self.db.get_contract(chain_id, address).await? {
            tracing::info!("Contract found in database:");
            tracing::info!("  Name: {}", contract.name);
            tracing::info!("  Chain: {}", contract.chain);
            tracing::info!("  Symbol: {:?}", contract.symbol);
            tracing::info!("  Is Proxy: {}", contract.is_proxy);
            if let Some(impl_addr) = contract.implementation_address {
                tracing::info!("  Implementation: {}", impl_addr);
            }
            tracing::info!("  TVL: {:?}", contract.tvl);
            tracing::info!("  Transaction Count: {:?}", contract.transaction_count);
            tracing::info!("  Data Source: {}", contract.data_source);
        } else {
            tracing::info!("Contract not found in database");
        }

        // Verify against Etherscan
        tracing::info!("Fetching from Etherscan...");
        match self.etherscan.get_contract_source(chain_id, address).await {
            Ok(source) => {
                tracing::info!("✓ Contract verified on Etherscan");
                tracing::info!("  Contract Name: {}", source.contract_name);
                tracing::info!("  Compiler: {}", source.compiler_version);
                tracing::info!("  Is Proxy: {}", source.proxy == "1");
                if !source.implementation.is_empty() {
                    tracing::info!("  Implementation: {}", source.implementation);
                }
            }
            Err(e) => {
                tracing::error!("Failed to verify contract: {}", e);
            }
        }

        Ok(())
    }

    /// Save contracts to database
    pub async fn save_contracts(&self, contracts: &[Contract]) -> Result<()> {
        if contracts.is_empty() {
            tracing::info!("No contracts to save");
            return Ok(());
        }

        tracing::info!("Saving {} contracts to database...", contracts.len());
        self.db.upsert_contracts_batch(contracts).await?;
        tracing::info!("✓ Successfully saved all contracts");

        Ok(())
    }
}
