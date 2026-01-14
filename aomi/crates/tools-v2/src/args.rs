use serde::{Deserialize, Serialize};

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
        Self {
            session_id,
            args,
        }
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
}
