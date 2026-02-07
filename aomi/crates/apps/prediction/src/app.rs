//! Prediction Wizard App - Aggregated prediction market intelligence

use crate::tools::{
    GetAggregatedOdds, GetMarketDetails, GetTrendingPredictions, SearchPredictionMarkets,
};
use aomi_core::{
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
    prompts::{PreambleBuilder, PromptSection},
    BuildOpts, CoreApp, CoreAppBuilder,
};
use async_trait::async_trait;
use eyre::Result;

pub type PredictionCommand = CoreCommand;

const PREDICTION_ROLE: &str = "You are the Prediction Wizard 🔮, an expert in prediction markets and probabilistic forecasting. You aggregate data from Polymarket, Kalshi, Manifold, and Metaculus to help users understand event probabilities and find trading opportunities.";

const PREDICTION_CAPABILITIES: &[&str] = &[
    "Search prediction markets across Polymarket, Kalshi, Manifold, and Metaculus",
    "Get detailed market information including resolution criteria and current prices",
    "Compare probabilities across platforms to find consensus and arbitrage",
    "Discover trending and high-volume prediction markets",
];

const PLATFORM_CONTEXT: &[&str] = &[
    "Polymarket: Crypto-native (USDC on Polygon), largest volume, politics & events",
    "Kalshi: CFTC-regulated, US only, economics, weather, current events",
    "Manifold: Community-driven, play money + prize markets, any topic",
    "Metaculus: Scientific forecasting, long-term AI/tech predictions, reputation-based",
];

const INTERPRETATION_GUIDE: &[&str] = &[
    "Prices represent crowd probability estimates (65% = 65 cents for $1 payout)",
    "Higher volume/liquidity means more reliable signal",
    "Platform spread >5% may indicate arbitrage or different resolution criteria",
    "Resolution criteria matter - always check exact terms before trading",
];

const EXECUTION_GUIDELINES: &[&str] = &[
    "Use search_prediction_markets to find markets on any topic across platforms",
    "Use get_prediction_market_details for deep dive on specific markets",
    "Use get_aggregated_odds to compare the same question across platforms",
    "Use get_trending_predictions to discover what people are betting on",
];

fn prediction_preamble() -> String {
    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(PREDICTION_ROLE))
        .section(
            PromptSection::titled("Your Capabilities")
                .bullet_list(PREDICTION_CAPABILITIES.iter().copied()),
        )
        .section(
            PromptSection::titled("Platform Guide")
                .bullet_list(PLATFORM_CONTEXT.iter().copied()),
        )
        .section(
            PromptSection::titled("Understanding Predictions")
                .bullet_list(INTERPRETATION_GUIDE.iter().copied()),
        )
        .section(
            PromptSection::titled("Execution Guidelines")
                .bullet_list(EXECUTION_GUIDELINES.iter().copied()),
        )
        .section(
            PromptSection::titled("Account Context")
                .paragraph(aomi_core::generate_account_context()),
        )
        .build()
}

pub struct PredictionApp {
    chat_app: CoreApp,
}

impl PredictionApp {
    pub async fn default() -> Result<Self> {
        Self::new(BuildOpts::default()).await
    }

    pub async fn new(opts: BuildOpts) -> Result<Self> {
        let mut builder = CoreAppBuilder::new(&prediction_preamble(), opts, None).await?;

        // Add Prediction-specific tools
        builder.add_tool(SearchPredictionMarkets)?;
        builder.add_tool(GetMarketDetails)?;
        builder.add_tool(GetAggregatedOdds)?;
        builder.add_tool(GetTrendingPredictions)?;

        // Build the final PredictionApp
        let chat_app = builder.build(opts, None).await?;

        Ok(Self { chat_app })
    }

    pub async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        tracing::debug!("[prediction] process message: {}", input);
        self.chat_app.process_message(input, state, ctx).await
    }
}

#[async_trait]
impl AomiApp for PredictionApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        PredictionApp::process_message(self, input, state, ctx).await
    }

    fn tool_namespaces(&self) -> std::sync::Arc<std::collections::HashMap<String, String>> {
        self.chat_app.tool_namespaces()
    }
}
