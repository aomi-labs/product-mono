use anyhow::Result;
use aomi_tools::etherscan::{EtherscanClient, Network};
use sqlx::any::AnyPoolOptions;
use tracing::{error, info, warn};

#[derive(Debug)]
struct ContractMetadata {
    address: &'static str,
    chain_id: u32,
    chain: &'static str,
    network: Network,
    name: &'static str,
    symbol: Option<&'static str>,
    protocol: &'static str,
    contract_type: &'static str,
    version: Option<&'static str>,
    tags: &'static str,
    is_proxy: bool,
}

const CONTRACTS: &[ContractMetadata] = &[
    // Ethereum Mainnet
    ContractMetadata {
        address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        chain_id: 1,
        chain: "ethereum",
        network: Network::Mainnet,
        name: "USD Coin",
        symbol: Some("USDC"),
        protocol: "Circle",
        contract_type: "ERC20",
        version: None,
        tags: "token,stablecoin,erc20",
        is_proxy: true,
    },
    ContractMetadata {
        address: "0x6B175474E89094C44Da98b954EedeAC495271d0F",
        chain_id: 1,
        chain: "ethereum",
        network: Network::Mainnet,
        name: "Dai Stablecoin",
        symbol: Some("DAI"),
        protocol: "MakerDAO",
        contract_type: "ERC20",
        version: None,
        tags: "token,stablecoin,erc20",
        is_proxy: false,
    },
    ContractMetadata {
        address: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
        chain_id: 1,
        chain: "ethereum",
        network: Network::Mainnet,
        name: "Uniswap V2 Router 02",
        symbol: None,
        protocol: "Uniswap",
        contract_type: "UniswapV2Router",
        version: Some("v2"),
        tags: "dex,router,swap,amm",
        is_proxy: false,
    },
    ContractMetadata {
        address: "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc",
        chain_id: 1,
        chain: "ethereum",
        network: Network::Mainnet,
        name: "Uniswap V2: USDC-WETH",
        symbol: Some("UNI-V2"),
        protocol: "Uniswap",
        contract_type: "UniswapV2Pair",
        version: Some("v2"),
        tags: "dex,amm,pair,liquidity",
        is_proxy: false,
    },
    ContractMetadata {
        address: "0xE592427A0AEce92De3Edee1F18E0157C05861564",
        chain_id: 1,
        chain: "ethereum",
        network: Network::Mainnet,
        name: "Uniswap V3 SwapRouter",
        symbol: None,
        protocol: "Uniswap",
        contract_type: "SwapRouter",
        version: Some("v3"),
        tags: "dex,router,swap,amm",
        is_proxy: false,
    },
    ContractMetadata {
        address: "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2",
        chain_id: 1,
        chain: "ethereum",
        network: Network::Mainnet,
        name: "Aave V3 Pool",
        symbol: None,
        protocol: "Aave",
        contract_type: "LendingPool",
        version: Some("v3"),
        tags: "lending,defi,aave",
        is_proxy: true,
    },
    ContractMetadata {
        address: "0x39AA39c021dfbaE8faC545936693aC917d5E7563",
        chain_id: 1,
        chain: "ethereum",
        network: Network::Mainnet,
        name: "Compound USDC",
        symbol: Some("cUSDC"),
        protocol: "Compound",
        contract_type: "CErc20",
        version: Some("v2"),
        tags: "lending,ctoken,defi,compound",
        is_proxy: false,
    },
    ContractMetadata {
        address: "0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7",
        chain_id: 1,
        chain: "ethereum",
        network: Network::Mainnet,
        name: "Curve 3Pool",
        symbol: Some("3Crv"),
        protocol: "Curve",
        contract_type: "StableSwap",
        version: Some("v1"),
        tags: "dex,stable,swap,curve,liquidity",
        is_proxy: false,
    },
    ContractMetadata {
        address: "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D",
        chain_id: 1,
        chain: "ethereum",
        network: Network::Mainnet,
        name: "Bored Ape Yacht Club",
        symbol: Some("BAYC"),
        protocol: "Yuga Labs",
        contract_type: "ERC721",
        version: None,
        tags: "nft,erc721,collectible",
        is_proxy: false,
    },
    // Arbitrum
    ContractMetadata {
        address: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831",
        chain_id: 42161,
        chain: "arbitrum",
        network: Network::Arbitrum,
        name: "USD Coin",
        symbol: Some("USDC"),
        protocol: "Circle",
        contract_type: "ERC20",
        version: None,
        tags: "token,stablecoin,erc20,bridged",
        is_proxy: false,
    },
    ContractMetadata {
        address: "0xE592427A0AEce92De3Edee1F18E0157C05861564",
        chain_id: 42161,
        chain: "arbitrum",
        network: Network::Arbitrum,
        name: "Uniswap V3 SwapRouter",
        symbol: None,
        protocol: "Uniswap",
        contract_type: "SwapRouter",
        version: Some("v3"),
        tags: "dex,router,swap,amm",
        is_proxy: false,
    },
    // Polygon
    ContractMetadata {
        address: "0x0A6513e40db6EB1b165753AD52E80663aeA50545",
        chain_id: 137,
        chain: "polygon",
        network: Network::Polygon,
        name: "Chainlink USDT/USD Price Feed",
        symbol: None,
        protocol: "Chainlink",
        contract_type: "AggregatorV3",
        version: None,
        tags: "oracle,pricefeed,chainlink",
        is_proxy: true,
    },
    // Optimism
    ContractMetadata {
        address: "0x794a61358D6845594F94dc1DB02A252b5b4814aD",
        chain_id: 10,
        chain: "optimism",
        network: Network::Optimism,
        name: "Aave V3 Pool",
        symbol: None,
        protocol: "Aave",
        contract_type: "LendingPool",
        version: Some("v3"),
        tags: "lending,defi,aave",
        is_proxy: true,
    },
    // Base
    ContractMetadata {
        address: "0x2626664c2603336E57B271c5C0b26F421741e481",
        chain_id: 8453,
        chain: "base",
        network: Network::Base,
        name: "Uniswap V3 SwapRouter",
        symbol: None,
        protocol: "Uniswap",
        contract_type: "SwapRouter",
        version: Some("v3"),
        tags: "dex,router,swap,amm",
        is_proxy: false,
    },
];

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting contract population script");

    // Load environment variables
    dotenv::dotenv().ok();

    // Connect to database
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aomi@localhost:5432/chatbot".to_string());

    info!("Connecting to database: {}", database_url);

    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Initialize Etherscan client
    let etherscan = EtherscanClient::from_env()?;

    info!("Processing {} contracts", CONTRACTS.len());

    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;

    for contract_meta in CONTRACTS {
        info!(
            "Processing: {} ({}) on {}",
            contract_meta.name, contract_meta.address, contract_meta.chain
        );

        // Check if contract already exists
        let check_query =
            "SELECT COUNT(*) as count FROM contracts WHERE chain_id = $1 AND address = $2";
        let count: i64 = sqlx::query_scalar(check_query)
            .bind(contract_meta.chain_id as i32)
            .bind(contract_meta.address.to_lowercase())
            .fetch_one(&pool)
            .await?;

        if count > 0 {
            warn!(
                "Contract already exists, skipping: {}",
                contract_meta.address
            );
            skip_count += 1;
            continue;
        }

        // Fetch from Etherscan
        match etherscan
            .fetch_contract(contract_meta.network, contract_meta.address)
            .await
        {
            Ok(etherscan_contract) => {
                // Insert with metadata
                let query = r#"
                    INSERT INTO contracts (
                        address, chain, chain_id, source_code, abi,
                        name, symbol, protocol, contract_type, version, tags, is_proxy,
                        data_source, created_at, updated_at
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                "#;

                let now = chrono::Utc::now().timestamp();
                let abi_json = serde_json::to_string(&etherscan_contract.abi)?;

                sqlx::query(query)
                    .bind(contract_meta.address.to_lowercase())
                    .bind(contract_meta.chain)
                    .bind(contract_meta.chain_id as i32)
                    .bind(&etherscan_contract.source_code)
                    .bind(&abi_json)
                    .bind(contract_meta.name)
                    .bind(contract_meta.symbol)
                    .bind(contract_meta.protocol)
                    .bind(contract_meta.contract_type)
                    .bind(contract_meta.version)
                    .bind(contract_meta.tags)
                    .bind(contract_meta.is_proxy)
                    .bind("etherscan")
                    .bind(now)
                    .bind(now)
                    .execute(&pool)
                    .await?;

                info!("✅ Successfully added: {}", contract_meta.name);
                success_count += 1;
            }
            Err(e) => {
                error!("❌ Failed to fetch {}: {}", contract_meta.address, e);
                error_count += 1;
            }
        }

        // Rate limiting - be nice to Etherscan (200ms between requests)
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    info!("\n=== Summary ===");
    info!("Total contracts: {}", CONTRACTS.len());
    info!("✅ Successfully added: {}", success_count);
    info!("⏭️  Skipped (already exists): {}", skip_count);
    info!("❌ Errors: {}", error_count);

    Ok(())
}
