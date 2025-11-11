use aomi_rag::{DocumentCategory, DocumentStore};
use eyre::Result;
use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub enum LoadingProgress {
    Message(String),
    Complete,
}

#[derive(Debug, Deserialize)]
pub struct SearchDocsInput {
    /// One-line note on what this documentation lookup is for
    pub topic: String,
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
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

impl Tool for SharedDocuments {
    const NAME: &'static str = "search_uniswap_docs";
    type Args = SearchDocsInput;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search Uniswap V2 and V3 documentation for concepts, contracts, and technical details"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this docs search is for"
                    },
                    "query": {
                        "type": "string",
                        "description": "Search query for Uniswap documentation"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Number of results to return (default: 3, max: 10)",
                        "default": 3,
                        "minimum": 1,
                        "maximum": 10
                    }
                },
                "required": ["topic", "query"]
            }),
        }
    }

    async fn call(&self, input: Self::Args) -> Result<Self::Output, Self::Error> {
        let limit = input.limit.min(10);

        let store = self.store.lock().await;
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
}
