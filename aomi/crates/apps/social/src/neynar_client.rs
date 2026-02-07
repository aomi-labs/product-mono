use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

const NEYNAR_API_BASE: &str = "https://api.neynar.com/v2";

#[derive(Clone)]
pub struct NeynarClient {
    http_client: reqwest::Client,
    api_key: String,
}

impl NeynarClient {
    pub fn new() -> Result<Self> {
        let api_key = env::var("NEYNAR_API_KEY")
            .map_err(|_| eyre::eyre!("NEYNAR_API_KEY environment variable not set"))?;

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http_client,
            api_key,
        })
    }

    /// Search for casts on Farcaster
    pub async fn search_casts(
        &self,
        query: &str,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<CastsResponse> {
        let url = format!("{}/farcaster/cast/search", NEYNAR_API_BASE);

        let mut params: Vec<(&str, String)> = vec![("q", query.to_string())];
        if let Some(c) = cursor {
            params.push(("cursor", c.to_string()));
        }
        let limit_val = limit.unwrap_or(25);
        params.push(("limit", limit_val.to_string()));

        let response = self
            .http_client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!("Search failed: {} - {}", status, error_text));
        }

        let api_response: SearchCastsApiResponse = response.json().await?;
        Ok(CastsResponse {
            casts: api_response.result.casts,
            cursor: api_response.result.next.map(|n| n.cursor),
        })
    }

    /// Get user by username
    pub async fn get_user_by_username(&self, username: &str) -> Result<FarcasterUser> {
        let url = format!("{}/farcaster/user/by_username", NEYNAR_API_BASE);

        let response = self
            .http_client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(&[("username", username)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get user {}: {} - {}",
                username,
                status,
                error_text
            ));
        }

        let api_response: UserApiResponse = response.json().await?;
        Ok(api_response.user)
    }

    /// Get user by FID
    pub async fn get_user_by_fid(&self, fid: u64) -> Result<FarcasterUser> {
        let url = format!("{}/farcaster/user/bulk", NEYNAR_API_BASE);

        let response = self
            .http_client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(&[("fids", fid.to_string())])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get user {}: {} - {}",
                fid,
                status,
                error_text
            ));
        }

        let api_response: BulkUsersApiResponse = response.json().await?;
        api_response
            .users
            .into_iter()
            .next()
            .ok_or_else(|| eyre::eyre!("User not found"))
    }

    /// Get channel info
    pub async fn get_channel(&self, channel_id: &str) -> Result<FarcasterChannel> {
        let url = format!("{}/farcaster/channel", NEYNAR_API_BASE);

        let response = self
            .http_client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(&[("id", channel_id)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get channel {}: {} - {}",
                channel_id,
                status,
                error_text
            ));
        }

        let api_response: ChannelApiResponse = response.json().await?;
        Ok(api_response.channel)
    }

    /// Get trending channels
    pub async fn get_trending_channels(&self, limit: Option<u32>) -> Result<Vec<FarcasterChannel>> {
        let url = format!("{}/farcaster/channel/trending", NEYNAR_API_BASE);

        let limit_val = limit.unwrap_or(10);
        let response = self
            .http_client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(&[("limit", limit_val.to_string())])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get trending channels: {} - {}",
                status,
                error_text
            ));
        }

        let api_response: TrendingChannelsApiResponse = response.json().await?;
        Ok(api_response.channels)
    }

    /// Get channel feed
    pub async fn get_channel_feed(
        &self,
        channel_id: &str,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<CastsResponse> {
        let url = format!("{}/farcaster/feed/channels", NEYNAR_API_BASE);

        let mut params: Vec<(&str, String)> = vec![("channel_ids", channel_id.to_string())];
        if let Some(c) = cursor {
            params.push(("cursor", c.to_string()));
        }
        let limit_val = limit.unwrap_or(25);
        params.push(("limit", limit_val.to_string()));

        let response = self
            .http_client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get channel feed: {} - {}",
                status,
                error_text
            ));
        }

        let api_response: ChannelFeedApiResponse = response.json().await?;
        Ok(CastsResponse {
            casts: api_response.casts,
            cursor: api_response.next.map(|n| n.cursor),
        })
    }
}

// API Response types

#[derive(Debug, Deserialize)]
struct SearchCastsApiResponse {
    result: SearchCastsResult,
}

#[derive(Debug, Deserialize)]
struct SearchCastsResult {
    casts: Vec<FarcasterCast>,
    next: Option<NextCursor>,
}

#[derive(Debug, Deserialize)]
struct NextCursor {
    cursor: String,
}

#[derive(Debug, Deserialize)]
struct UserApiResponse {
    user: FarcasterUser,
}

#[derive(Debug, Deserialize)]
struct BulkUsersApiResponse {
    users: Vec<FarcasterUser>,
}

#[derive(Debug, Deserialize)]
struct ChannelApiResponse {
    channel: FarcasterChannel,
}

#[derive(Debug, Deserialize)]
struct TrendingChannelsApiResponse {
    channels: Vec<FarcasterChannel>,
}

#[derive(Debug, Deserialize)]
struct ChannelFeedApiResponse {
    casts: Vec<FarcasterCast>,
    next: Option<NextCursor>,
}

// Public data types

pub struct CastsResponse {
    pub casts: Vec<FarcasterCast>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarcasterUser {
    pub fid: u64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub pfp_url: Option<String>,
    #[serde(default)]
    pub profile: Option<UserProfile>,
    pub follower_count: Option<u64>,
    pub following_count: Option<u64>,
    pub verifications: Option<Vec<String>>,
    pub verified_addresses: Option<VerifiedAddresses>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub bio: Option<Bio>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bio {
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedAddresses {
    pub eth_addresses: Option<Vec<String>>,
    pub sol_addresses: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarcasterCast {
    pub hash: String,
    pub text: Option<String>,
    pub timestamp: Option<String>,
    pub author: Option<CastAuthor>,
    pub reactions: Option<CastReactions>,
    pub replies: Option<CastReplies>,
    pub channel: Option<CastChannel>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastAuthor {
    pub fid: u64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub pfp_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastReactions {
    pub likes_count: Option<u64>,
    pub recasts_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastReplies {
    pub count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastChannel {
    pub id: Option<String>,
    pub name: Option<String>,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarcasterChannel {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub follower_count: Option<u64>,
    pub lead: Option<CastAuthor>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
