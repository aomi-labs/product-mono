use anyhow::{Result, anyhow};

use super::types::*;
use baml_client::apis::{
    configuration::{ApiKey, Configuration},
    default_api,
};

/// BAML client wrapper for forge executor operations
pub struct BamlClient {
    config: Configuration,
}

impl BamlClient {
    /// Create a new BAML client
    ///
    /// Requires `ANTHROPIC_API_KEY` environment variable to be set.
    /// Optionally uses `BAML_API_URL` for custom BAML server (defaults to http://localhost:2024)
    pub fn new() -> Result<Self> {
        let mut config = Configuration::new();

        // Set API key from environment
        config.api_key = Some(ApiKey {
            prefix: None,
            key: std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| anyhow!("ANTHROPIC_API_KEY environment variable not set"))?,
        });

        // Override base_path if BAML_API_URL is set
        if let Ok(url) = std::env::var("BAML_API_URL") {
            config.base_path = url;
        }

        Ok(Self { config })
    }

    /// Phase 1: Extract relevant contract information from full ABIs and source code
    ///
    /// Takes natural language operations and full contract details, returns extracted
    /// interface definitions with only the relevant functions, storage, and events.
    pub async fn extract_contract_info(
        &self,
        operations: &[String],
        contracts: &[ContractSource],
    ) -> Result<Vec<ExtractedContractInfo>> {
        // Convert to BAML types
        let baml_contracts: Vec<ContractInfo> = contracts.iter().map(ContractInfo::from).collect();

        let request = ExtractContractInfoRequest::new(baml_contracts, operations.to_vec());

        // Call BAML Phase 1
        let result = default_api::extract_contract_info(&self.config, request)
            .await
            .map_err(|e| anyhow!("BAML Phase 1 (ExtractContractInfo) failed: {:?}", e))?;

        Ok(result)
    }

    /// Phase 2: Generate Forge script from operations and extracted contract info
    ///
    /// Takes natural language operations and extracted contract information, returns
    /// a ScriptBlock with code lines and import/interface specifications.
    pub async fn generate_script(
        &self,
        operations: &[String],
        extracted_infos: &[ExtractedContractInfo],
    ) -> Result<ScriptBlock> {
        let request = GenerateScriptRequest::new(extracted_infos.to_vec(), operations.to_vec());

        // Call BAML Phase 2
        let result = default_api::generate_script(&self.config, request)
            .await
            .map_err(|e| anyhow!("BAML Phase 2 (GenerateScript) failed: {:?}", e))?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip_without_anthropic_api_key() -> bool {
        std::env::var("ANTHROPIC_API_KEY").is_err()
    }

    #[tokio::test]
    async fn test_client_creation() {
        if skip_without_anthropic_api_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }

        let client = BamlClient::new();
        assert!(client.is_ok(), "Should create BAML client successfully");
    }

    #[tokio::test]
    async fn test_extract_contract_info() {
        if skip_without_anthropic_api_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }

        let client = BamlClient::new().expect("Failed to create client");

        let operations = vec!["wrap 0.75 ETH to WETH by calling wrap() function".to_string()];

        let contracts = vec![ContractSource {
            chain_id: "1".to_string(),
            address: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
            name: "WETH".to_string(),
            abi: r#"[{"name":"wrap","type":"function","stateMutability":"payable","inputs":[],"outputs":[]}]"#.to_string(),
            source_code: Some("contract WETH { function wrap() external payable { balances[msg.sender] += msg.value; } }".to_string()),
        }];

        let result = client.extract_contract_info(&operations, &contracts).await;
        assert!(result.is_ok(), "Phase 1 should succeed");

        let extracted = result.unwrap();
        assert!(
            !extracted.is_empty(),
            "Should extract at least one contract"
        );
        assert_eq!(
            extracted[0].address,
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        );
    }
}
