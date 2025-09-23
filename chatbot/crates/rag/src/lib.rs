pub mod documents;
pub mod embeddings;
pub mod vector_store;

pub use documents::{ChunkMetadata, Document, DocumentCategory, DocumentChunk, DocumentError, FrontMatter};
pub use embeddings::{EmbeddingClient, EmbeddingError};
pub use vector_store::{DocumentStore, SearchResult, VectorStoreError};
