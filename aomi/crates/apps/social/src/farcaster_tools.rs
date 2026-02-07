#![allow(clippy::manual_async_fn)]

use crate::neynar_client::NeynarClient;
use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, WithTopic};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::LazyLock;
use tokio::sync::Mutex;

// Global client instance
static NEYNAR_CLIENT: LazyLock<Mutex<Option<NeynarClient>>> = LazyLock::new(|| Mutex::new(None));

async fn get_client() -> eyre::Result<NeynarClient> {
    let mut guard = NEYNAR_CLIENT.lock().await;
    if let Some(client) = guard.clone() {
        return Ok(client);
    }

    let client = NeynarClient::new()?;
    *guard = Some(client.clone());
    Ok(client)
}

// ============================================================================
// Tool 1: Search Farcaster
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFarcasterArgs {
    /// Search query for casts
    pub query: String,
    /// Pagination cursor (optional)
    pub cursor: Option<String>,
    /// Number of results (default: 25, max: 100)
    pub limit: Option<u32>,
}

impl AomiToolArgs for SearchFarcasterArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query for casts. Supports text search, @mentions, and channel names."
                },
                "cursor": {
                    "type": "string",
                    "description": "Pagination cursor for fetching more results"
                },
                "limit": {
                    "type": "integer",
                    "description": "Number of results to return (default: 25, max: 100)"
                }
            },
            "required": ["query"]
        })
    }
}

pub type SearchFarcasterParameters = WithTopic<SearchFarcasterArgs>;

#[derive(Debug, Clone)]
pub struct SearchFarcaster;

impl AomiTool for SearchFarcaster {
    const NAME: &'static str = "search_farcaster";
    const NAMESPACE: &'static str = "social";

    type Args = SearchFarcasterParameters;
    type Output = serde_json::Value;
    type Error = FarcasterToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Search for casts (posts) on Farcaster. Returns matching posts with author info, engagement metrics, and channel context."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let response = client
                .search_casts(
                    &args.inner.query,
                    args.inner.cursor.as_deref(),
                    args.inner.limit,
                )
                .await?;

            let formatted_casts: Vec<serde_json::Value> =
                response.casts.iter().map(format_cast).collect();

            Ok(json!({
                "query": args.inner.query,
                "results_count": formatted_casts.len(),
                "casts": formatted_casts,
                "cursor": response.cursor,
            }))
        }
    }
}

// ============================================================================
// Tool 2: Get Farcaster User
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetFarcasterUserArgs {
    /// Username (without @) OR FID (numeric)
    pub identifier: String,
}

impl AomiToolArgs for GetFarcasterUserArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "identifier": {
                    "type": "string",
                    "description": "Username (e.g., 'vitalik.eth', 'dwr.eth') or FID (numeric ID like '3')"
                }
            },
            "required": ["identifier"]
        })
    }
}

pub type GetFarcasterUserParameters = WithTopic<GetFarcasterUserArgs>;

#[derive(Debug, Clone)]
pub struct GetFarcasterUser;

impl AomiTool for GetFarcasterUser {
    const NAME: &'static str = "get_farcaster_user";
    const NAMESPACE: &'static str = "social";

    type Args = GetFarcasterUserParameters;
    type Output = serde_json::Value;
    type Error = FarcasterToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get a Farcaster user's profile by username or FID. Returns follower count, bio, verified addresses (ETH/SOL), and more."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let identifier = args.inner.identifier.trim_start_matches('@');

            let user = if let Ok(fid) = identifier.parse::<u64>() {
                client.get_user_by_fid(fid).await?
            } else {
                client.get_user_by_username(identifier).await?
            };

            let bio = user
                .profile
                .as_ref()
                .and_then(|p| p.bio.as_ref())
                .and_then(|b| b.text.clone());

            Ok(json!({
                "fid": user.fid,
                "username": user.username,
                "display_name": user.display_name,
                "bio": bio,
                "profile_image": user.pfp_url,
                "followers": user.follower_count,
                "following": user.following_count,
                "verified_addresses": {
                    "ethereum": user.verified_addresses.as_ref().and_then(|v| v.eth_addresses.clone()),
                    "solana": user.verified_addresses.as_ref().and_then(|v| v.sol_addresses.clone()),
                },
                "verifications": user.verifications,
            }))
        }
    }
}

// ============================================================================
// Tool 3: Get Farcaster Channel
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetFarcasterChannelArgs {
    /// Channel ID (e.g., 'base', 'degen', 'crypto')
    pub channel_id: String,
    /// Include recent casts from the channel
    pub include_feed: Option<bool>,
    /// Number of casts to include (default: 10)
    pub feed_limit: Option<u32>,
}

impl AomiToolArgs for GetFarcasterChannelArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "channel_id": {
                    "type": "string",
                    "description": "Channel ID (e.g., 'base', 'degen', 'crypto', 'memes')"
                },
                "include_feed": {
                    "type": "boolean",
                    "description": "Include recent casts from the channel (default: true)"
                },
                "feed_limit": {
                    "type": "integer",
                    "description": "Number of recent casts to include (default: 10)"
                }
            },
            "required": ["channel_id"]
        })
    }
}

pub type GetFarcasterChannelParameters = WithTopic<GetFarcasterChannelArgs>;

#[derive(Debug, Clone)]
pub struct GetFarcasterChannel;

impl AomiTool for GetFarcasterChannel {
    const NAME: &'static str = "get_farcaster_channel";
    const NAMESPACE: &'static str = "social";

    type Args = GetFarcasterChannelParameters;
    type Output = serde_json::Value;
    type Error = FarcasterToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get information about a Farcaster channel including description, follower count, and optionally recent casts. Popular channels include /base, /degen, /crypto, /memes."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let channel = client.get_channel(&args.inner.channel_id).await?;

            let mut result = json!({
                "id": channel.id,
                "name": channel.name,
                "description": channel.description,
                "image": channel.image_url,
                "followers": channel.follower_count,
                "lead": channel.lead.map(|l| json!({
                    "fid": l.fid,
                    "username": l.username,
                    "display_name": l.display_name,
                })),
            });

            // Include feed if requested (default: true)
            let include_feed = args.inner.include_feed.unwrap_or(true);
            if include_feed {
                let feed_limit = args.inner.feed_limit.unwrap_or(10);
                let feed = client
                    .get_channel_feed(&args.inner.channel_id, None, Some(feed_limit))
                    .await?;
                let formatted_casts: Vec<serde_json::Value> =
                    feed.casts.iter().map(format_cast).collect();
                result["recent_casts"] = json!(formatted_casts);
            }

            Ok(result)
        }
    }
}

// ============================================================================
// Tool 4: Get Farcaster Trending
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetFarcasterTrendingArgs {
    /// Number of trending channels to return (default: 10)
    pub limit: Option<u32>,
}

impl AomiToolArgs for GetFarcasterTrendingArgs {
    fn schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Number of trending channels to return (default: 10)"
                }
            },
            "required": []
        })
    }
}

pub type GetFarcasterTrendingParameters = WithTopic<GetFarcasterTrendingArgs>;

#[derive(Debug, Clone)]
pub struct GetFarcasterTrending;

impl AomiTool for GetFarcasterTrending {
    const NAME: &'static str = "get_farcaster_trending";
    const NAMESPACE: &'static str = "social";

    type Args = GetFarcasterTrendingParameters;
    type Output = serde_json::Value;
    type Error = FarcasterToolError;

    fn support_async(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Get trending Farcaster channels. Shows what topics and communities are gaining attention in the Web3 social space."
    }

    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = eyre::Result<serde_json::Value>> + Send {
        async move {
            let client = get_client().await?;
            let channels = client.get_trending_channels(args.inner.limit).await?;

            let formatted_channels: Vec<serde_json::Value> = channels
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "name": c.name,
                        "description": c.description,
                        "image": c.image_url,
                        "followers": c.follower_count,
                    })
                })
                .collect();

            Ok(json!({
                "trending_count": formatted_channels.len(),
                "channels": formatted_channels,
            }))
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn format_cast(c: &crate::neynar_client::FarcasterCast) -> serde_json::Value {
    json!({
        "hash": c.hash,
        "text": c.text,
        "timestamp": c.timestamp,
        "author": c.author.as_ref().map(|a| json!({
            "fid": a.fid,
            "username": a.username,
            "display_name": a.display_name,
            "profile_image": a.pfp_url,
        })),
        "likes": c.reactions.as_ref().and_then(|r| r.likes_count),
        "recasts": c.reactions.as_ref().and_then(|r| r.recasts_count),
        "replies": c.replies.as_ref().and_then(|r| r.count),
        "channel": c.channel.as_ref().map(|ch| json!({
            "id": ch.id,
            "name": ch.name,
        })),
    })
}

// ============================================================================
// Error Type
// ============================================================================

#[derive(Debug)]
pub struct FarcasterToolError(String);

impl std::fmt::Display for FarcasterToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FarcasterToolError: {}", self.0)
    }
}

impl std::error::Error for FarcasterToolError {}
