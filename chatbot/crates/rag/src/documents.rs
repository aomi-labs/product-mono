use chrono::{DateTime, Utc};
use rig::Embed;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("Failed to parse frontmatter: {0}")]
    FrontmatterParseError(String),

    #[error("Failed to read document: {0}")]
    ReadError(String),

    #[error("Invalid document format: {0}")]
    InvalidFormat(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrontMatter {
    pub id: String,
    pub title: String,
    pub sidebar_position: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentCategory {
    Concepts,
    V2ContractDocumentation,
    V3ContractDocumentation,
    V2Contract,
    V3Contract,
    SwapRouterContract,
}

impl DocumentCategory {
    pub fn from_path(path: &str) -> Option<Self> {
        if path.contains("documents/concepts") {
            Some(DocumentCategory::Concepts)
        } else if path.contains("documents/contracts/v2") {
            Some(DocumentCategory::V2ContractDocumentation)
        } else if path.contains("documents/contracts/v3") {
            Some(DocumentCategory::V3ContractDocumentation)
        } else if path.contains("documents/v2-contracts") {
            Some(DocumentCategory::V2Contract)
        } else if path.contains("documents/v3-contracts") {
            Some(DocumentCategory::V3Contract)
        } else if path.contains("documents/swap-router-contracts") {
            Some(DocumentCategory::SwapRouterContract)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub content: String,
    pub category: DocumentCategory,
    pub file_path: PathBuf,
    pub frontmatter: Option<FrontMatter>,
    pub sidebar_position: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Document {
    pub fn new(file_path: PathBuf, raw_content: String) -> Result<Self, DocumentError> {
        let category = DocumentCategory::from_path(file_path.to_str().unwrap_or(""))
            .ok_or_else(|| DocumentError::InvalidFormat("Unknown document category".to_string()))?;

        // For .sol files, we don't extract frontmatter
        let is_solidity = file_path.extension().and_then(|ext| ext.to_str()).map(|ext| ext == "sol").unwrap_or(false);

        let (frontmatter, content) = if is_solidity {
            (None, raw_content)
        } else {
            Self::extract_frontmatter(&raw_content)?
        };

        let (id, title, sidebar_position) = if let Some(fm) = &frontmatter {
            (fm.id.clone(), fm.title.clone(), fm.sidebar_position)
        } else {
            let default_id = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
            // For Solidity files, use the filename with extension as title
            let default_title = if is_solidity {
                file_path.file_name().and_then(|s| s.to_str()).unwrap_or(&default_id).to_string()
            } else {
                default_id.replace(['-', '_'], " ")
            };
            (default_id, default_title, None)
        };

        let now = Utc::now();

        Ok(Document {
            id,
            title,
            content,
            category,
            file_path,
            frontmatter,
            sidebar_position,
            created_at: now,
            updated_at: now,
        })
    }

    fn extract_frontmatter(content: &str) -> Result<(Option<FrontMatter>, String), DocumentError> {
        let trimmed = content.trim_start();

        if !trimmed.starts_with("---") {
            return Ok((None, content.to_string()));
        }

        let parts: Vec<&str> = trimmed.splitn(3, "---").collect();

        if parts.len() < 3 {
            return Ok((None, content.to_string()));
        }

        let yaml_content = parts[1].trim();

        if yaml_content.is_empty() {
            return Ok((None, parts[2].to_string()));
        }

        match serde_yaml::from_str::<FrontMatter>(yaml_content) {
            Ok(frontmatter) => Ok((Some(frontmatter), parts[2].trim_start().to_string())),
            Err(e) => {
                eprintln!("Warning: Failed to parse frontmatter: {e}");
                Ok((None, content.to_string()))
            }
        }
    }

    pub fn chunk(&self, chunk_size: usize, overlap: usize) -> Vec<DocumentChunk> {
        let mut chunks = Vec::new();
        let content_chars: Vec<char> = self.content.chars().collect();
        let total_chars = content_chars.len();

        if total_chars <= chunk_size {
            chunks.push(DocumentChunk {
                document_id: self.id.clone(),
                chunk_index: 0,
                content: self.content.clone(),
                metadata: ChunkMetadata {
                    document_title: self.title.clone(),
                    document_category: self.category.clone(),
                    file_path: self.file_path.clone(),
                    total_chunks: 1,
                },
            });
            return chunks;
        }

        let mut start = 0;
        let mut chunk_index = 0;

        while start < total_chars {
            let end = std::cmp::min(start + chunk_size, total_chars);
            let chunk_content: String = content_chars[start..end].iter().collect();

            chunks.push(DocumentChunk {
                document_id: self.id.clone(),
                chunk_index,
                content: chunk_content,
                metadata: ChunkMetadata {
                    document_title: self.title.clone(),
                    document_category: self.category.clone(),
                    file_path: self.file_path.clone(),
                    total_chunks: 0,
                },
            });

            chunk_index += 1;

            if end >= total_chars {
                break;
            }

            start = if overlap > 0 && chunk_size > overlap {
                end - overlap
            } else {
                end
            };
        }

        let total_chunks = chunks.len();
        for chunk in &mut chunks {
            chunk.metadata.total_chunks = total_chunks;
        }

        chunks
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Embed, PartialEq, Eq)]
pub struct DocumentChunk {
    pub document_id: String,
    pub chunk_index: usize,
    #[embed]
    pub content: String,
    pub metadata: ChunkMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkMetadata {
    pub document_title: String,
    pub document_category: DocumentCategory,
    pub file_path: PathBuf,
    pub total_chunks: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_frontmatter_with_valid_yaml() {
        let content = r#"---
id: swaps
title: Swaps
sidebar_position: 2
---

# Swaps

This is the content about swaps."#;

        let doc = Document::new(PathBuf::from("documents/concepts/swaps.md"), content.to_string()).unwrap();

        assert_eq!(doc.id, "swaps");
        assert_eq!(doc.title, "Swaps");
        assert_eq!(doc.sidebar_position, Some(2));
        assert!(doc.content.contains("This is the content about swaps"));
        assert!(!doc.content.contains("---"));
    }

    #[test]
    fn test_extract_frontmatter_without_yaml() {
        let content = r#"# UniswapV3Pool

## Functions

This is content without frontmatter."#;

        let doc = Document::new(PathBuf::from("documents/contracts/v3/UniswapV3Pool.md"), content.to_string()).unwrap();

        assert_eq!(doc.id, "UniswapV3Pool");
        assert_eq!(doc.title, "UniswapV3Pool");
        assert_eq!(doc.sidebar_position, None);
        assert!(doc.content.contains("This is content without frontmatter"));
    }

    #[test]
    fn test_category_detection() {
        assert_eq!(DocumentCategory::from_path("documents/concepts/pools.md"), Some(DocumentCategory::Concepts));
        assert_eq!(
            DocumentCategory::from_path("documents/contracts/v2/router.md"),
            Some(DocumentCategory::V2ContractDocumentation)
        );
        assert_eq!(
            DocumentCategory::from_path("documents/contracts/v3/pool.md"),
            Some(DocumentCategory::V3ContractDocumentation)
        );
        assert_eq!(
            DocumentCategory::from_path("documents/v2-contracts/UniswapV2Factory.sol"),
            Some(DocumentCategory::V2Contract)
        );
        assert_eq!(
            DocumentCategory::from_path("documents/v3-contracts/UniswapV3Pool.sol"),
            Some(DocumentCategory::V3Contract)
        );
        assert_eq!(DocumentCategory::from_path("random/path/file.md"), None);
    }

    #[test]
    fn test_document_chunking() {
        let content = "The quick brown fox jumps over the lazy dog. ".repeat(10);
        let doc = Document::new(
            PathBuf::from("documents/concepts/test.md"),
            format!("---\nid: test\ntitle: Test Document\n---\n{}", content),
        )
        .unwrap();

        let chunks = doc.chunk(50, 10);

        assert!(chunks.len() > 1);

        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.document_id, "test");
            assert_eq!(chunk.chunk_index, i);
            assert_eq!(chunk.metadata.document_title, "Test Document");
            assert_eq!(chunk.metadata.total_chunks, chunks.len());
        }
    }

    #[test]
    fn test_document_chunking_small_content() {
        let content = "Small content";
        let doc = Document::new(
            PathBuf::from("documents/concepts/small.md"),
            format!("---\nid: small\ntitle: Small\n---\n{}", content),
        )
        .unwrap();

        let chunks = doc.chunk(100, 10);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, content);
        assert_eq!(chunks[0].metadata.total_chunks, 1);
    }
}
