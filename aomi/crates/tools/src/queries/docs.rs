use aomi_rag::{DocumentCategory, DocumentStore};
use eyre::Result;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::oneshot;

use crate::{AomiTool, AomiToolArgs, ToolCallCtx, with_topic};
use serde_json::json;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchDocsInput {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

impl AomiToolArgs for SearchDocsInput {
    fn schema() -> serde_json::Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query for documentation lookup"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of results to return"
                }
            },
            "required": ["query"]
        }))
    }
}

fn default_limit() -> usize {
    3
}

#[derive(Clone)]
pub struct SharedDocuments {
    store: Arc<Mutex<DocumentStore>>,
}

impl SharedDocuments {
    pub fn new(store: Arc<Mutex<DocumentStore>>) -> Self {
        Self { store }
    }

    pub fn get_store(&self) -> Arc<Mutex<DocumentStore>> {
        self.store.clone()
    }
}

pub async fn execute_call(
    tool: &SharedDocuments,
    input: SearchDocsInput,
) -> Result<String, ToolError> {
    let limit = input.limit.min(10);

    let store = tool.store.lock().await;
    let results = store
        .search(&input.query, limit)
        .await
        .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

    if results.is_empty() {
        return Ok(format!(
            "No documentation found for query: '{}'",
            input.query
        ));
    }

    let mut output = String::new();

    for result in results.iter() {
        let category = match result.chunk.metadata.document_category {
            DocumentCategory::Concepts => "Concepts",
            DocumentCategory::V2ContractDocumentation => "V2 Docs",
            DocumentCategory::V3ContractDocumentation => "V3 Docs",
            DocumentCategory::V2Contract => "V2 Code",
            DocumentCategory::V3Contract => "V3 Code",
            DocumentCategory::SwapRouterContract => "SwapRouter Code",
        };

        // Concise format: [Category] Title (score)
        output.push_str(&format!(
            "[{}] {} ({:.2})\n{}\n\n",
            category, result.chunk.metadata.document_title, result.score, result.chunk.content
        ));
    }

    Ok(output.trim_end().to_string())
}

impl AomiTool for SharedDocuments {
    const NAME: &'static str = "search_docs";
    const NAMESPACE: &'static str = "docs";

    type Args = SearchDocsInput;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Search documentation sources for relevant passages."
    }

    fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<serde_json::Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        let tool = self.clone();
        async move {
            let result = execute_call(&tool, args)
                .await
                .map(serde_json::Value::String)
                .map_err(|e| eyre::eyre!(e.to_string()));
            let _ = sender.send(result);
        }
    }
}
