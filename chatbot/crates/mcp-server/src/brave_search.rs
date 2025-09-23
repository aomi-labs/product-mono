//! MCP tool for Brave Search API integration
use rmcp::{
    ErrorData,
    handler::server::tool::Parameters,
    model::{CallToolResult, Content},
    tool,
};
use serde::Deserialize;
use serde_json::Value;

/// Parameters for the Brave Search tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BraveSearchParams {
    #[schemars(description = "The search query to execute")]
    pub q: String,

    #[schemars(description = "Number of results to return (default: 10, max: 20)")]
    pub count: Option<u32>,

    #[schemars(description = "Offset for pagination (default: 0)")]
    pub offset: Option<u32>,

    #[schemars(description = "Language preference (e.g., 'en-US')")]
    pub lang: Option<String>,

    #[schemars(description = "Country preference (e.g., 'US')")]
    pub country: Option<String>,

    #[schemars(description = "Safe search setting: 'off', 'moderate', or 'strict' (default: 'moderate')")]
    pub safesearch: Option<String>,

    #[schemars(description = "Time range filter: 'day', 'week', 'month', 'year'")]
    pub freshness: Option<String>,
}

#[derive(Clone)]
pub struct BraveSearchTool {
    api_key: String,
    client: reqwest::Client,
}

impl BraveSearchTool {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Search the web using Brave Search API
    #[tool(
        description = "Search the web using Brave Search API. Returns web search results including titles, URLs, and descriptions. Rate limited to 1 request per second."
    )]
    pub async fn brave_search(
        &self,
        Parameters(params): Parameters<BraveSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut query_params = vec![("q", params.q)];

        // Add optional parameters
        if let Some(count) = params.count {
            query_params.push(("count", count.to_string()));
        }
        if let Some(offset) = params.offset {
            query_params.push(("offset", offset.to_string()));
        }
        if let Some(lang) = params.lang {
            query_params.push(("lang", lang));
        }
        if let Some(country) = params.country {
            query_params.push(("country", country));
        }
        if let Some(safesearch) = params.safesearch {
            query_params.push(("safesearch", safesearch));
        }
        if let Some(freshness) = params.freshness {
            query_params.push(("freshness", freshness));
        }

        let response = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("Accept", "application/json")
            .header("Accept-Encoding", "gzip")
            .header("X-Subscription-Token", &self.api_key)
            .query(&query_params)
            .send()
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to send request: {e}"), None))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ErrorData::internal_error(format!("Brave Search API error: {status} - {error_text}"), None));
        }

        let result: Value = response
            .json()
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to parse response: {e}"), None))?;

        // Format the results for display
        let mut formatted_results = String::new();

        if let Some(web_results) = result.get("web").and_then(|w| w.get("results")).and_then(|r| r.as_array()) {
            formatted_results.push_str(&format!("Found {} results:\n\n", web_results.len()));

            for (i, result) in web_results.iter().enumerate() {
                if let (Some(title), Some(url), Some(description)) = (
                    result.get("title").and_then(|t| t.as_str()),
                    result.get("url").and_then(|u| u.as_str()),
                    result.get("description").and_then(|d| d.as_str()),
                ) {
                    formatted_results.push_str(&format!("{}. {}\n", i + 1, title));
                    formatted_results.push_str(&format!("   URL: {url}\n"));
                    formatted_results.push_str(&format!("   {description}\n\n"));
                }
            }
        } else {
            formatted_results.push_str("No web results found.");
        }

        // Also include any info boxes if present
        if let Some(infobox) = result.get("infobox") {
            formatted_results.push_str("\nInfo Box:\n");
            formatted_results.push_str(
                &serde_json::to_string_pretty(&infobox).unwrap_or_else(|_| "Unable to format infobox".to_string()),
            );
            formatted_results.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(formatted_results)]))
    }
}
