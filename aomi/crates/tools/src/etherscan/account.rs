use super::{EtherscanResponse, Transaction};
use anyhow::{Context, Result};

/// Fetches transaction history for an address from Etherscan API
/// API key is read from ETHERSCAN_API_KEY environment variable
///
/// Returns up to 1000 most recent transactions (Etherscan API limit per request)
pub async fn fetch_transaction_history(address: String, chainid: u32) -> Result<Vec<Transaction>> {
    let api_key = std::env::var("ETHERSCAN_API_KEY")
        .context("ETHERSCAN_API_KEY environment variable not set")?;

    // Validate address format
    if !address.starts_with("0x") || address.len() != 42 {
        anyhow::bail!("Invalid address format. Must be a 42-character hex string starting with 0x");
    }

    let client = reqwest::Client::new();
    let response = client
        .get("https://api.etherscan.io/v2/api")
        .query(&[
            ("chainid", chainid.to_string().as_str()),
            ("module", "account"),
            ("action", "txlist"),
            ("address", address.as_str()),
            ("startblock", "0"),
            ("endblock", "latest"),
            ("page", "1"),
            ("offset", "1000"),
            ("sort", "desc"),
            ("apikey", api_key.as_str()),
        ])
        .send()
        .await
        .context("Failed to send request to Etherscan")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Etherscan API request failed with status: {}",
            response.status()
        );
    }

    let tx_response: EtherscanResponse<Vec<Transaction>> = response
        .json()
        .await
        .context("Failed to parse Etherscan transaction response")?;

    if tx_response.status != "1" {
        anyhow::bail!("Etherscan API error: {}", tx_response.message);
    }

    Ok(tx_response.result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_fetch_transaction_history() -> Result<()> {
        use super::super::ETHEREUM_MAINNET;

        // Test with Vitalik's address on Ethereum mainnet
        let transactions = fetch_transaction_history(
            "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            ETHEREUM_MAINNET,
        )
        .await?;

        assert!(!transactions.is_empty());

        // Verify first transaction has expected fields
        let first_tx = &transactions[0];
        assert!(!first_tx.hash.is_empty());
        assert!(!first_tx.from.is_empty());
        assert!(!first_tx.block_number.is_empty());

        println!("Fetched {} transactions", transactions.len());
        Ok(())
    }
}
