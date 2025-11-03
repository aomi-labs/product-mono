use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use std::env;

/// Etherscan API client
#[derive(Debug, Clone)]
pub struct EtherscanClient {
    client: Client,
    api_key: String,
    network: Network,
}

/// Response from Etherscan API
#[derive(Debug, Deserialize)]
struct EtherscanResponse {
    status: String,
    message: String,
    result: serde_json::Value,
}

/// Results from fetching contract data from Etherscan
#[derive(Debug, Clone)]
pub struct EtherscanResults {
    pub address: String,
    pub abi: Option<serde_json::Value>,
    pub source_code: Option<serde_json::Value>,
}

/// Supported Etherscan networks
#[derive(Debug, Clone, Copy)]
pub enum Network {
    Mainnet,
    Goerli,
    Sepolia,
    Polygon,
    Arbitrum,
    Optimism,
    Base,
}

impl Network {
    /// Get the API URL for this network (V2 unified endpoint)
    fn base_url(&self) -> &'static str {
        "https://api.etherscan.io/v2/api"
    }

    /// Get chain id for this network (required for V2 API)
    fn chain_id(&self) -> &'static str {
        match self {
            Network::Mainnet => "1",
            Network::Goerli => "5",
            Network::Sepolia => "11155111",
            Network::Polygon => "137",
            Network::Arbitrum => "42161",
            Network::Optimism => "10",
            Network::Base => "8453",
        }
    }
}

impl EtherscanClient {
    /// Create a new EtherscanClient
    pub fn new(network: Network) -> Result<Self> {
        let api_key = env::var("ETHERSCAN_API_KEY")
            .map_err(|_| anyhow!("ETHERSCAN_API_KEY environment variable not set"))?;

        Ok(Self {
            client: Client::new(),
            api_key,
            network,
        })
    }

    /// Fetch the ABI for a given contract address
    pub async fn fetch_abi(&self, address: &str) -> Result<serde_json::Value> {
        let url = format!(
            "{}?chainid={}&module=contract&action=getabi&address={}&apikey={}",
            self.network.base_url(),
            self.network.chain_id(),
            address,
            self.api_key
        );

        let resp: EtherscanResponse = self.client.get(&url).send().await?.json().await?;

        if resp.status == "1" {
            Ok(resp.result)
        } else {
            Err(anyhow!(
                "Failed to fetch ABI: {} (result: {})",
                resp.message,
                resp.result
            ))
        }
    }

    /// Fetch the source code for a given contract address
    pub async fn fetch_source_code(&self, address: &str) -> Result<serde_json::Value> {
        let url = format!(
            "{}?chainid={}&module=contract&action=getsourcecode&address={}&apikey={}",
            self.network.base_url(),
            self.network.chain_id(),
            address,
            self.api_key
        );

        let resp: EtherscanResponse = self.client.get(&url).send().await?.json().await?;
        if resp.status == "1" {
            Ok(resp.result)
        } else {
            Err(anyhow!(
                "Failed to fetch source code: {} (result: {})",
                resp.message,
                resp.result
            ))
        }
    }

    /// Fetch both ABI and source code for a contract
    pub async fn fetch_contract_data(&self, address: &str) -> Result<EtherscanResults> {
        let abi = self.fetch_abi(address).await.ok();
        let source_code = self.fetch_source_code(address).await.ok();

        Ok(EtherscanResults {
            address: address.to_string(),
            abi,
            source_code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_abi_real() {
        // This test requires a real ETHERSCAN_API_KEY environment variable
        let client = EtherscanClient::new(Network::Mainnet).expect("ETHERSCAN_API_KEY not set");

        // Test with USDC proxy contract (verified on Etherscan)
        let address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let result = client.fetch_abi(address).await;

        match result {
            Ok(abi) => {
                // ABI should be a string containing JSON array
                assert!(abi.is_string() || abi.is_array());
                println!("✓ Successfully fetched ABI for USDC proxy");
            }
            Err(e) => {
                panic!(
                    "Failed to fetch ABI: {}. Make sure ETHERSCAN_API_KEY is valid.",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_fetch_source_code_real() {
        let client = EtherscanClient::new(Network::Mainnet).expect("ETHERSCAN_API_KEY not set");

        // Test with USDC implementation contract (verified on Etherscan)
        let address = "0x0882477e7895bdC5cea7cB1552ed914aB157Fe56";
        let result = client.fetch_source_code(address).await;

        match result {
            Ok(source) => {
                // Source code result should be an array with contract info
                assert!(source.is_array());
                if let Some(contracts) = source.as_array() {
                    assert!(!contracts.is_empty(), "Expected at least one contract");
                    println!("✓ Successfully fetched source code for USDC implementation");
                }
            }
            Err(e) => {
                panic!(
                    "Failed to fetch source code: {}. Make sure ETHERSCAN_API_KEY is valid.",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_unverified_contract() {
        let client = EtherscanClient::new(Network::Mainnet).expect("ETHERSCAN_API_KEY not set");

        // Use a random address that's unlikely to be verified
        let address = "0x0000000000000000000000000000000000000001";
        let result = client.fetch_source_code(address).await;

        // Should either succeed with empty result or fail with appropriate message
        match result {
            Ok(_) => println!("Got response for unverified contract (may be empty)"),
            Err(e) => println!("Expected error for unverified contract: {}", e),
        }
    }

    #[test]
    fn test_response_parsing() {
        // Test that we correctly parse Etherscan response format
        let success_json = r#"{
            "status": "1",
            "message": "OK",
            "result": "[{\"type\":\"function\",\"name\":\"test\"}]"
        }"#;

        let response: EtherscanResponse = serde_json::from_str(success_json).unwrap();
        assert_eq!(response.status, "1");
        assert_eq!(response.message, "OK");
        assert!(response.result.is_string());

        let error_json = r#"{
            "status": "0",
            "message": "NOTOK",
            "result": "Contract source code not verified"
        }"#;

        let error_response: EtherscanResponse = serde_json::from_str(error_json).unwrap();
        assert_eq!(error_response.status, "0");
        assert_eq!(error_response.message, "NOTOK");
    }

    #[test]
    fn test_source_code_response_format() {
        // Test parsing of actual Etherscan source code response
        let source_response = r#"{
            "status": "1",
            "message": "OK",
            "result": [{
                "SourceCode": "contract MyContract { }",
                "ABI": "[{\"type\":\"constructor\"}]",
                "ContractName": "MyContract",
                "CompilerVersion": "v0.8.0+commit.c7dfd78e",
                "OptimizationUsed": "1",
                "Runs": "200",
                "ConstructorArguments": "",
                "EVMVersion": "Default",
                "Library": "",
                "LicenseType": "MIT",
                "Proxy": "0",
                "Implementation": "",
                "SwarmSource": ""
            }]
        }"#;

        let response: EtherscanResponse = serde_json::from_str(source_response).unwrap();
        assert_eq!(response.status, "1");
        assert!(response.result.is_array());
    }

    #[tokio::test]
    async fn test_e2e_usdc_proxy() {
        use crate::l2b::lib::adapter::etherscan_to_contract_info;

        // USDC Proxy contract on Ethereum mainnet
        let usdc_proxy_address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";

        let client = EtherscanClient::new(Network::Mainnet)
            .expect("ETHERSCAN_API_KEY not set. Set it to run this E2E test.");

        // Fetch contract data from Etherscan
        let etherscan_results = client
            .fetch_contract_data(usdc_proxy_address)
            .await
            .expect("Failed to fetch USDC proxy data from Etherscan");

        // Verify we got both ABI and source code
        assert!(
            etherscan_results.abi.is_some(),
            "Expected ABI for USDC proxy"
        );
        assert!(
            etherscan_results.source_code.is_some(),
            "Expected source code for USDC proxy"
        );

        println!("✓ Successfully fetched USDC proxy data from Etherscan");

        // Convert to ContractInfo for BAML processing
        let contract_info =
            etherscan_to_contract_info(etherscan_results.clone(), Some("USDC Proxy".to_string()))
                .expect("Failed to convert to ContractInfo");

        // Verify ContractInfo structure
        assert_eq!(contract_info.address, Some(usdc_proxy_address.to_string()));
        assert_eq!(contract_info.description, Some("USDC Proxy".to_string()));
        assert!(contract_info.abi.is_some(), "ContractInfo should have ABI");
        assert!(
            contract_info.source_code.is_some(),
            "ContractInfo should have source code"
        );

        println!("✓ Successfully converted to ContractInfo");
        println!("  Address: {:?}", contract_info.address);
        println!("  Description: {:?}", contract_info.description);
        println!("  Has ABI: {}", contract_info.abi.is_some());
        println!("  Has Source: {}", contract_info.source_code.is_some());

        // Verify ABI contains expected functions (USDC has standard ERC20 functions)
        if let Some(abi_str) = contract_info.abi {
            assert!(
                abi_str.contains("totalSupply") || abi_str.contains("balanceOf"),
                "Expected standard ERC20 functions in ABI"
            );
            println!("✓ ABI contains expected ERC20 functions");
        }

        println!(
            "\n✓ E2E test passed: USDC proxy → Etherscan → ContractInfo pipeline works correctly"
        );
    }
}
