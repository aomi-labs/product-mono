use eyre::Result as EyreResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use tokio::sync::{mpsc, oneshot};

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
