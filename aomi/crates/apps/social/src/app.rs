use crate::farcaster_tools::{
    GetFarcasterChannel, GetFarcasterTrending, GetFarcasterUser, SearchFarcaster,
};
use crate::sentiment_tools::{GetCryptoSentiment, GetTopicSummary, GetTrendingTopics};
use aomi_core::{
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
    prompts::{PreambleBuilder, PromptSection},
    BuildOpts, CoreApp, CoreAppBuilder,
};
use aomi_x::tools::{GetXPost, GetXTrends, GetXUser, GetXUserPosts, SearchX};
use async_trait::async_trait;
use eyre::Result;

pub type SocialCommand = CoreCommand;

const SOCIAL_ROLE: &str = r#"You are an AI assistant specialized in crypto social intelligence. You aggregate and analyze social signals across multiple platforms to help users understand market sentiment, track influencers, discover trends, and monitor community discussions.

You have access to three data sources:
- **X (Twitter)** - The largest crypto discussion platform
- **Farcaster** - Web3-native social with on-chain identities
- **LunarCrush** - Aggregated sentiment from X, Reddit, YouTube, TikTok, and news

Use multiple sources to provide comprehensive answers. Cross-reference information when accuracy matters."#;

const SOCIAL_CAPABILITIES: &[&str] = &[
    "Search posts on X and Farcaster simultaneously",
    "Track influencer activity across platforms",
    "Analyze sentiment for any crypto topic (coins, tokens, narratives)",
    "Discover trending topics and conversations",
    "Get AI-generated summaries of what's happening",
    "Monitor Farcaster channels (/base, /degen, /crypto, etc.)",
    "Compare social metrics across platforms",
    "Identify emerging narratives and community signals",
];

const PLATFORM_CONTEXT: &[&str] = &[
    "X (Twitter): Largest reach, breaking news, influencer takes, $ticker discussions",
    "Farcaster: Web3-native, crypto-focused, on-chain identities, channels like /base, /degen",
    "LunarCrush sentiment: Aggregated from X, Reddit, YouTube, TikTok, news; includes Galaxy Score™",
    "Sentiment scale: 0-100 where 50 is neutral, >70 is bullish, <30 is bearish",
    "Social dominance: Shows relative attention share compared to total crypto discussion",
];

const EXECUTION_GUIDELINES: &[&str] = &[
    "For sentiment queries, use get_crypto_sentiment for aggregated data",
    "For specific posts/takes, use search_x and search_farcaster",
    "For influencer research, check both get_x_user and get_farcaster_user",
    "For trending discovery, use get_trending_topics and get_farcaster_trending",
    "For quick summaries, use get_topic_summary (AI-generated)",
    "Cross-reference platforms when accuracy matters",
    "Note platform-specific context (Farcaster is more web3-native)",
    "Provide sentiment interpretation (what the numbers mean)",
];

fn social_preamble() -> String {
    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(SOCIAL_ROLE))
        .section(
            PromptSection::titled("Your Capabilities")
                .bullet_list(SOCIAL_CAPABILITIES.iter().copied()),
        )
        .section(
            PromptSection::titled("Platform Context")
                .bullet_list(PLATFORM_CONTEXT.iter().copied()),
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

pub struct SocialApp {
    chat_app: CoreApp,
}

impl SocialApp {
    pub async fn default() -> Result<Self> {
        Self::new(BuildOpts::default()).await
    }

    pub async fn new(opts: BuildOpts) -> Result<Self> {
        let mut builder = CoreAppBuilder::new(&social_preamble(), opts, None).await?;

        // Add X tools (from aomi-x)
        builder.add_tool(GetXUser)?;
        builder.add_tool(GetXUserPosts)?;
        builder.add_tool(SearchX)?;
        builder.add_tool(GetXTrends)?;
        builder.add_tool(GetXPost)?;

        // Add Farcaster tools
        builder.add_tool(SearchFarcaster)?;
        builder.add_tool(GetFarcasterUser)?;
        builder.add_tool(GetFarcasterChannel)?;
        builder.add_tool(GetFarcasterTrending)?;

        // Add LunarCrush sentiment tools
        builder.add_tool(GetCryptoSentiment)?;
        builder.add_tool(GetTrendingTopics)?;
        builder.add_tool(GetTopicSummary)?;

        // Build the final SocialApp
        let chat_app = builder.build(opts, None).await?;

        Ok(Self { chat_app })
    }

    pub async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        tracing::debug!("[social] process message: {}", input);
        self.chat_app.process_message(input, state, ctx).await
    }
}

#[async_trait]
impl AomiApp for SocialApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        SocialApp::process_message(self, input, state, ctx).await
    }

    fn tool_namespaces(&self) -> std::sync::Arc<std::collections::HashMap<String, String>> {
        self.chat_app.tool_namespaces()
    }
}
