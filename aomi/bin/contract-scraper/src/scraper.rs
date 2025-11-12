use anyhow::Result;
use crate::clients::{DefiLlamaClient, CoinGeckoClient, EtherscanClient};
use crate::db::ContractStore;
use crate::models::{Contract, DataSource};
use std::collections::HashMap;

pub struct ContractScraper {
    defillama: DefiLlamaClient,
    coingecko: CoinGeckoClient,
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
            coingecko,
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
            match self.defillama.get_protocol(&protocol.slug).await {
                Ok(detail) => {
                    // Determine which chain to use FIRST
                    // For multi-chain protocols, prefer the user's filtered chain over DeFi Llama's "primary" chain
                    let target_chain = if !chains.is_empty() {
                        // User specified chains - find which one this protocol supports
                        let chains_lower: Vec<String> = chains.iter().map(|c| c.to_lowercase()).collect();
                        protocol.chains.iter()
                            .find(|c| chains_lower.contains(&c.to_lowercase()))
                            .map(|c| c.as_str())
                            .unwrap_or(&detail.chain)
                    } else {
                        &detail.chain
                    };

                    // Try to get address from DeFi Llama first
                    let mut address = match &detail.address {
                        Some(addr) if !addr.is_empty() => {
                            // Clean address (remove chain prefix like "arbitrum:0x...")
                            Self::clean_address(addr)
                        }
                        _ => {
                            // No address from DeFi Llama, try CoinGecko fallback
                            tracing::warn!("No address from DeFi Llama for {}, trying CoinGecko...", protocol.name);
                            match self.try_coingecko_address(&protocol.name, target_chain).await {
                                Ok(addr) => addr,
                                Err(_) => {
                                    tracing::warn!("No contract address found for {} from any source", protocol.name);
                                    continue;
                                }
                            }
                        }
                    };

                    tracing::debug!("Got address for {}: {}", protocol.name, address);

                    // Convert chain name to chain_id
                    let chain_id = match crate::clients::etherscan::chain_to_chain_id(target_chain) {
                        Ok(id) => id,
                        Err(_) => {
                            tracing::warn!("Unsupported chain: {}", target_chain);
                            continue;
                        }
                    };

                    // Fetch contract source code from Etherscan
                    tracing::debug!("Fetching source code for {} on chain {}", address, target_chain);

                    let source = match self.etherscan.get_contract_source(chain_id, &address).await {
                        Ok(s) => s,
                        Err(e) => {
                            // Check if it's a 404 error (contract not found)
                            let is_404 = e.to_string().contains("404");

                            if is_404 {
                                // Try CoinGecko fallback for better address
                                tracing::warn!("Etherscan 404 for {}, trying CoinGecko fallback...", address);
                                match self.try_coingecko_address(&protocol.name, target_chain).await {
                                    Ok(coingecko_addr) if coingecko_addr != address => {
                                        address = coingecko_addr;
                                        tracing::info!("Retrying with CoinGecko address: {}", address);

                                        // Retry with CoinGecko address
                                        match self.etherscan.get_contract_source(chain_id, &address).await {
                                            Ok(s) => s,
                                            Err(e2) => {
                                                tracing::warn!("Failed to get source for {} even with CoinGecko address: {}", address, e2);
                                                continue;
                                            }
                                        }
                                    }
                                    _ => {
                                        tracing::warn!("Failed to get source for {}: {}", address, e);
                                        continue;
                                    }
                                }
                            } else {
                                tracing::warn!("Failed to get source for {}: {}", address, e);
                                continue;
                            }
                        }
                    };

                    // Get transaction count
                    let tx_count = self.etherscan.get_transaction_count(chain_id, &address)
                        .await
                        .ok();

                    // Get last activity
                    let last_activity = self.etherscan.get_last_activity(chain_id, &address)
                        .await
                        .ok()
                        .flatten();

                    // Detect proxy
                    let (is_proxy, impl_addr) = self.etherscan.detect_proxy(chain_id, &address)
                        .await
                        .unwrap_or((false, None));

                    let mut contract = Contract::new(
                        address.clone(),
                        target_chain.to_string(),
                        chain_id,
                        detail.name.clone(),
                        source.source_code.clone(),
                        source.abi.clone(),
                        DataSource::DefiLlama,
                    )
                    .with_symbol(detail.symbol.clone().unwrap_or_default())
                    .with_description(detail.description.clone().unwrap_or_default())
                    .with_proxy(is_proxy, impl_addr);

                    // Add TVL if available
                    if let Some(tvl) = protocol.tvl {
                        contract = contract.with_tvl(tvl);
                    }

                    // Add transaction count if available
                    if let Some(count) = tx_count {
                        contract = contract.with_transaction_count(count as i64);
                    }

                    // Add last activity if available
                    if let Some(ts) = last_activity {
                        contract = contract.with_last_activity(ts);
                    }

                    contracts.push(contract);
                    tracing::info!("✓ Successfully scraped {} ({}) on {}", detail.name, address, target_chain);
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

        let mut updated_count = 0;
        let mut failed_count = 0;

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
                    updated_count += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to update {}: {}", contract.address, e);
                    failed_count += 1;
                }
            }
        }

        tracing::info!("========================================");
        tracing::info!("Update Summary:");
        tracing::info!("========================================");
        tracing::info!("✓ Successfully updated: {} contracts", updated_count);
        if failed_count > 0 {
            tracing::info!("✗ Failed to update: {} contracts", failed_count);
        }
        tracing::info!("Total processed: {}", contracts.len());
        tracing::info!("========================================");

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

        // Print summary before saving
        self.print_scrape_summary(contracts);

        tracing::info!("Saving {} contracts to database...", contracts.len());
        self.db.upsert_contracts_batch(contracts).await?;

        // Verify the save was successful by counting database records
        let total_in_db = self.db.get_contract_count().await?;

        tracing::info!("✓ Successfully saved all {} contracts to database", contracts.len());
        tracing::info!("✓ Total contracts in database: {}", total_in_db);

        Ok(())
    }

    /// Print a summary of scraped contracts
    fn print_scrape_summary(&self, contracts: &[Contract]) {
        // Count by chain
        let mut by_chain: HashMap<String, usize> = HashMap::new();
        let mut with_tvl = 0;
        let mut total_tvl = 0.0;
        let mut with_tx_count = 0;

        for contract in contracts {
            *by_chain.entry(contract.chain.clone()).or_insert(0) += 1;
            if let Some(tvl) = contract.tvl {
                with_tvl += 1;
                total_tvl += tvl;
            }
            if contract.transaction_count.is_some() {
                with_tx_count += 1;
            }
        }

        tracing::info!("========================================");
        tracing::info!("Scraping Summary:");
        tracing::info!("========================================");
        tracing::info!("Total contracts scraped: {}", contracts.len());
        tracing::info!("");
        tracing::info!("By chain:");
        let mut chain_vec: Vec<_> = by_chain.iter().collect();
        chain_vec.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        for (chain, count) in chain_vec {
            tracing::info!("  {}: {} contracts", chain, count);
        }
        tracing::info!("");
        tracing::info!("Metadata coverage:");
        tracing::info!("  Contracts with TVL: {}/{}", with_tvl, contracts.len());
        if with_tvl > 0 {
            tracing::info!("  Total TVL: ${:.2}M", total_tvl / 1_000_000.0);
        }
        tracing::info!("  Contracts with transaction counts: {}/{}", with_tx_count, contracts.len());
        tracing::info!("");

        if !contracts.is_empty() {
            tracing::info!("Sample contracts:");
            for (i, contract) in contracts.iter().take(5).enumerate() {
                let tvl_str = contract.tvl
                    .map(|t| format!("${:.2}M", t / 1_000_000.0))
                    .unwrap_or_else(|| "N/A".to_string());
                tracing::info!(
                    "  {}. {} ({}) - {} - TVL: {}",
                    i + 1,
                    contract.name,
                    contract.symbol.as_deref().unwrap_or("N/A"),
                    contract.chain,
                    tvl_str
                );
            }
        }
        tracing::info!("========================================");
    }

    /// Try to get contract address from CoinGecko as fallback
    async fn try_coingecko_address(&self, protocol_name: &str, chain: &str) -> Result<String> {
        tracing::debug!("Trying CoinGecko fallback for {} on {}", protocol_name, chain);

        // Get all coins from CoinGecko
        let coins = self.coingecko.get_coins_list().await?;

        // Try exact match first
        let coin = coins.iter()
            .find(|c| c.name.to_lowercase() == protocol_name.to_lowercase())
            .or_else(|| {
                // Try partial match - protocol name contains coin name or vice versa
                coins.iter().find(|c| {
                    let coin_name = c.name.to_lowercase();
                    let proto_name = protocol_name.to_lowercase();

                    // Remove common suffixes for better matching
                    let proto_clean = proto_name
                        .replace(" v3", "")
                        .replace(" v2", "")
                        .replace(" v1", "")
                        .replace(" stake", "")
                        .trim()
                        .to_string();

                    coin_name == proto_clean ||
                    coin_name.contains(&proto_clean) ||
                    proto_clean.contains(&coin_name)
                })
            })
            .ok_or_else(|| anyhow::anyhow!("No CoinGecko coin found for {}", protocol_name))?;

        tracing::debug!("Found CoinGecko coin: {} for protocol {}", coin.name, protocol_name);

        // Normalize chain name for CoinGecko
        let normalized_chain = crate::clients::coingecko::CoinGeckoClient::normalize_chain_name(chain)
            .ok_or_else(|| anyhow::anyhow!("Unsupported chain: {}", chain))?;

        // Extract platform address for the chain
        let address = crate::clients::coingecko::CoinGeckoClient::get_contract_address(coin, &normalized_chain)
            .ok_or_else(|| anyhow::anyhow!("No {} address in CoinGecko for {}", chain, protocol_name))?;

        tracing::info!("✓ Found address from CoinGecko: {} for {}", address, protocol_name);
        Ok(address)
    }

    /// Clean address by removing chain prefix (e.g., "arbitrum:0x123..." -> "0x123...")
    fn clean_address(address: &str) -> String {
        address.split(':').last().unwrap_or(address).to_string()
    }
}
