use anyhow::Result;
use clap::{Parser, Subcommand};

mod clients;
mod config;
mod db;
mod models;
mod scraper;

use clients::{CoinGeckoClient, DefiLlamaClient, EtherscanClient};
use config::Config;
use db::ContractStore;
use scraper::ContractScraper;

#[derive(Parser)]
#[command(name = "contract-scraper")]
#[command(about = "CLI tool to scrape and maintain top DeFi contracts", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scrape contracts from all sources
    Scrape {
        #[arg(short, long, default_value = "100")]
        limit: usize,

        #[arg(short, long, value_delimiter = ',')]
        chains: Vec<String>,
    },

    /// Update existing contracts
    Update {
        #[arg(short, long)]
        chain_id: Option<i32>,
    },

    /// Verify contract data
    Verify {
        #[arg(short, long)]
        address: String,

        #[arg(short, long)]
        chain_id: i32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Load configuration
    tracing::info!("Loading configuration...");
    let config = Config::from_env()?;
    config.validate()?;

    // Connect to database
    tracing::info!("Connecting to database...");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    tracing::info!("✓ Database connected");

    // Initialize clients
    let defillama = DefiLlamaClient::new();
    let coingecko = CoinGeckoClient::new(config.coingecko_api_key.clone());
    let etherscan = EtherscanClient::new(config.etherscan_api_key);
    let db = ContractStore::new(pool);

    // Create scraper
    let scraper = ContractScraper::new(defillama, coingecko, etherscan, db);

    match cli.command {
        Commands::Scrape { limit, chains } => {
            tracing::info!("Starting scrape with limit={} chains={:?}", limit, chains);

            // Scrape contracts
            let contracts = scraper.scrape_top_contracts(limit, &chains).await?;

            tracing::info!("Scraped {} contracts", contracts.len());

            // Save to database
            scraper.save_contracts(&contracts).await?;

            tracing::info!("✓ Scraping complete!");
        }

        Commands::Update { chain_id } => {
            tracing::info!("Updating existing contracts");
            if let Some(id) = chain_id {
                tracing::info!("Filtering by chain_id={}", id);
            }

            scraper.update_existing_contracts(chain_id).await?;

            tracing::info!("✓ Update complete!");
        }

        Commands::Verify { address, chain_id } => {
            tracing::info!("Verifying contract {} on chain {}", address, chain_id);

            scraper.verify_contract(chain_id, &address).await?;

            tracing::info!("✓ Verification complete!");
        }
    }

    Ok(())
}
