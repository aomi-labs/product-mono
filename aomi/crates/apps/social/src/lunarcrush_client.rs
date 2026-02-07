use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

const LUNARCRUSH_API_BASE: &str = "https://lunarcrush.com/api4";

#[derive(Clone)]
pub struct LunarCrushClient {
    http_client: reqwest::Client,
    api_key: String,
}

impl LunarCrushClient {
    pub fn new() -> Result<Self> {
        let api_key = env::var("LUNARCRUSH_API_KEY")
            .map_err(|_| eyre::eyre!("LUNARCRUSH_API_KEY environment variable not set"))?;

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http_client,
            api_key,
        })
    }

    /// Get trending social topics
    pub async fn get_trending_topics(&self) -> Result<Vec<TrendingTopic>> {
        let url = format!("{}/public/topics/list/v1", LUNARCRUSH_API_BASE);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get trending topics: {} - {}",
                status,
                error_text
            ));
        }

        let api_response: TopicsListResponse = response.json().await?;
        Ok(api_response.data)
    }

    /// Get sentiment data for a specific topic (coin, token, or social topic)
    pub async fn get_topic_sentiment(&self, topic: &str) -> Result<TopicSentiment> {
        // Normalize topic to lowercase
        let topic = topic.to_lowercase().replace(['$', '#'], "");
        let url = format!("{}/public/topic/{}/v1", LUNARCRUSH_API_BASE, topic);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get topic sentiment for {}: {} - {}",
                topic,
                status,
                error_text
            ));
        }

        let api_response: TopicSentimentResponse = response.json().await?;
        Ok(api_response.data)
    }

    /// Get AI summary of what's happening for a topic
    pub async fn get_topic_summary(&self, topic: &str) -> Result<TopicSummary> {
        let topic = topic.to_lowercase().replace(['$', '#'], "");
        let url = format!("{}/public/topic/{}/whatsup/v1", LUNARCRUSH_API_BASE, topic);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get topic summary for {}: {} - {}",
                topic,
                status,
                error_text
            ));
        }

        let api_response: TopicSummaryResponse = response.json().await?;
        Ok(TopicSummary {
            topic: api_response.config.topic,
            summary: api_response.summary,
            generated_at: api_response.config.generated,
        })
    }

    /// Get coin list with sentiment data
    pub async fn get_coins_list(&self, filter: Option<&str>) -> Result<Vec<CoinData>> {
        let url = format!("{}/public/coins/list/v2", LUNARCRUSH_API_BASE);

        let mut query: Vec<(&str, &str)> = vec![];
        if let Some(f) = filter {
            query.push(("filter", f));
        }

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .query(&query)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(eyre::eyre!(
                "Failed to get coins list: {} - {}",
                status,
                error_text
            ));
        }

        let api_response: CoinsListResponse = response.json().await?;
        Ok(api_response.data)
    }
}

// API Response types

#[derive(Debug, Deserialize)]
struct TopicsListResponse {
    data: Vec<TrendingTopic>,
}

#[derive(Debug, Deserialize)]
struct TopicSentimentResponse {
    #[allow(dead_code)]
    config: TopicConfig,
    data: TopicSentiment,
}

#[derive(Debug, Deserialize)]
struct TopicConfig {
    #[allow(dead_code)]
    topic: String,
    #[allow(dead_code)]
    generated: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TopicSummaryResponse {
    config: TopicSummaryConfig,
    summary: String,
}

#[derive(Debug, Deserialize)]
struct TopicSummaryConfig {
    topic: String,
    generated: u64,
}

#[derive(Debug, Deserialize)]
struct CoinsListResponse {
    data: Vec<CoinData>,
}

// Public data types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingTopic {
    pub topic: String,
    pub title: Option<String>,
    pub topic_rank: Option<u32>,
    pub topic_rank_1h_previous: Option<u32>,
    pub topic_rank_24h_previous: Option<u32>,
    pub num_contributors: Option<u64>,
    pub num_posts: Option<u64>,
    pub interactions_24h: Option<u64>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicSentiment {
    pub topic: String,
    pub title: Option<String>,
    pub topic_rank: Option<u32>,
    pub related_topics: Option<Vec<String>>,
    pub types_count: Option<HashMap<String, u64>>,
    pub types_interactions: Option<HashMap<String, u64>>,
    pub types_sentiment: Option<HashMap<String, u32>>,
    pub types_sentiment_detail: Option<HashMap<String, SentimentDetail>>,
    pub interactions_24h: Option<u64>,
    pub num_contributors: Option<u64>,
    pub num_posts: Option<u64>,
    pub categories: Option<Vec<String>>,
    pub trend: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentDetail {
    pub positive: Option<u64>,
    pub neutral: Option<u64>,
    pub negative: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicSummary {
    pub topic: String,
    pub summary: String,
    pub generated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinData {
    pub id: Option<u64>,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub price: Option<f64>,
    pub price_change_24h: Option<f64>,
    pub market_cap: Option<f64>,
    pub volume_24h: Option<f64>,
    pub galaxy_score: Option<f64>,
    pub alt_rank: Option<u32>,
    pub sentiment: Option<u32>,
    pub social_volume: Option<u64>,
    pub social_dominance: Option<f64>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
