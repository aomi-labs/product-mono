use std::sync::Arc;

use aomi_chat::{
    ChatApp, ChatAppBuilder,
    app::{ChatCommand, LoadingProgress},
    prompts::{PromptSection, PreambleBuilder},
};
use eyre::Result;
use rig::{agent::Agent, message::Message, providers::anthropic::completion::CompletionModel};
use tokio::sync::{Mutex, mpsc};

use crate::polymarket_tools::{GetMarketDetails, GetMarkets, GetTrades};

// Type alias for PolymarketCommand with our specific ToolResultStream type
pub type PolymarketCommand = ChatCommand;

const POLYMARKET_ROLE: &str = "You are an AI assistant specialized in Polymarket prediction markets analysis. You help users discover markets, analyze trends, and understand prediction market dynamics. Keep responses clear and data-driven.";

const POLYMARKET_CAPABILITIES: &[&str] = &[
    "Query live and historical market data with filtering by status, category, and tags",
    "Analyze market trends, trading volumes, and liquidity metrics",
    "Retrieve detailed trade history with price and size information",
    "Identify trading opportunities based on market sentiment and probability shifts",
    "Monitor specific markets across different event categories",
];

const POPULAR_TAGS: &[&str] = &[
    "Politics & Elections: election 2024, elections 2024, Presidential Debate, donald trump, kamala harris, electoral votes",
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

const EXECUTION_GUIDELINES: &[&str] = &[
    "Use GetMarkets to discover markets by category, status (active/closed), or tags",
    "Use GetMarketDetails to analyze a specific market's prices, volume, and outcomes",
    "Use GetTrades to examine trading patterns, user activity, and historical price movements",
    "Filter by tags to find niche markets (e.g., 'crypto', 'election 2024', 'Wimbledon')",
];


fn polymarket_preamble() -> String {
    PreambleBuilder::new()
        .section(
            PromptSection::titled("Role")
                .paragraph(POLYMARKET_ROLE)
        )
        .section(
            PromptSection::titled("Your Capabilities")
                .bullet_list(POLYMARKET_CAPABILITIES.iter().copied())
        )
        .section(
            PromptSection::titled("Popular Tags for Filtering")
                .bullet_list(POPULAR_TAGS.iter().copied())
        )
        .section(
            PromptSection::titled("Understanding Polymarket")
                .bullet_list(POLYMARKET_CONTEXT.iter().copied())
        )
        .section(
            PromptSection::titled("Execution Guidelines")
                .bullet_list(EXECUTION_GUIDELINES.iter().copied())
        )
        .section(
            PromptSection::titled("Account Context")
                .paragraph(aomi_chat::generate_account_context())
        )
        .build()
}

pub struct PolymarketApp {
    chat_app: ChatApp,
}

impl PolymarketApp {
    pub async fn new() -> Result<Self> {
        Self::init_internal(true, true, None, None).await
    }

    pub async fn new_with_options(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        Self::init_internal(skip_docs, skip_mcp, None, None).await
    }

    pub async fn new_with_senders(
        sender_to_ui: &mpsc::Sender<PolymarketCommand>,
        loading_sender: mpsc::Sender<LoadingProgress>,
        skip_docs: bool,
    ) -> Result<Self> {
        Self::init_internal(skip_docs, false, Some(sender_to_ui), Some(loading_sender)).await
    }

    async fn init_internal(
        skip_docs: bool,
        skip_mcp: bool,
        sender_to_ui: Option<&mpsc::Sender<PolymarketCommand>>,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
    ) -> Result<Self> {
        let mut builder =
            ChatAppBuilder::new_with_model_connection(&polymarket_preamble(), sender_to_ui, false)
                .await?;

        // Add Polymarket-specific tools
        builder.add_tool(GetMarkets)?;
        builder.add_tool(GetMarketDetails)?;
        builder.add_tool(GetTrades)?;

        // Add docs tool if not skipped
        if !skip_docs {
            builder.add_docs_tool(loading_sender, sender_to_ui).await?;
        }

        // Build the final PolymarketApp
        let chat_app = builder.build(skip_mcp, sender_to_ui).await?;

        Ok(Self { chat_app })
    }

    pub fn agent(&self) -> Arc<Agent<CompletionModel>> {
        self.chat_app.agent()
    }

    pub fn chat_app(&self) -> &ChatApp {
        &self.chat_app
    }

    pub fn document_store(&self) -> Option<Arc<Mutex<aomi_rag::DocumentStore>>> {
        self.chat_app.document_store()
    }

    pub async fn process_message(
        &self,
        history: &mut Vec<Message>,
        input: String,
        sender_to_ui: &mpsc::Sender<PolymarketCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        tracing::debug!("[polymarket] process message: {}", input);
        // Delegate to the inner ChatApp
        self.chat_app
            .process_message(history, input, sender_to_ui, interrupt_receiver)
            .await
    }
}

pub async fn run_polymarket_chat(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<PolymarketCommand>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let app =
        Arc::new(PolymarketApp::new_with_senders(&sender_to_ui, loading_sender, skip_docs).await?);
    let mut agent_history: Vec<Message> = Vec::new();

    use aomi_chat::connections::ensure_connection_with_retries;
    ensure_connection_with_retries(&app.agent(), &sender_to_ui).await?;

    let mut receiver_from_ui = receiver_from_ui;
    let mut interrupt_receiver = interrupt_receiver;

    while let Some(input) = receiver_from_ui.recv().await {
        app.process_message(
            &mut agent_history,
            input,
            &sender_to_ui,
            &mut interrupt_receiver,
        )
        .await?;
    }

    Ok(())
}
