use eyre::Result as EyreResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::future::Future;
use tokio::sync::{mpsc, oneshot};

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

/// Wrapper for tool arguments that injects session context for session-aware execution.
///
/// These fields are auto-injected by the completion layer.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AomiToolArgs<T> {
    /// Session ID (auto-injected by completion layer)
    pub session_id: String,

    /// Actual tool arguments (flattened into the same object)
    #[serde(flatten)]
    pub args: T,
}

impl<T> AomiToolArgs<T> {
    /// Create new args with session context
    pub fn new(session_id: String, args: T) -> Self {
        Self { session_id, args }
    }

    /// Get session ID
    pub fn session_id(&self) -> String {
        self.session_id.clone()
    }

    /// Unwrap to get inner args
    pub fn into_inner(self) -> T {
        self.args
    }
}

/// Core trait for Aomi tools - supports both sync and async execution patterns.
pub trait AomiTool: Send + Sync + Clone + 'static {
    /// Tool's unique name (used for LLM tool calls)
    const NAME: &'static str;

    /// Tool's namespace for organization and access control
    const NAMESPACE: &'static str = "default";

    /// Request type - must be deserializable from LLM JSON args
    type Args: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static;

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
    fn parameters_schema(&self) -> Value;

    /// Execute synchronously - sends one result via oneshot channel.
    ///
    /// For tools that complete quickly and return a single value.
    /// The implementation should send the result through the channel and return.
    ///
    /// Default implementation returns an error indicating sync is not supported.
    fn run_sync(
        &self,
        result_sender: oneshot::Sender<EyreResult<Value>>,
        _args: Self::Args,
    ) -> impl Future<Output = ()> + Send {
        async move {
            let _ = result_sender.send(Err(eyre::eyre!(
                "Tool {} does not support sync execution",
                Self::NAME
            )));
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
        _args: Self::Args,
    ) -> impl Future<Output = ()> + Send {
        async move {
            // Default: no async support
            // Async tools must override this
        }
    }

    /// Optional: custom topic for UI display.
    /// Defaults to formatted tool name.
    fn topic(&self) -> String {
        format_tool_name(Self::NAME)
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

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct TestArgs {
        pub topic: String,
    }

    #[test]
    fn test_args_with_session_id() {
        let args = AomiToolArgs::new(
            "session_123".to_string(),
            TestArgs {
                topic: "test".to_string(),
            },
        );

        assert_eq!(args.session_id(), "session_123");
        assert_eq!(args.args.topic, "test");
    }

    #[test]
    fn test_args_serialization() {
        let args = AomiToolArgs::new(
            "session_123".to_string(),
            TestArgs {
                topic: "test".to_string(),
            },
        );

        let json = serde_json::to_value(&args).unwrap();
        assert_eq!(
            json,
            json!({
                "session_id": "session_123",
                "topic": "test"
            })
        );
    }

    #[test]
    fn test_args_deserialization() {
        let json = json!({
            "session_id": "session_123",
            "topic": "test"
        });

        let args: AomiToolArgs<TestArgs> = serde_json::from_value(json).unwrap();
        assert_eq!(args.session_id(), "session_123");
        assert_eq!(args.args.topic, "test");
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
