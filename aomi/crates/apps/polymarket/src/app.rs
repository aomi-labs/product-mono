use crate::tools::{GetMarketDetails, GetMarkets, GetTrades, PlacePolymarketOrder};
use aomi_core::{
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
    prompts::{PreambleBuilder, PromptSection},
    BuildOpts, CoreApp, CoreAppBuilder,
};
use async_trait::async_trait;
use eyre::Result;

// Type alias for PolymarketCommand with our specific ToolReturn type
pub type PolymarketCommand = CoreCommand;

const POLYMARKET_ROLE: &str = "You are an AI assistant specialized in Polymarket prediction markets analysis. You help users discover markets, analyze trends, and understand prediction market dynamics. Keep responses clear and data-driven.";

const POLYMARKET_CAPABILITIES: &[&str] = &[
    "Query live and historical market data with filtering by status, category, and tags",
    "Analyze market trends, trading volumes, and liquidity metrics",
    "Retrieve detailed trade history with price and size information",
    "Identify trading opportunities based on market sentiment and probability shifts",
    "Monitor specific markets across different event categories",
    "Place orders on Polymarket using EIP-712 typed data signatures via the connected wallet",
];

const POPULAR_TAGS: &[&str] = &[
    "Politics & Elections: election 2026, elections 2026, Presidential Debate, donald trump, kamala harris, electoral votes",
    "Crypto & Web3: Bitcoin Conference, Stablecoins, DJT, blast, celestia, eigenlayer",
    "Sports: EPL (English Premier League), MLS Cup, NCAA, CFB (College Football), Cricket, Wimbledon",
    "International: European Union, Euros, ukraine, russia, china, azerbaijan",
    "Economics: stock market, banks, crude oil, recession, gdp",
    "Technology: ai technology, anthropic",
    "Culture: Kai Cenat, dating, Wimbledon",
];

const POLYMARKET_CONTEXT: &[&str] = &[
    "Polymarket is a prediction market platform where users trade on real-world event outcomes",
    "Market prices represent collective probability assessments (e.g., 0.65 = 65% probability)",
    "Higher volume and liquidity indicate more active and reliable markets",
    "Markets resolve to 'Yes' (1.00) or 'No' (0.00) based on specified resolution criteria",
];

const ORDER_PLACEMENT_WORKFLOW: &[&str] = &[
    "1. Use GetMarketDetails to get the token_id for the outcome you want to trade",
    "2. Call PlacePolymarketOrder with the order parameters (token_id, side, price, size)",
    "3. The tool will format the EIP-712 typed data and use SendTransactionToWallet to request a signature from the connected wallet",
    "4. Once the user signs in their wallet, the signed order is submitted to the Polymarket CLOB",
    "5. The order confirmation with order_id is returned upon successful placement",
];

const EXECUTION_GUIDELINES: &[&str] = &[
    "Use GetMarkets to discover markets by category, status (active/closed), or tags",
    "Use GetMarketDetails to analyze a specific market's prices, volume, and outcomes",
    "Use GetTrades to examine trading patterns, user activity, and historical price movements",
    "Filter by tags to find niche markets (e.g., 'crypto', 'election 2026', 'Wimbledon')",
    "Use PlacePolymarketOrder to submit signed orders to the Polymarket CLOB",
    "For order placement: use SendTransactionToWallet with EIP-712 typed data to request wallet signature, then submit the signed order",
];

fn polymarket_preamble() -> String {
    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(POLYMARKET_ROLE))
        .section(
            PromptSection::titled("Your Capabilities")
                .bullet_list(POLYMARKET_CAPABILITIES.iter().copied()),
        )
        .section(
            PromptSection::titled("Popular Tags for Filtering")
                .bullet_list(POPULAR_TAGS.iter().copied()),
        )
        .section(
            PromptSection::titled("Understanding Polymarket")
                .bullet_list(POLYMARKET_CONTEXT.iter().copied()),
        )
        .section(
            PromptSection::titled("Execution Guidelines")
                .bullet_list(EXECUTION_GUIDELINES.iter().copied()),
        )
        .section(
            PromptSection::titled("Order Placement Workflow")
                .bullet_list(ORDER_PLACEMENT_WORKFLOW.iter().copied()),
        )
        .section(
            PromptSection::titled("Account Context")
                .paragraph(aomi_core::generate_account_context()),
        )
        .build()
}

pub struct PolymarketApp {
    chat_app: CoreApp,
}

impl PolymarketApp {
    pub async fn default() -> Result<Self> {
        Self::new(BuildOpts::default()).await
    }

    pub async fn new(opts: BuildOpts) -> Result<Self> {
        let mut builder = CoreAppBuilder::new(&polymarket_preamble(), opts, None).await?;

        // Add Polymarket-specific tools
        builder.add_tool(GetMarkets)?;
        builder.add_tool(GetMarketDetails)?;
        builder.add_tool(GetTrades)?;
        builder.add_tool(PlacePolymarketOrder)?;

        // Build the final PolymarketApp
        let chat_app = builder.build(opts, None).await?;

        Ok(Self { chat_app })
    }

    pub async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        tracing::debug!("[polymarket] process message: {}", input);
        self.chat_app.process_message(input, state, ctx).await
    }
}

#[async_trait]
impl AomiApp for PolymarketApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        PolymarketApp::process_message(self, input, state, ctx).await
    }

    fn tool_namespaces(&self) -> std::sync::Arc<std::collections::HashMap<String, String>> {
        self.chat_app.tool_namespaces()
    }
}
