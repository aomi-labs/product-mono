use std::path::Path;

use crate::{
    documents::{Document, DocumentChunk},
    embeddings::{EmbeddingClient, EmbeddingError},
};
use rig::vector_store::in_memory_store::InMemoryVectorStore;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VectorStoreError {
    #[error("Embedding error: {0}")]
    EmbeddingError(#[from] EmbeddingError),

    #[error("Document error: {0}")]
    DocumentError(#[from] crate::documents::DocumentError),

    #[error("Search error: {0}")]
    SearchError(String),

    #[error("Store error: {0}")]
    StoreError(String),
}

pub struct DocumentStore {
    embedding_client: EmbeddingClient,
    store: InMemoryVectorStore<DocumentChunk>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub chunk: DocumentChunk,
    pub score: f64,
}

impl DocumentStore {
    pub async fn new() -> Result<Self, VectorStoreError> {
        let embedding_client = EmbeddingClient::new().await?;
        let store = InMemoryVectorStore::from_documents(vec![]);

        Ok(Self {
            embedding_client,
            store,
        })
    }

    pub async fn add_document(
        &mut self,
        document: Document,
        chunk_size: usize,
        overlap: usize,
    ) -> Result<usize, VectorStoreError> {
        let chunks = document.chunk(chunk_size, overlap);
        let num_chunks = chunks.len();

        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let embeddings = self.embedding_client.embed_batch(texts).await?;

        use rig::OneOrMany;

        let chunk_embeddings: Vec<(DocumentChunk, OneOrMany<rig::embeddings::Embedding>)> = chunks
            .into_iter()
            .zip(embeddings.into_iter().map(OneOrMany::one))
            .collect();

        self.store.add_documents(chunk_embeddings);

        Ok(num_chunks)
    }

    pub async fn add_documents(
        &mut self,
        documents: Vec<Document>,
        chunk_size: usize,
        overlap: usize,
    ) -> Result<usize, VectorStoreError> {
        let mut total_chunks = 0;

        for document in documents {
            let chunks_added = self.add_document(document, chunk_size, overlap).await?;
            total_chunks += chunks_added;
        }

        Ok(total_chunks)
    }

    pub async fn load_directory(
        &mut self,
        dir_path: &str,
        chunk_size: usize,
        overlap: usize,
    ) -> Result<usize, VectorStoreError> {
        use std::path::Path;

        let mut documents = Vec::new();
        self.load_directory_recursive(Path::new(dir_path), &mut documents)?;

        self.add_documents(documents, chunk_size, overlap).await
    }

    #[allow(clippy::only_used_in_recursion)]
    fn load_directory_recursive(
        &self,
        dir: &Path,
        documents: &mut Vec<Document>,
    ) -> Result<(), VectorStoreError> {
        use std::fs;

        let paths = fs::read_dir(dir)
            .map_err(|e| VectorStoreError::StoreError(format!("Failed to read directory: {e}")))?;

        for path in paths {
            let path = path
                .map_err(|e| VectorStoreError::StoreError(format!("Failed to read path: {e}")))?;
            let path = path.path();

            if path.is_dir() {
                // Recursively load subdirectories
                self.load_directory_recursive(&path, documents)?;
            } else {
                let extension = path.extension().and_then(|s| s.to_str());
                if extension == Some("md") || extension == Some("sol") {
                    let content = fs::read_to_string(&path).map_err(|e| {
                        VectorStoreError::StoreError(format!("Failed to read file: {e}"))
                    })?;

                    let document = Document::new(path, content)?;
                    documents.push(document);
                }
            }
        }

        Ok(())
    }

    pub async fn search(
        &self,
        query: &str,
        top_n: usize,
    ) -> Result<Vec<SearchResult>, VectorStoreError> {
        use rig::vector_store::VectorStoreIndex;
        use rig::vector_store::request::VectorSearchRequest;

        let index = self
            .store
            .clone()
            .index(self.embedding_client.model().clone());

        let request = VectorSearchRequest::builder()
            .query(query)
            .samples(top_n as u64)
            .build()
            .map_err(|e| VectorStoreError::SearchError(e.to_string()))?;

        let results: Vec<(f64, String, DocumentChunk)> = index
            .top_n(request)
            .await
            .map_err(|e| VectorStoreError::SearchError(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|(score, _id, chunk)| SearchResult { chunk, score })
            .collect())
    }

    pub async fn search_with_threshold(
        &self,
        query: &str,
        top_n: usize,
        threshold: f64,
    ) -> Result<Vec<SearchResult>, VectorStoreError> {
        let results = self.search(query, top_n).await?;

        Ok(results
            .into_iter()
            .filter(|r| r.score >= threshold)
            .collect())
    }

    pub fn document_count(&self) -> usize {
        self.store.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::LazyLock;
    use tokio::sync::Mutex;

    static FASTEMBED_TEST_GUARD: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[tokio::test]
    async fn test_document_store_creation() {
        let _guard = FASTEMBED_TEST_GUARD.lock().await;
        let store = DocumentStore::new().await;
        assert!(store.is_ok());
        let store = store.unwrap();
        assert_eq!(store.document_count(), 0);
    }

    #[tokio::test]
    async fn test_add_document() {
        let _guard = FASTEMBED_TEST_GUARD.lock().await;
        let mut store = DocumentStore::new().await.unwrap();

        let content = r#"---
id: test-doc
title: Test Document
---

This is a test document with some content that should be chunked and embedded."#;

        let doc = Document::new(
            PathBuf::from("documents/concepts/test.md"),
            content.to_string(),
        )
        .unwrap();

        let chunks_added = store.add_document(doc, 100, 10).await.unwrap();
        assert!(chunks_added > 0);
        assert_eq!(store.document_count(), chunks_added);
    }

    #[tokio::test]
    async fn test_search() {
        let _guard = FASTEMBED_TEST_GUARD.lock().await;
        let mut store = DocumentStore::new().await.unwrap();

        let content = r#"---
id: swaps
title: Swaps in Uniswap
---

Uniswap allows users to swap tokens directly from their wallets.
The protocol uses an automated market maker (AMM) design.
Liquidity providers earn fees from trades."#;

        let doc = Document::new(
            PathBuf::from("documents/concepts/swaps.md"),
            content.to_string(),
        )
        .unwrap();

        store.add_document(doc, 100, 10).await.unwrap();

        let results = store.search("how do swaps work", 5).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].score > 0.0);
    }
}
