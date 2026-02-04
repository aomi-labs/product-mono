use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

const API_BASE: &str = "https://api.twitterapi.io";

#[derive(Clone)]
pub struct XClient {
    http_client: reqwest::Client,
    api_key: String,
}

impl XClient {
    pub fn new() -> Result<Self> {
        let api_key = env::var("X_API_KEY")
            .map_err(|_| eyre::eyre!("X_API_KEY environment variable not set"))?;

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { http_client, api_key })
    }

    pub fn with_api_key(api_key: String) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { http_client, api_key })
    }

    /// Get user profile by username
    pub async fn get_user(&self, username: &str) -> Result<User> {
        let url = format!("{}/twitter/user/info", API_BASE);

        let response = self
            .http_client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&[("userName", username)])
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

        let api_response: ApiResponse<User> = response.json().await?;
        
        if api_response.status != "success" {
            return Err(eyre::eyre!("API error: {:?}", api_response.msg));
        }

        api_response.data.ok_or_else(|| eyre::eyre!("No user data returned"))
    }

    /// Get recent posts from a user
    pub async fn get_user_posts(&self, username: &str, cursor: Option<&str>) -> Result<PostsResponse> {
        let url = format!("{}/twitter/user/last_tweets", API_BASE);

        let mut query: Vec<(&str, &str)> = vec![("userName", username)];
        if let Some(c) = cursor {
            query.push(("cursor", c));
        }

        let response = self
            .http_client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&query)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get posts for {}: {} - {}",
                username,
                status,
                error_text
            ));
        }

        let api_response: ApiResponse<PostsData> = response.json().await?;
        
        if api_response.status != "success" {
            return Err(eyre::eyre!("API error: {:?}", api_response.msg));
        }

        let data = api_response.data.ok_or_else(|| eyre::eyre!("No posts data returned"))?;
        
        Ok(PostsResponse {
            posts: data.tweets.unwrap_or_default(),
            cursor: data.next_cursor,
            has_more: data.has_next_page.unwrap_or(false),
        })
    }

    /// Search posts with advanced query
    pub async fn search_posts(&self, query: &str, query_type: &str, cursor: Option<&str>) -> Result<PostsResponse> {
        let url = format!("{}/twitter/tweet/advanced_search", API_BASE);

        let mut params: Vec<(&str, &str)> = vec![
            ("query", query),
            ("queryType", query_type),
        ];
        if let Some(c) = cursor {
            params.push(("cursor", c));
        }

        let response = self
            .http_client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Search failed: {} - {}",
                status,
                error_text
            ));
        }

        let api_response: ApiResponse<PostsData> = response.json().await?;
        
        if api_response.status != "success" {
            return Err(eyre::eyre!("API error: {:?}", api_response.msg));
        }

        let data = api_response.data.ok_or_else(|| eyre::eyre!("No search results returned"))?;
        
        Ok(PostsResponse {
            posts: data.tweets.unwrap_or_default(),
            cursor: data.next_cursor,
            has_more: data.has_next_page.unwrap_or(false),
        })
    }

    /// Get trending topics
    pub async fn get_trends(&self) -> Result<Vec<Trend>> {
        let url = format!("{}/twitter/trends", API_BASE);

        let response = self
            .http_client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get trends: {} - {}",
                status,
                error_text
            ));
        }

        let api_response: ApiResponse<TrendsData> = response.json().await?;
        
        if api_response.status != "success" {
            return Err(eyre::eyre!("API error: {:?}", api_response.msg));
        }

        let data = api_response.data.ok_or_else(|| eyre::eyre!("No trends data returned"))?;
        
        Ok(data.trends.unwrap_or_default())
    }

    /// Get post by ID
    pub async fn get_post(&self, post_id: &str) -> Result<Post> {
        let url = format!("{}/twitter/tweet/info", API_BASE);

        let response = self
            .http_client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&[("tweetId", post_id)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get post {}: {} - {}",
                post_id,
                status,
                error_text
            ));
        }

        let api_response: ApiResponse<Post> = response.json().await?;
        
        if api_response.status != "success" {
            return Err(eyre::eyre!("API error: {:?}", api_response.msg));
        }

        api_response.data.ok_or_else(|| eyre::eyre!("No post data returned"))
    }
}

impl Default for XClient {
    fn default() -> Self {
        Self::new().expect("Failed to create XClient - ensure X_API_KEY is set")
    }
}

// API Response wrapper
#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    status: String,
    msg: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct PostsData {
    tweets: Option<Vec<Post>>,
    next_cursor: Option<String>,
    has_next_page: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct TrendsData {
    trends: Option<Vec<Trend>>,
}

// Public response types
pub struct PostsResponse {
    pub posts: Vec<Post>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

// Data models
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: Option<String>,
    pub user_name: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub url: Option<String>,
    pub profile_image_url: Option<String>,
    pub profile_banner_url: Option<String>,
    pub followers_count: Option<u64>,
    pub following_count: Option<u64>,
    pub favourites_count: Option<u64>,
    pub statuses_count: Option<u64>,
    pub listed_count: Option<u64>,
    pub created_at: Option<String>,
    pub verified: Option<bool>,
    pub is_blue_verified: Option<bool>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub id: Option<String>,
    pub text: Option<String>,
    pub full_text: Option<String>,
    pub created_at: Option<String>,
    pub author: Option<PostAuthor>,
    pub retweet_count: Option<u64>,
    pub favorite_count: Option<u64>,
    pub reply_count: Option<u64>,
    pub quote_count: Option<u64>,
    pub view_count: Option<String>,
    pub lang: Option<String>,
    pub is_retweet: Option<bool>,
    pub is_quote: Option<bool>,
    pub in_reply_to_status_id: Option<String>,
    pub conversation_id: Option<String>,
    pub hashtags: Option<Vec<String>>,
    pub mentions: Option<Vec<Mention>>,
    pub urls: Option<Vec<UrlEntity>>,
    pub media: Option<Vec<Media>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostAuthor {
    pub id: Option<String>,
    pub user_name: Option<String>,
    pub name: Option<String>,
    pub profile_image_url: Option<String>,
    pub is_blue_verified: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    pub id: Option<String>,
    pub user_name: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlEntity {
    pub url: Option<String>,
    pub expanded_url: Option<String>,
    pub display_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub media_url_https: Option<String>,
    #[serde(rename = "type")]
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trend {
    pub name: Option<String>,
    pub url: Option<String>,
    pub tweet_count: Option<u64>,
    pub description: Option<String>,
    pub domain_context: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_requires_api_key() {
        // Remove the env var if set for this test
        std::env::remove_var("X_API_KEY");
        let result = XClient::new();
        assert!(result.is_err());
    }

    #[test]
    fn test_client_with_api_key() {
        let client = XClient::with_api_key("test_key".to_string());
        assert!(client.is_ok());
    }
}
