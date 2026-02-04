#![allow(clippy::manual_async_fn)]

use crate::client::XClient;
use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, WithTopic};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::LazyLock;
use tokio::sync::Mutex;

// Global client instance
static X_CLIENT: LazyLock<Mutex<Option<XClient>>> = LazyLock::new(|| {
    Mutex::new(XClient::new().ok())
});

async fn get_client() -> eyre::Result<XClient> {
    let guard = X_CLIENT.lock().await;
    guard.clone().ok_or_else(|| eyre::eyre!("X client not initialized - ensure X_API_KEY is set"))
}

// ============================================================================
// Tool 1: Get X User
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetXUserArgs {
    /// X username (without @)
    pub username: String,
}

impl AomiToolArgs for GetXUserArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "username": {
                    "type": "string",
                    "description": "X username without the @ symbol (e.g., 'elonmusk')"
                }
            },
            "required": ["username"]
        })
    }
}

pub type GetXUserParameters = WithTopic<GetXUserArgs>;

#[derive(Debug, Clone)]
pub struct GetXUser;

impl AomiTool for GetXUser {
    const NAME: &'static str = "get_x_user";

    type Args = GetXUserParameters;
    type Output = serde_json::Value;
    type Error = XToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get an X (Twitter) user's profile information by username. Returns follower count, bio, verification status, and more."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let username = args.inner.username.trim_start_matches('@');
            let client = get_client().await?;
            let user = client.get_user(username).await?;

            Ok(json!({
                "id": user.id,
                "username": user.user_name,
                "name": user.name,
                "bio": user.description,
                "location": user.location,
                "url": user.url,
                "profile_image": user.profile_image_url,
                "banner_image": user.profile_banner_url,
                "followers": user.followers_count,
                "following": user.following_count,
                "posts_count": user.statuses_count,
                "likes_count": user.favourites_count,
                "listed_count": user.listed_count,
                "created_at": user.created_at,
                "verified": user.verified,
                "blue_verified": user.is_blue_verified,
            }))
        }
    }
}

// ============================================================================
// Tool 2: Get X User Posts
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetXUserPostsArgs {
    /// X username (without @)
    pub username: String,
    /// Pagination cursor (optional)
    pub cursor: Option<String>,
}

impl AomiToolArgs for GetXUserPostsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "username": {
                    "type": "string",
                    "description": "X username without the @ symbol"
                },
                "cursor": {
                    "type": "string",
                    "description": "Pagination cursor for fetching more results"
                }
            },
            "required": ["username"]
        })
    }
}

pub type GetXUserPostsParameters = WithTopic<GetXUserPostsArgs>;

#[derive(Debug, Clone)]
pub struct GetXUserPosts;

impl AomiTool for GetXUserPosts {
    const NAME: &'static str = "get_x_user_posts";

    type Args = GetXUserPostsParameters;
    type Output = serde_json::Value;
    type Error = XToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get recent posts from an X (Twitter) user. Returns post text, engagement metrics, and metadata."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let username = args.inner.username.trim_start_matches('@');
            let client = get_client().await?;
            let response = client.get_user_posts(username, args.inner.cursor.as_deref()).await?;

            let formatted_posts: Vec<serde_json::Value> = response
                .posts
                .iter()
                .map(|p| format_post(p))
                .collect();

            Ok(json!({
                "posts_count": formatted_posts.len(),
                "posts": formatted_posts,
                "cursor": response.cursor,
                "has_more": response.has_more,
            }))
        }
    }
}

// ============================================================================
// Tool 3: Search X
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchXArgs {
    /// Search query (supports operators like from:, #, @, lang:, since:, until:)
    pub query: String,
    /// Sort order: "Latest" or "Top" (default: Latest)
    pub query_type: Option<String>,
    /// Pagination cursor (optional)
    pub cursor: Option<String>,
}

impl AomiToolArgs for SearchXArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query. Supports operators: from:user, #hashtag, @mention, lang:en, since:2026-01-01, until:2026-02-01, min_faves:100"
                },
                "query_type": {
                    "type": "string",
                    "enum": ["Latest", "Top"],
                    "description": "Sort order: 'Latest' for recent posts, 'Top' for popular posts (default: Latest)"
                },
                "cursor": {
                    "type": "string",
                    "description": "Pagination cursor for fetching more results"
                }
            },
            "required": ["query"]
        })
    }
}

pub type SearchXParameters = WithTopic<SearchXArgs>;

#[derive(Debug, Clone)]
pub struct SearchX;

impl AomiTool for SearchX {
    const NAME: &'static str = "search_x";

    type Args = SearchXParameters;
    type Output = serde_json::Value;
    type Error = XToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Search for posts on X (Twitter) using advanced query operators. Supports filtering by user, hashtag, date range, and engagement metrics."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let query_type = args.inner.query_type.as_deref().unwrap_or("Latest");
            let client = get_client().await?;
            let response = client.search_posts(&args.inner.query, query_type, args.inner.cursor.as_deref()).await?;

            let formatted_posts: Vec<serde_json::Value> = response
                .posts
                .iter()
                .map(|p| format_post(p))
                .collect();

            Ok(json!({
                "query": args.inner.query,
                "query_type": query_type,
                "results_count": formatted_posts.len(),
                "posts": formatted_posts,
                "cursor": response.cursor,
                "has_more": response.has_more,
            }))
        }
    }
}

// ============================================================================
// Tool 4: Get X Trends
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetXTrendsArgs {}

impl AomiToolArgs for GetXTrendsArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
}

pub type GetXTrendsParameters = WithTopic<GetXTrendsArgs>;

#[derive(Debug, Clone)]
pub struct GetXTrends;

impl AomiTool for GetXTrends {
    const NAME: &'static str = "get_x_trends";

    type Args = GetXTrendsParameters;
    type Output = serde_json::Value;
    type Error = XToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get current trending topics on X (Twitter). Returns trend names and post counts."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let trends = client.get_trends().await?;

            let formatted_trends: Vec<serde_json::Value> = trends
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "url": t.url,
                        "post_count": t.tweet_count,
                        "description": t.description,
                        "category": t.domain_context,
                    })
                })
                .collect();

            Ok(json!({
                "trends_count": formatted_trends.len(),
                "trends": formatted_trends,
            }))
        }
    }
}

// ============================================================================
// Tool 5: Get X Post
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetXPostArgs {
    /// Post ID
    pub post_id: String,
}

impl AomiToolArgs for GetXPostArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "post_id": {
                    "type": "string",
                    "description": "The ID of the post to retrieve"
                }
            },
            "required": ["post_id"]
        })
    }
}

pub type GetXPostParameters = WithTopic<GetXPostArgs>;

#[derive(Debug, Clone)]
pub struct GetXPost;

impl AomiTool for GetXPost {
    const NAME: &'static str = "get_x_post";

    type Args = GetXPostParameters;
    type Output = serde_json::Value;
    type Error = XToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get details of a specific X (Twitter) post by its ID. Returns full post content, engagement metrics, and author info."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let post = client.get_post(&args.inner.post_id).await?;

            Ok(format_post(&post))
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn format_post(p: &crate::client::Post) -> serde_json::Value {
    json!({
        "id": p.id,
        "text": p.full_text.as_ref().or(p.text.as_ref()),
        "created_at": p.created_at,
        "author": p.author.as_ref().map(|a| json!({
            "id": a.id,
            "username": a.user_name,
            "name": a.name,
            "profile_image": a.profile_image_url,
            "blue_verified": a.is_blue_verified,
        })),
        "reposts": p.retweet_count,
        "likes": p.favorite_count,
        "replies": p.reply_count,
        "quotes": p.quote_count,
        "views": p.view_count,
        "language": p.lang,
        "is_repost": p.is_retweet,
        "is_quote": p.is_quote,
        "reply_to": p.in_reply_to_status_id,
        "conversation_id": p.conversation_id,
        "hashtags": p.hashtags,
        "mentions": p.mentions.as_ref().map(|m| 
            m.iter().map(|mention| json!({
                "username": mention.user_name,
                "name": mention.name,
            })).collect::<Vec<_>>()
        ),
        "urls": p.urls.as_ref().map(|u|
            u.iter().map(|url| json!({
                "url": url.expanded_url,
                "display": url.display_url,
            })).collect::<Vec<_>>()
        ),
        "media": p.media.as_ref().map(|m|
            m.iter().map(|media| json!({
                "url": media.media_url_https,
                "type": media.media_type,
            })).collect::<Vec<_>>()
        ),
    })
}

// ============================================================================
// Error Type
// ============================================================================

#[derive(Debug)]
pub struct XToolError(String);

impl std::fmt::Display for XToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "XToolError: {}", self.0)
    }
}

impl std::error::Error for XToolError {}
