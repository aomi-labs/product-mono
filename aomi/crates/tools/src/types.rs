use eyre::Result as EyreResult;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::hash::{Hash, Hasher};
use tokio::sync::mpsc;

/// Metadata about a tool call.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq)]
pub struct CallMetadata {
    /// Tool name
    pub name: String,
    /// Tool namespace
    pub namespace: String,
    /// Unique identifier for this call instance
    pub id: String,
    /// Optional LLM-provided call ID (for correlation)
    pub call_id: Option<String>,
    /// Whether this is an async/streaming call
    pub is_async: bool,
}

impl CallMetadata {
    pub fn new(
        name: String,
        namespace: String,
        id: String,
        call_id: Option<String>,
        is_async: bool,
    ) -> Self {
        Self {
            name,
            namespace,
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
            && self.namespace == other.namespace
            && self.id == other.id
            && self.call_id == other.call_id
            && self.is_async == other.is_async
    }
}

impl Hash for CallMetadata {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.namespace.hash(state);
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

/// Runtime context for a tool call.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCallCtx {
    pub session_id: String,
    pub metadata: CallMetadata,
}

/// Envelope passed to tools from the completion layer.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeEnvelope<T> {
    pub ctx: ToolCallCtx,
    pub args: T,
}

/// User-only tool argument contract.
pub trait AomiToolArgs: DeserializeOwned + Send + Sync + 'static {
    fn schema() -> Value;
}

/// Wrapper that automatically handles the `topic` field injected by `add_topic`.
/// Use this as your Args type to auto-strip the topic during deserialization.
#[derive(Debug, Clone, Serialize)]
pub struct WithTopic<T> {
    /// One-liner topic of this operation (auto-injected by schema)
    #[serde(default)]
    pub topic: Option<String>,
    /// The actual tool arguments
    #[serde(flatten)]
    pub inner: T,
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for WithTopic<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as a map first to extract topic
        let mut map: serde_json::Map<String, Value> = serde_json::Map::deserialize(deserializer)?;

        // Extract topic if present
        let topic = map
            .remove("topic")
            .and_then(|v| v.as_str().map(String::from));

        // Deserialize remaining fields into inner type
        let inner = T::deserialize(Value::Object(map)).map_err(serde::de::Error::custom)?;

        Ok(WithTopic { topic, inner })
    }
}

impl<T: AomiToolArgs> AomiToolArgs for WithTopic<T> {
    fn schema() -> Value {
        with_topic(T::schema())
    }
}

pub fn with_topic(mut schema: Value) -> Value {
    let obj = schema.as_object_mut().expect("schema must be an object");
    let props = obj
        .entry("properties")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("schema properties must be an object");

    props.insert(
        "topic".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "One-liner topic of this operation"
        }),
    );

    let required = obj
        .entry("required")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .expect("schema required must be an array");

    if !required.iter().any(|v| v == "topic") {
        required.push(serde_json::json!("topic"));
    }

    schema
}

/// Core trait for Aomi tools - supports both sync and async execution patterns.
pub trait AomiTool: Send + Sync + Clone + 'static {
    /// Tool's unique name (used for LLM tool calls)
    const NAME: &'static str;

    /// Tool's namespace for organization and access control
    const NAMESPACE: &'static str = "default";

    /// Request type - must be deserializable from LLM JSON args
    type Args: AomiToolArgs;

    /// Response type - must be serializable to JSON
    type Output: Serialize + Send + Sync + 'static;

    /// Error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Whether this tool supports async/streaming results.
    /// - false: Single result via `run_sync()`
    /// - true: Multiple results via `run_async()`
    fn support_async(&self) -> bool {
        false
    }

    /// Alias for async capability checks.
    fn is_async(&self) -> bool {
        self.support_async()
    }

    /// Get tool description for LLM (displayed in tool definition)
    fn description(&self) -> &'static str;

    /// Get JSON schema for arguments (OpenAPI-style)
    fn parameters_schema(&self) -> Value {
        Self::Args::schema()
    }

    /// Get tool metadata for registration
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata::new(
            Self::NAME.to_string(),
            Self::NAMESPACE.to_string(),
            self.description().to_string(),
            self.support_async(),
        )
    }

    /// Execute synchronously - returns a single result directly.
    ///
    /// For tools that complete quickly and return a single value.
    ///
    /// Default implementation returns an error indicating sync is not supported.
    #[allow(clippy::manual_async_fn)]
    fn run_sync(
        &self,
        _ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl Future<Output = EyreResult<Value>> + Send {
        async move {
            Err(eyre::eyre!(
                "Tool {} does not support sync execution",
                Self::NAME
            ))
        }
    }

    /// Execute asynchronously - streams multiple results via mpsc channel.
    ///
    /// For tools that:
    /// - Take time to complete
    /// - Have multiple progress updates
    /// - Need to stream results incrementally
    ///
    /// The tool owns the sender and can send multiple values before dropping it.
    /// Dropping the sender signals completion to the receiver.
    ///
    /// Default implementation does nothing (no async support).
    fn run_async(
        &self,
        _results_sender: mpsc::Sender<EyreResult<Value>>,
        _ctx: ToolCallCtx,
        _args: Self::Args,
    ) -> impl Future<Output = ()> + Send {
        async move {
            // Default: no async support
            // Async tools must override this
        }
    }
}

/// Format tool name for display.
/// Converts snake_case to readable format (e.g., "encode_function_call" -> "Encode function call")
pub fn format_tool_name(name: &str) -> String {
    let words: Vec<String> =
        if name.contains('_') && name.chars().all(|c| c.is_lowercase() || c == '_') {
            name.split('_')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        } else {
            let mut words = Vec::new();
            let mut word = String::new();
            for (i, ch) in name.chars().enumerate() {
                if ch.is_uppercase() && i > 0 && !word.is_empty() {
                    words.push(word);
                    word = String::new();
                }
                word.push(ch);
            }
            if !word.is_empty() {
                words.push(word);
            }
            words
        };

    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let w = w.to_lowercase();
            if i == 0 {
                let mut chars = w.chars();
                chars.next().map_or_else(String::new, |c| {
                    c.to_uppercase().to_string() + chars.as_str()
                })
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[allow(dead_code)]
    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct TestArgs {
        pub query: String,
    }

    impl AomiToolArgs for TestArgs {
        fn schema() -> Value {
            with_topic(json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Test query"
                    }
                },
                "required": ["query"]
            }))
        }
    }

    #[test]
    fn test_format_tool_name_snake_case() {
        assert_eq!(format_tool_name("get_current_time"), "Get current time");
        assert_eq!(
            format_tool_name("encode_function_call"),
            "Encode function call"
        );
        assert_eq!(format_tool_name("simple"), "Simple");
    }

    #[test]
    fn test_format_tool_name_camel_case() {
        assert_eq!(format_tool_name("GetCurrentTime"), "Get current time");
        assert_eq!(format_tool_name("SimpleTest"), "Simple test");
    }
}
