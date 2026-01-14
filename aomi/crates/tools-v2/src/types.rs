use serde::{Deserialize, Serialize};

/// Metadata about a tool call, replaces ToolCallId with richer information
#[derive(Debug, Clone, Serialize, Deserialize)]
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
