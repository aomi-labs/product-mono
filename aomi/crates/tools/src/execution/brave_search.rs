use crate::clients::external_clients;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BraveSearchParameters {
    pub topic: String,
    pub query: String,
    pub count: Option<u32>,
    pub offset: Option<u32>,
    pub lang: Option<String>,
    pub country: Option<String>,
    pub safesearch: Option<String>,
    pub freshness: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BraveSearch;

pub async fn execute_call(args: BraveSearchParameters) -> Result<String, ToolError> {
    let query_for_log = args.query.clone();

    info!(
        target: "aomi_tools::brave_search",
        query = %query_for_log,
        count = args.count.unwrap_or(0),
        offset = args.offset.unwrap_or(0),
        lang = args.lang.as_deref().unwrap_or("default"),
        country = args.country.as_deref().unwrap_or("default"),
        safesearch = args.safesearch.as_deref().unwrap_or("default"),
        freshness = args.freshness.as_deref().unwrap_or("default"),
        "Invoking Brave search"
    );

    let clients = external_clients().await;
    let base_request = clients.brave_request().ok_or_else(|| {
        ToolError::ToolCallError("BRAVE_SEARCH_API_KEY is not set in the environment".into())
    })?;

    let mut query_params = vec![("q".to_string(), args.query)];
    if let Some(value) = args.count {
        query_params.push(("count".to_string(), value.to_string()));
    }
    if let Some(value) = args.offset {
        query_params.push(("offset".to_string(), value.to_string()));
    }
    if let Some(value) = args.lang {
        query_params.push(("lang".to_string(), value));
    }
    if let Some(value) = args.country {
        query_params.push(("country".to_string(), value));
    }
    if let Some(value) = args.safesearch {
        query_params.push(("safesearch".to_string(), value));
    }
    if let Some(value) = args.freshness {
        query_params.push(("freshness".to_string(), value));
    }

    let response = base_request
        .try_clone()
        .unwrap_or_else(|| {
            crate::clients::build_http_client().get(crate::clients::BRAVE_SEARCH_URL)
        })
        .query(&query_params)
        .send()
        .await
        .map_err(|e| {
            warn!(
                target: "aomi_tools::brave_search",
                query = %query_for_log,
                error = %e,
                "Brave search request failed"
            );
            ToolError::ToolCallError(format!("Failed to contact Brave Search API: {e}").into())
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        warn!(
            target: "aomi_tools::brave_search",
            query = %query_for_log,
            status = %status,
            body = %body,
            "Brave search returned error response"
        );
        return Err(ToolError::ToolCallError(
            format!("Brave Search API error {status}: {body}").into(),
        ));
    }

    let result: serde_json::Value = response.json().await.map_err(|e| {
        warn!(
            target: "aomi_tools::brave_search",
            query = %query_for_log,
            error = %e,
            "Failed to parse Brave search response"
        );
        ToolError::ToolCallError(format!("Failed to parse Brave Search response: {e}").into())
    })?;

    let mut formatted = String::new();
    let mut result_count = 0usize;
    if let Some(web_results) = result
        .get("web")
        .and_then(|w| w.get("results"))
        .and_then(|r| r.as_array())
    {
        result_count = web_results.len();
        formatted.push_str(&format!("Found {} results:\n\n", result_count));
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

    info!(
        target: "aomi_tools::brave_search",
        query = %query_for_log,
        result_count,
        "Brave search completed"
    );

    Ok(formatted.trim_end().to_string())
}
