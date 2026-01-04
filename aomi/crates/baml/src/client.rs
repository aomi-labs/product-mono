use anyhow::{Result, anyhow};

use crate::baml_client::{async_client::B, types as baml_types};
use crate::types::ContractSource;

/// BAML client wrapper for forge executor operations
///
/// Uses native FFI runtime - no HTTP server needed
pub struct BamlClient;

impl BamlClient {
    /// Create a new BAML client
    ///
    /// Requires `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` environment variable to be set.
    /// No server configuration needed - uses native FFI runtime.
    pub fn new() -> Result<Self> {
        // Runtime auto-initializes with embedded .baml files
        // Just verify API keys are available
        if std::env::var("ANTHROPIC_API_KEY").is_err() && std::env::var("OPENAI_API_KEY").is_err() {
            return Err(anyhow!("Neither ANTHROPIC_API_KEY nor OPENAI_API_KEY environment variable is set"));
        }
        Ok(Self)
    }

    /// Phase 1: Extract relevant contract information from full ABIs and source code
    ///
    /// Takes natural language operations and full contract details, returns extracted
    /// interface definitions with only the relevant functions, storage, and events.
    pub async fn extract_contract_info(
        &self,
        operations: &[String],
        contracts: &[ContractSource],
    ) -> Result<Vec<baml_types::ExtractedContractInfo>> {
        // Convert to BAML types
        let baml_contracts: Vec<baml_types::ContractInfo> = contracts
            .iter()
            .map(|c| baml_types::ContractInfo {
                description: Some(c.name.clone()),
                address: c.address.clone(),
                abi: c.abi.clone(),
                source_code: c.source_code.clone(),
            })
            .collect();

        // Call BAML Phase 1 via native FFI
        B.ExtractContractInfo
            .call(operations, &baml_contracts)
            .await
            .map_err(|e| anyhow!("BAML Phase 1 (ExtractContractInfo) failed: {}", e))
    }

    /// Phase 2: Generate Forge script from operations and extracted contract info
    ///
    /// Takes natural language operations and extracted contract information, returns
    /// a ScriptBlock with code lines and import/interface specifications.
    pub async fn generate_script(
        &self,
        operations: &[String],
        extracted_infos: &[baml_types::ExtractedContractInfo],
    ) -> Result<baml_types::ScriptBlock> {
        // Call BAML Phase 2 via native FFI
        B.GenerateScript
            .call(operations, extracted_infos)
            .await
            .map_err(|e| anyhow!("BAML Phase 2 (GenerateScript) failed: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip_without_api_key() -> bool {
        std::env::var("ANTHROPIC_API_KEY").is_err() && std::env::var("OPENAI_API_KEY").is_err()
    }

    #[tokio::test]
    async fn test_client_creation() {
        if skip_without_api_key() {
            eprintln!("Skipping: No API key set");
            return;
        }

        let client = BamlClient::new();
        assert!(client.is_ok(), "Should create BAML client successfully");
    }

    #[tokio::test]
    #[ignore = "requires baml runtime to be fully initialized"]
    async fn test_extract_contract_info() {
        if skip_without_api_key() {
            eprintln!("Skipping: No API key set");
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
