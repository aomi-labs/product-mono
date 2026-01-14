use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// Metadata about a tool call.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq)]
pub struct CallMetadata {
    /// Tool name
    pub name: String,
    /// Unique identifier for this call instance
    pub id: String,
    /// Optional LLM-provided call ID (for correlation)
    pub call_id: Option<String>,
    /// Whether this is an async/streaming call
    pub is_async: bool,
}

impl CallMetadata {
    pub fn new(name: String, id: String, call_id: Option<String>, is_async: bool) -> Self {
        Self {
            name,
            id,
            call_id,
            is_async,
        }
    }

    pub fn key(&self) -> String {
        format!("{}:{:?}", self.id, self.call_id)
    }
}

impl PartialEq for CallMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.id == other.id
            && self.call_id == other.call_id
            && self.is_async == other.is_async
    }
}

impl Hash for CallMetadata {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.id.hash(state);
        self.call_id.hash(state);
        self.is_async.hash(state);
    }
}

/// Metadata about a registered tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// Tool name
    pub name: String,
    /// Tool namespace (for filtering)
    pub namespace: String,
    /// Human-readable description
    pub description: String,
    /// Whether this tool supports async execution
    pub is_async: bool,
}

impl ToolMetadata {
    pub fn new(name: String, namespace: String, description: String, is_async: bool) -> Self {
        Self {
            name,
            namespace,
            description,
            is_async,
        }
    }
}
