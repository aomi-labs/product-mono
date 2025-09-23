use eyre::Result;
use rag::{DocumentCategory, DocumentStore};
use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

#[derive(Debug, Clone)]
pub enum LoadingProgress {
    Message(String),
    Complete,
}

pub async fn initialize_document_store() -> Result<Arc<Mutex<DocumentStore>>> {
    initialize_document_store_with_progress(None).await
}

pub async fn initialize_document_store_with_progress(
    progress_sender: Option<mpsc::Sender<LoadingProgress>>,
) -> Result<Arc<Mutex<DocumentStore>>> {
    // Helper function to send progress
    async fn send_progress(msg: String, sender: &Option<mpsc::Sender<LoadingProgress>>) {
        if let Some(sender) = sender {
            let _ = sender.send(LoadingProgress::Message(msg)).await;
        } else {
            println!("{msg}");
        }
    }

    send_progress("Loading Uniswap documentation...".to_string(), &progress_sender).await;
    let mut store = DocumentStore::new().await?;

    // Load all documentation directories
    let concepts_count = store.load_directory("documents/concepts", 1000, 100).await?;
    send_progress(format!("  Loaded {concepts_count} chunks from concepts"), &progress_sender).await;

    let v2_docs_count = store.load_directory("documents/contracts/v2", 1000, 100).await?;
    send_progress(format!("  Loaded {v2_docs_count} chunks from V2 docs"), &progress_sender).await;

    let v3_docs_count = store.load_directory("documents/contracts/v3", 1000, 100).await?;
    send_progress(format!("  Loaded {v3_docs_count} chunks from V3 docs"), &progress_sender).await;

    // Load Solidity contract files
    let v2_contracts_count = store.load_directory("documents/v2-contracts", 1500, 150).await?;
    send_progress(format!("  Loaded {v2_contracts_count} chunks from V2 contracts"), &progress_sender).await;

    let v3_contracts_count = store.load_directory("documents/v3-contracts", 1500, 150).await?;
    send_progress(format!("  Loaded {v3_contracts_count} chunks from V3 contracts"), &progress_sender).await;

    let swap_router_count = store.load_directory("documents/swap-router-contracts", 1500, 150).await?;
    send_progress(format!("  Loaded {swap_router_count} chunks from Swap Router contracts"), &progress_sender).await;

    send_progress(format!("Total document chunks indexed: {}", store.document_count()), &progress_sender).await;

    if let Some(sender) = progress_sender {
        let _ = sender.send(LoadingProgress::Complete).await;
    }

    Ok(Arc::new(Mutex::new(store)))
}

#[derive(Debug, Deserialize)]
pub struct SearchDocsInput {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    3
}

#[derive(Clone)]
pub struct SearchUniswapDocs {
    store: Arc<Mutex<DocumentStore>>,
}

impl SearchUniswapDocs {
    pub fn new(store: Arc<Mutex<DocumentStore>>) -> Self {
        Self { store }
    }

    pub async fn new_empty() -> Result<Self> {
        let empty_store = DocumentStore::new().await?;
        Ok(Self {
            store: Arc::new(Mutex::new(empty_store)),
        })
    }
}

impl Tool for SearchUniswapDocs {
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
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, input: Self::Args) -> Result<Self::Output, Self::Error> {
        let limit = input.limit.min(10);

        let store = self.store.lock().await;
        let results = store.search(&input.query, limit).await.map_err(|e| ToolError::ToolCallError(e.into()))?;

        if results.is_empty() {
            return Ok(format!("No documentation found for query: '{}'", input.query));
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
