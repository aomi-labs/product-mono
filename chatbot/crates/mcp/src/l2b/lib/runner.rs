use alloy_primitives::Address;
use alloy_provider::{RootProvider};
use anyhow::{Result, anyhow};
use std::collections::HashMap;

use baml_client::apis::configuration::{ApiKey, Configuration};
use baml_client::apis::default_api::analyze_contract_for_handlers;
use baml_client::models::{
    AnalyzeContractForHandlersRequest, ContractAnalysis,
};

use super::adapter::{
    etherscan_to_contract_info, to_handler_definition,
};
use super::etherscan::{EtherscanClient, Network};
use super::handlers::access_control::AccessControlHandler;
use super::handlers::array::ArrayHandler;
use super::handlers::call::CallHandler;
use super::handlers::config::HandlerDefinition;
use super::handlers::event::EventHandler;
use super::handlers::storage::StorageHandler;
use super::handlers::types::{Handler, HandlerResult};


/// Discovery runner that orchestrates the full contract analysis pipeline
pub struct DiscoveryRunner<N: alloy_provider::network::Network> {
    baml_config: Configuration,
    etherscan_client: EtherscanClient,
    provider: RootProvider<N>,
}

impl<N: alloy_provider::network::Network> DiscoveryRunner<N> {
    /// Create a new DiscoveryRunner
    pub fn new(etherscan_network: Network, provider: RootProvider<N>) -> Result<Self> {
        let mut baml_config = Configuration::new();
        baml_config.api_key = Some(ApiKey {
            prefix: None,
            key: std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| anyhow!("ANTHROPIC_API_KEY environment variable not set"))?,
        });

        let etherscan_client = EtherscanClient::new(etherscan_network)?;

        Ok(Self {
            baml_config,
            etherscan_client,
            provider,
        })
    }    

    /// Execute a single handler definition
    pub async fn execute_handler(
        &self,
        field_name: String,
        handler_def: HandlerDefinition,
        contract_address: &Address,
        previous_results: &HashMap<String, HandlerResult>,
    ) -> Result<HandlerResult> {
        match handler_def {
            HandlerDefinition::Call { .. } => {
                let handler = CallHandler::<N>::from_handler_definition(field_name, handler_def)
                    .map_err(|e| anyhow!("Failed to create CallHandler: {}", e))?;
                Ok(handler
                    .execute(&self.provider, contract_address, previous_results)
                    .await)
            }
            HandlerDefinition::Storage { .. } => {
                let handler = StorageHandler::<N>::from_handler_definition(field_name, handler_def)
                    .map_err(|e| anyhow!("Failed to create StorageHandler: {}", e))?;
                Ok(handler
                    .execute(&self.provider, contract_address, previous_results)
                    .await)
            }
            HandlerDefinition::DynamicArray { .. } => {
                let handler = ArrayHandler::<N>::from_handler_definition(field_name, handler_def)
                    .map_err(|e| anyhow!("Failed to create ArrayHandler: {}", e))?;
                Ok(handler
                    .execute(&self.provider, contract_address, previous_results)
                    .await)
            }
            HandlerDefinition::AccessControl { .. } => {
                let handler =
                    AccessControlHandler::<N>::from_handler_definition(field_name, handler_def)
                        .map_err(|e| anyhow!("Failed to create AccessControlHandler: {}", e))?;
                Ok(handler
                    .execute(&self.provider, contract_address, previous_results)
                    .await)
            }
            HandlerDefinition::Event { .. } => {
                let handler = EventHandler::<N>::from_handler_definition(field_name, handler_def)
                    .map_err(|e| anyhow!("Failed to create EventHandler: {}", e))?;
                Ok(handler
                    .execute(&self.provider, contract_address, previous_results)
                    .await)
            }
            _ => Err(anyhow!("Unsupported handler type")),
        }
    }

    #[allow(dead_code)]
    pub async fn generate_handler_configs(
        &self,
        address: &str,
        intent: &str,
    ) -> Result<(ContractAnalysis, Vec<(String, HandlerDefinition)>)> {
        let mut handler_definitions = Vec::new();

        // Step 1: Fetch contract data from Etherscan
        let etherscan_results = self.etherscan_client.fetch_contract_data(address).await?;

        // Step 2: Convert to ContractInfo for BAML
        let contract_info = etherscan_to_contract_info(
            etherscan_results.clone(),
            Some(format!("Contract at {}", address)),
        )
        .map_err(|e| anyhow!("Failed to convert Etherscan data to ContractInfo: {}", e))?;

        // Step 3: Analyze ABI if available
        if contract_info.abi.is_some() && contract_info.source_code.is_some() {
            let abi_request =
                AnalyzeContractForHandlersRequest::new(contract_info.clone(), intent.to_string());

            let contract_analysis = analyze_contract_for_handlers(&self.baml_config, abi_request)
                .await
                .map_err(|e| anyhow!("Failed to analyze ABI: {:?}", e))?;

            println!(
                "   ├─ AI identified {} Handler(s)",
                contract_analysis.handlers.len()
            );

            // Step 4: Convert ABI analysis to Call handlers
            handler_definitions = to_handler_definition(contract_analysis.clone())?;
            return Ok((contract_analysis, handler_definitions));
        }

        Err(anyhow::anyhow!("error getting contract info"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_provider::network::AnyNetwork;

    #[test]
    fn test_runner_creation() {
        let provider =
            RootProvider::<AnyNetwork>::new_http("http://localhost:8545".parse().unwrap());

        let runner = DiscoveryRunner::new(Network::Mainnet, provider);

        assert!(runner.is_ok(), "Should create runner successfully");
    }    
    
    #[tokio::test]
    async fn test_generate_handler_configs() {
        // Test generate_handler_configs with USDC proxy
        let usdc_address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";

        let provider =
            RootProvider::<AnyNetwork>::new_http("http://localhost:8545".parse().unwrap());

        let runner = DiscoveryRunner::new(Network::Mainnet, provider)
            .expect("Failed to create DiscoveryRunner");

        let intent = "Identify how to retrieve the implementation address and admin address";

        println!("\n╔═══════════════════════════════════════════════════════════╗");
        println!("║          Testing generate_handler_configs                ║");
        println!("╚═══════════════════════════════════════════════════════════╝");
        println!("\n📍 Contract: {}", usdc_address);
        println!("💭 Intent: {}\n", intent);

        let (_analysis, handler_configs) = runner
            .generate_handler_configs(usdc_address, intent)
            .await
            .expect("Failed to generate handler configs");

        println!(
            "📋 Generated {} handler configurations:\n",
            handler_configs.len()
        );

        for (field_name, handler_def) in &handler_configs {
            println!("┌─ Field: {} ────────────────", field_name);

            // Serialize the handler definition to pretty JSON
            let json_str = serde_json::to_string_pretty(&handler_def)
                .expect("Failed to serialize handler definition");

            // Print each line with proper indentation
            for line in json_str.lines() {
                println!("│ {}", line);
            }

            println!("└────────────────────────────────────────────────────────\n");
        }

        // Verify we got some handler configs
        assert!(
            !handler_configs.is_empty(),
            "Expected at least one handler config"
        );

        // Should find implementation and admin configs
        let has_implementation = handler_configs
            .iter()
            .any(|(name, _)| name.to_lowercase().contains("implementation"));
        let has_admin = handler_configs
            .iter()
            .any(|(name, _)| name.to_lowercase().contains("admin"));

        println!("✅ Found implementation config: {}", has_implementation);
        println!("✅ Found admin config: {}", has_admin);

        println!("\n✓ Test passed: Successfully generated handler configs\n");
    }
}
