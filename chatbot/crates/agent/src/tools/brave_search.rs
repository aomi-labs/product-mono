use once_cell::sync::Lazy;
use rig_derive::rig_tool;
use std::sync::Arc;

static BRAVE_API_KEY: Lazy<Option<String>> =
    Lazy::new(|| std::env::var("BRAVE_SEARCH_API_KEY").ok());
static BRAVE_CLIENT: Lazy<Arc<reqwest::Client>> =
    Lazy::new(|| Arc::new(reqwest::Client::new()));

fn tool_error(message: impl Into<String>) -> rig::tool::ToolError {
    rig::tool::ToolError::ToolCallError(message.into().into())
}

#[rig_tool(
    description = "Search the web using the Brave Search API. Returns formatted results with titles, URLs, and descriptions (rate-limited to ~1 req/s).",
    params(
        query = "Search query string",
        count = "Optional number of results to return (default Brave behaviour, max 20)",
        offset = "Optional offset for pagination",
        lang = "Optional language preference (e.g., 'en-US')",
        country = "Optional country preference (e.g., 'US')",
        safesearch = "Optional safesearch level: 'off', 'moderate', or 'strict'",
        freshness = "Optional freshness filter: 'day', 'week', 'month', 'year'"
    ),
    required(query)
)]
pub async fn brave_search(
    query: String,
    count: Option<u32>,
    offset: Option<u32>,
    lang: Option<String>,
    country: Option<String>,
    safesearch: Option<String>,
    freshness: Option<String>,
) -> Result<String, rig::tool::ToolError> {
    let api_key = BRAVE_API_KEY
        .as_ref()
        .cloned()
        .ok_or_else(|| tool_error("BRAVE_SEARCH_API_KEY is not set in the environment"))?;

    let mut query_params = vec![("q".to_string(), query)];

    if let Some(value) = count {
        query_params.push(("count".to_string(), value.to_string()));
    }
    if let Some(value) = offset {
        query_params.push(("offset".to_string(), value.to_string()));
    }
    if let Some(value) = lang {
        query_params.push(("lang".to_string(), value));
    }
    if let Some(value) = country {
        query_params.push(("country".to_string(), value));
    }
    if let Some(value) = safesearch {
        query_params.push(("safesearch".to_string(), value));
    }
    if let Some(value) = freshness {
        query_params.push(("freshness".to_string(), value));
    }

    let client = BRAVE_CLIENT.clone();
    let response = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .header("Accept", "application/json")
        .header("Accept-Encoding", "gzip")
        .header("X-Subscription-Token", api_key.as_str())
        .query(&query_params)
        .send()
        .await
        .map_err(|e| tool_error(format!("Failed to contact Brave Search API: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(tool_error(format!(
            "Brave Search API error {status}: {body}"
        )));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| tool_error(format!("Failed to parse Brave Search response: {e}")))?;

    let mut formatted = String::new();
    if let Some(web_results) = result
        .get("web")
        .and_then(|w| w.get("results"))
        .and_then(|r| r.as_array())
    {
        formatted.push_str(&format!("Found {} results:\n\n", web_results.len()));
        for (index, entry) in web_results.iter().enumerate() {
            if let (Some(title), Some(url)) = (
                entry.get("title").and_then(|t| t.as_str()),
                entry.get("url").and_then(|u| u.as_str()),
            ) {
                let description = entry
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("No description provided.");
                formatted.push_str(&format!("{}. {}\n", index + 1, title));
                formatted.push_str(&format!("   URL: {url}\n"));
                formatted.push_str(&format!("   {description}\n\n"));
            }
        }
    } else {
        formatted.push_str("No web results found.");
    }

    if let Some(infobox) = result.get("infobox") {
        formatted.push_str("\nInfo Box:\n");
        formatted.push_str(
            &serde_json::to_string_pretty(infobox)
                .unwrap_or_else(|_| "Unable to format infobox".to_string()),
        );
        formatted.push('\n');
    }

    Ok(formatted.trim_end().to_string())
}

impl_rig_tool_clone!(
    BraveSearch,
    BraveSearchParameters,
    [query, count, offset, lang, country, safesearch, freshness]
);
