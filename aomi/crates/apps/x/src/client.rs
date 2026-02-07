use eyre::Result;
use serde::{de::Deserializer, Deserialize, Serialize};
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

        Ok(Self {
            http_client,
            api_key,
        })
    }

    pub fn with_api_key(api_key: String) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http_client,
            api_key,
        })
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

        if !api_response.is_success() {
            return Err(eyre::eyre!("API error: {}", api_response.error_message()));
        }

        api_response
            .data
            .ok_or_else(|| eyre::eyre!("No user data returned"))
    }

    /// Get recent posts from a user
    pub async fn get_user_posts(
        &self,
        username: &str,
        cursor: Option<&str>,
    ) -> Result<PostsResponse> {
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

        if !api_response.is_success() {
            return Err(eyre::eyre!("API error: {}", api_response.error_message()));
        }

        let data = api_response
            .data
            .ok_or_else(|| eyre::eyre!("No posts data returned"))?;

        Ok(PostsResponse {
            posts: data.tweets.unwrap_or_default(),
            cursor: data.next_cursor,
            has_more: data.has_next_page.unwrap_or(false),
        })
    }

    /// Search posts with advanced query
    pub async fn search_posts(
        &self,
        query: &str,
        query_type: &str,
        cursor: Option<&str>,
    ) -> Result<PostsResponse> {
        let url = format!("{}/twitter/tweet/advanced_search", API_BASE);

        let mut params: Vec<(&str, &str)> = vec![("query", query), ("queryType", query_type)];
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
            return Err(eyre::eyre!("Search failed: {} - {}", status, error_text));
        }

        let api_response: ApiResponse<PostsData> = response.json().await?;

        if !api_response.is_success() {
            return Err(eyre::eyre!("API error: {}", api_response.error_message()));
        }

        let data = api_response
            .data
            .ok_or_else(|| eyre::eyre!("No search results returned"))?;

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

        if !api_response.is_success() {
            return Err(eyre::eyre!("API error: {}", api_response.error_message()));
        }

        let data = api_response
            .data
            .ok_or_else(|| eyre::eyre!("No trends data returned"))?;

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

        if !api_response.is_success() {
            return Err(eyre::eyre!("API error: {}", api_response.error_message()));
        }

        api_response
            .data
            .ok_or_else(|| eyre::eyre!("No post data returned"))
    }
}

impl Default for XClient {
    fn default() -> Self {
        Self::new().expect("Failed to create XClient - ensure X_API_KEY is set")
    }
}

// API Response wrapper (handles multiple response shapes)
#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    success: Option<bool>,
    data: Option<T>,
}

impl<T> ApiResponse<T> {
    fn is_success(&self) -> bool {
        if let Some(success) = self.success {
            return success;
        }

        if let Some(status) = &self.status {
            if status.eq_ignore_ascii_case("success") || status.eq_ignore_ascii_case("ok") {
                return true;
            }
        }

        if let Some(code) = self.code {
            return code == 0 || code == 200;
        }

        false
    }

    fn error_message(&self) -> String {
        self.msg
            .clone()
            .or_else(|| self.message.clone())
            .unwrap_or_else(|| "Unknown API error".to_string())
    }
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
    #[serde(default, deserialize_with = "de_opt_string")]
    pub id: Option<String>,
    pub user_name: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub url: Option<String>,
    pub profile_image_url: Option<String>,
    pub profile_banner_url: Option<String>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub followers_count: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub following_count: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub favourites_count: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub statuses_count: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
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
    #[serde(default, deserialize_with = "de_opt_string")]
    pub id: Option<String>,
    pub text: Option<String>,
    pub full_text: Option<String>,
    pub created_at: Option<String>,
    pub author: Option<PostAuthor>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub retweet_count: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub favorite_count: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub reply_count: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub quote_count: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub view_count: Option<u64>,
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
    #[serde(default, deserialize_with = "de_opt_string")]
    pub id: Option<String>,
    pub user_name: Option<String>,
    pub name: Option<String>,
    pub profile_image_url: Option<String>,
    pub is_blue_verified: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    #[serde(default, deserialize_with = "de_opt_string")]
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
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub tweet_count: Option<u64>,
    pub description: Option<String>,
    pub domain_context: Option<String>,
}

fn de_opt_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        Some(serde_json::Value::Bool(b)) => Some(b.to_string()),
        Some(other) => Some(other.to_string()),
    })
}

fn de_opt_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Number(n)) => n.as_u64().or_else(|| {
            n.as_i64()
                .and_then(|v| if v >= 0 { Some(v as u64) } else { None })
        }),
        Some(serde_json::Value::String(s)) => s.parse::<u64>().ok(),
        Some(serde_json::Value::Bool(b)) => Some(if b { 1 } else { 0 }),
        _ => None,
    })
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
