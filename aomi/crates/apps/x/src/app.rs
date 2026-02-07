use crate::tools::{GetXPost, GetXTrends, GetXUser, GetXUserPosts, SearchX};
use aomi_core::{
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
    prompts::{PreambleBuilder, PromptSection},
    BuildOpts, CoreApp, CoreAppBuilder,
};
use async_trait::async_trait;
use eyre::Result;

pub type XCommand = CoreCommand;

const X_ROLE: &str = "You are an AI assistant specialized in X (formerly Twitter) data analysis. You help users discover content, analyze trends, monitor accounts, and understand social media dynamics. Keep responses concise and data-driven.";

const X_CAPABILITIES: &[&str] = &[
    "Search posts by keywords, hashtags, users, or advanced operators",
    "Get user profiles with follower counts, bio, and verification status",
    "Retrieve recent posts from any public account",
    "Discover trending topics and conversations",
    "Analyze post engagement (likes, reposts, replies, views)",
    "Track mentions and conversations around specific topics",
];

const SEARCH_OPERATORS: &[&str] = &[
    "from:username — Posts from specific user",
    "#hashtag — Posts containing hashtag",
    "@mention — Posts mentioning user",
    "to:username — Replies to specific user",
    "lang:en — Filter by language (en, es, fr, ja, etc.)",
    "since:2026-01-01 — Posts after date",
    "until:2026-02-01 — Posts before date",
    "min_faves:100 — Minimum likes",
    "min_retweets:50 — Minimum reposts",
    "-keyword — Exclude keyword",
    "filter:media — Only posts with media",
    "filter:links — Only posts with links",
];

const X_CONTEXT: &[&str] = &[
    "X (formerly Twitter) is a real-time social media platform for short-form content",
    "Posts are limited to 280 characters (longer for premium users)",
    "Engagement metrics include likes, reposts (retweets), replies, quotes, and views",
    "Blue checkmarks indicate X Premium subscribers, not necessarily verified identities",
    "Trending topics reflect current popular conversations",
];

const EXECUTION_GUIDELINES: &[&str] = &[
    "Use search_x with operators to find specific content (e.g., 'from:elonmusk AI')",
    "Use get_x_user to look up profiles and follower counts",
    "Use get_x_user_posts to see what someone has been posting recently",
    "Use get_x_trends to discover what's currently popular",
    "Use get_x_post to get full details of a specific post by ID",
    "Combine search operators for precise queries (e.g., '#crypto min_faves:1000 lang:en')",
];

fn x_preamble() -> String {
    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(X_ROLE))
        .section(
            PromptSection::titled("Your Capabilities").bullet_list(X_CAPABILITIES.iter().copied()),
        )
        .section(
            PromptSection::titled("Search Operators").bullet_list(SEARCH_OPERATORS.iter().copied()),
        )
        .section(PromptSection::titled("Understanding X").bullet_list(X_CONTEXT.iter().copied()))
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

pub struct XApp {
    chat_app: CoreApp,
}

impl XApp {
    pub async fn default() -> Result<Self> {
        Self::new(BuildOpts::default()).await
    }

    pub async fn new(opts: BuildOpts) -> Result<Self> {
        let mut builder = CoreAppBuilder::new(&x_preamble(), opts, None).await?;

        // Add X-specific tools
        builder.add_tool(GetXUser)?;
        builder.add_tool(GetXUserPosts)?;
        builder.add_tool(SearchX)?;
        builder.add_tool(GetXTrends)?;
        builder.add_tool(GetXPost)?;

        // Build the final XApp
        let chat_app = builder.build(opts, None).await?;

        Ok(Self { chat_app })
    }

    pub async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        tracing::debug!("[x] process message: {}", input);
        self.chat_app.process_message(input, state, ctx).await
    }
}

#[async_trait]
impl AomiApp for XApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        XApp::process_message(self, input, state, ctx).await
    }

    fn tool_namespaces(&self) -> std::sync::Arc<std::collections::HashMap<String, String>> {
        self.chat_app.tool_namespaces()
    }
}
