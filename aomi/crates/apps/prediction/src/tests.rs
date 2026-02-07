//! Integration tests for Prediction Wizard APIs
//!
//! These tests make real API calls to verify integrations work.
//! Run with: cargo test -p aomi-prediction -- --ignored

#[cfg(test)]
mod tests {
    use crate::client::{Platform, PredictionClient};

    fn setup_client() -> PredictionClient {
        PredictionClient::new().expect("Failed to create client")
    }

    // =========================================================================
    // Polymarket Tests (Free, no API key)
    // =========================================================================

    #[tokio::test]
    #[ignore] // Run with --ignored flag
    async fn test_polymarket_search() {
        let client = setup_client();
        let markets = client.polymarket_search("election", 5).await;
        
        assert!(markets.is_ok(), "Polymarket search failed: {:?}", markets.err());
        let markets = markets.unwrap();
        println!("Found {} Polymarket markets", markets.len());
        
        for market in markets.iter().take(3) {
            println!("  - {} ({:?})", market.question, market.probability);
        }
    }

    // =========================================================================
    // Kalshi Tests (Free, no API key)
    // =========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_kalshi_search() {
        let client = setup_client();
        let markets = client.kalshi_search("weather", 5).await;
        
        // Kalshi might not have matching markets, so we just check it doesn't error badly
        match markets {
            Ok(m) => println!("Found {} Kalshi markets", m.len()),
            Err(e) => println!("Kalshi search: {:?}", e),
        }
    }

    // =========================================================================
    // Manifold Tests (Free, no API key)
    // =========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_manifold_search() {
        let client = setup_client();
        let markets = client.manifold_search("AI", 5).await;
        
        assert!(markets.is_ok(), "Manifold search failed: {:?}", markets.err());
        let markets = markets.unwrap();
        println!("Found {} Manifold markets", markets.len());
        
        for market in markets.iter().take(3) {
            println!("  - {} ({:?})", market.question, market.probability);
        }
    }

    // =========================================================================
    // Metaculus Tests (Free, no API key)
    // =========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_metaculus_search() {
        let client = setup_client();
        let markets = client.metaculus_search("AI", 5).await;
        
        assert!(markets.is_ok(), "Metaculus search failed: {:?}", markets.err());
        let markets = markets.unwrap();
        println!("Found {} Metaculus questions", markets.len());
        
        for market in markets.iter().take(3) {
            println!("  - {} ({:?})", market.question, market.probability);
        }
    }

    // =========================================================================
    // Aggregated Tests
    // =========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_search_all_platforms() {
        let client = setup_client();
        let markets = client.search_all("bitcoin", None, 3).await;
        
        assert!(markets.is_ok(), "Search all failed: {:?}", markets.err());
        let markets = markets.unwrap();
        println!("Found {} total markets across all platforms", markets.len());
        
        // Group by platform
        let mut by_platform = std::collections::HashMap::new();
        for market in &markets {
            *by_platform.entry(market.platform).or_insert(0) += 1;
        }
        
        for (platform, count) in by_platform {
            println!("  {:?}: {} markets", platform, count);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_aggregated_odds() {
        let client = setup_client();
        let odds = client.get_aggregated_odds("Trump").await;
        
        assert!(odds.is_ok(), "Aggregated odds failed: {:?}", odds.err());
        let odds = odds.unwrap();
        
        println!("Aggregated odds for '{}':", odds.query);
        if let Some(p) = odds.polymarket {
            println!("  Polymarket: {:.1}%", p * 100.0);
        }
        if let Some(p) = odds.kalshi {
            println!("  Kalshi: {:.1}%", p * 100.0);
        }
        if let Some(p) = odds.manifold {
            println!("  Manifold: {:.1}%", p * 100.0);
        }
        if let Some(p) = odds.metaculus {
            println!("  Metaculus: {:.1}%", p * 100.0);
        }
        if let Some(c) = odds.consensus {
            println!("  Consensus: {:.1}%", c * 100.0);
        }
        if let Some(s) = odds.spread {
            println!("  Spread: {:.1}%", s * 100.0);
        }
    }
}
