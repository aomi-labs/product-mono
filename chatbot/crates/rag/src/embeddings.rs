use rig::embeddings::{Embedding, embedding::EmbeddingModelDyn};
use rig_fastembed::{Client, EmbeddingModel, FastembedModel};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("Failed to initialize embedding client: {0}")]
    InitializationError(String),

    #[error("Failed to generate embeddings: {0}")]
    GenerationError(String),
}

pub struct EmbeddingClient {
    model: EmbeddingModel,
}

impl EmbeddingClient {
    pub async fn new() -> Result<Self, EmbeddingError> {
        let client = Client::new();
        let model = client.embedding_model(&FastembedModel::BGEBaseENV15);

        Ok(Self { model })
    }

    pub async fn embed(&self, text: &str) -> Result<Embedding, EmbeddingError> {
        self.model
            .embed_text(text)
            .await
            .map_err(|e| EmbeddingError::GenerationError(e.to_string()))
    }

    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Embedding>, EmbeddingError> {
        let mut embeddings = Vec::new();

        for text in texts {
            let embedding = self.embed(&text).await?;
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }

    pub fn embedding_dim(&self) -> usize {
        768
    }

    pub fn model(&self) -> &EmbeddingModel {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_client_creation() {
        let client = EmbeddingClient::new().await;
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_embed_single_text() {
        let client = EmbeddingClient::new().await.unwrap();
        let embedding = client.embed("Hello, world!").await;

        assert!(embedding.is_ok());
        let embedding = embedding.unwrap();
        assert_eq!(embedding.vec.len(), 768);
    }

    #[tokio::test]
    async fn test_embed_batch() {
        let client = EmbeddingClient::new().await.unwrap();
        let texts = vec![
            "First text".to_string(),
            "Second text".to_string(),
            "Third text".to_string(),
        ];

        let embeddings = client.embed_batch(texts.clone()).await;

        assert!(embeddings.is_ok());
        let embeddings = embeddings.unwrap();
        assert_eq!(embeddings.len(), texts.len());

        for embedding in embeddings {
            assert_eq!(embedding.vec.len(), 768);
        }
    }
}
