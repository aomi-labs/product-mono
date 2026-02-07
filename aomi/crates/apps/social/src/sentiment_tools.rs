#![allow(clippy::manual_async_fn)]

use crate::lunarcrush_client::LunarCrushClient;
use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, WithTopic};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::LazyLock;
use tokio::sync::Mutex;

// Global client instance
static LUNARCRUSH_CLIENT: LazyLock<Mutex<Option<LunarCrushClient>>> =
    LazyLock::new(|| Mutex::new(None));

async fn get_client() -> eyre::Result<LunarCrushClient> {
    let mut guard = LUNARCRUSH_CLIENT.lock().await;
    if let Some(client) = guard.clone() {
        return Ok(client);
    }

    let client = LunarCrushClient::new()?;
    *guard = Some(client.clone());
    Ok(client)
}

// ============================================================================
// Tool 1: Get Crypto Sentiment
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetCryptoSentimentArgs {
    /// Topic to analyze (e.g., 'bitcoin', 'ethereum', 'solana', 'defi')
    pub topic: String,
}

impl AomiToolArgs for GetCryptoSentimentArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "Topic to analyze sentiment for. Can be a coin (bitcoin, ethereum), token symbol ($ETH), or topic (defi, nfts, ai)"
                }
            },
            "required": ["topic"]
        })
    }
}

pub type GetCryptoSentimentParameters = WithTopic<GetCryptoSentimentArgs>;

#[derive(Debug, Clone)]
pub struct GetCryptoSentiment;

impl AomiTool for GetCryptoSentiment {
    const NAME: &'static str = "get_crypto_sentiment";
    const NAMESPACE: &'static str = "social";

    type Args = GetCryptoSentimentParameters;
    type Output = serde_json::Value;
    type Error = SentimentToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get aggregated sentiment data for a crypto topic from X, Reddit, YouTube, TikTok, and news. Returns sentiment scores, social volume, contributor counts, and platform breakdown."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let sentiment = client.get_topic_sentiment(&args.inner.topic).await?;

            // Calculate overall sentiment from platform breakdown
            let overall_sentiment = sentiment.types_sentiment.as_ref().map(|ts| {
                let values: Vec<u32> = ts.values().copied().collect();
                if values.is_empty() {
                    50
                } else {
                    values.iter().sum::<u32>() / values.len() as u32
                }
            });

            Ok(json!({
                "topic": sentiment.topic,
                "title": sentiment.title,
                "rank": sentiment.topic_rank,
                "trend": sentiment.trend,
                "overall_sentiment": overall_sentiment,
                "sentiment_breakdown": {
                    "platforms": sentiment.types_sentiment,
                    "details": sentiment.types_sentiment_detail.map(|d| {
                        d.iter().map(|(k, v)| {
                            (k.clone(), json!({
                                "positive": v.positive,
                                "neutral": v.neutral,
                                "negative": v.negative,
                            }))
                        }).collect::<serde_json::Map<String, serde_json::Value>>()
                    }),
                },
                "social_metrics": {
                    "interactions_24h": sentiment.interactions_24h,
                    "contributors": sentiment.num_contributors,
                    "posts": sentiment.num_posts,
                },
                "platform_activity": {
                    "post_counts": sentiment.types_count,
                    "interactions": sentiment.types_interactions,
                },
                "related_topics": sentiment.related_topics,
                "categories": sentiment.categories,
            }))
        }
    }
}

// ============================================================================
// Tool 2: Get Trending Topics
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetTrendingTopicsArgs {
    /// Maximum number of topics to return (default: 20)
    pub limit: Option<u32>,
}

impl AomiToolArgs for GetTrendingTopicsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of trending topics to return (default: 20)"
                }
            },
            "required": []
        })
    }
}

pub type GetTrendingTopicsParameters = WithTopic<GetTrendingTopicsArgs>;

#[derive(Debug, Clone)]
pub struct GetTrendingTopics;

impl AomiTool for GetTrendingTopics {
    const NAME: &'static str = "get_trending_topics";
    const NAMESPACE: &'static str = "social";

    type Args = GetTrendingTopicsParameters;
    type Output = serde_json::Value;
    type Error = SentimentToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get trending social topics across X, Reddit, YouTube, TikTok, and news. Shows what's gaining attention with rank changes and engagement metrics."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let mut topics = client.get_trending_topics().await?;

            // Limit results
            let limit = args.inner.limit.unwrap_or(20) as usize;
            topics.truncate(limit);

            let formatted_topics: Vec<serde_json::Value> = topics
                .iter()
                .map(|t| {
                    let rank_change_1h = match (t.topic_rank, t.topic_rank_1h_previous) {
                        (Some(current), Some(prev)) => Some(prev as i32 - current as i32),
                        _ => None,
                    };
                    let rank_change_24h = match (t.topic_rank, t.topic_rank_24h_previous) {
                        (Some(current), Some(prev)) => Some(prev as i32 - current as i32),
                        _ => None,
                    };

                    json!({
                        "topic": t.topic,
                        "title": t.title,
                        "rank": t.topic_rank,
                        "rank_change_1h": rank_change_1h,
                        "rank_change_24h": rank_change_24h,
                        "contributors": t.num_contributors,
                        "posts": t.num_posts,
                        "interactions_24h": t.interactions_24h,
                    })
                })
                .collect();

            Ok(json!({
                "trending_count": formatted_topics.len(),
                "topics": formatted_topics,
            }))
        }
    }
}

// ============================================================================
// Tool 3: Get Topic Summary
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetTopicSummaryArgs {
    /// Topic to get AI summary for (e.g., 'bitcoin', 'ethereum', 'solana')
    pub topic: String,
}

impl AomiToolArgs for GetTopicSummaryArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "Topic to get AI summary for (e.g., 'bitcoin', 'ethereum', 'defi')"
                }
            },
            "required": ["topic"]
        })
    }
}

pub type GetTopicSummaryParameters = WithTopic<GetTopicSummaryArgs>;

#[derive(Debug, Clone)]
pub struct GetTopicSummary;

impl AomiTool for GetTopicSummary {
    const NAME: &'static str = "get_topic_summary";
    const NAMESPACE: &'static str = "social";

    type Args = GetTopicSummaryParameters;
    type Output = serde_json::Value;
    type Error = SentimentToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get an AI-generated summary of the hottest news and social posts for a crypto topic. Provides a quick overview of what's being discussed."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let summary = client.get_topic_summary(&args.inner.topic).await?;

            Ok(json!({
                "topic": summary.topic,
                "summary": summary.summary,
                "generated_at": summary.generated_at,
            }))
        }
    }
}

// ============================================================================
// Error Type
// ============================================================================

#[derive(Debug)]
pub struct SentimentToolError(String);

impl std::fmt::Display for SentimentToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SentimentToolError: {}", self.0)
    }
}

impl std::error::Error for SentimentToolError {}
