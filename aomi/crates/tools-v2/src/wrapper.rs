use crate::{AomiTool, AomiToolArgs, CallMetadata};
use eyre::Result as EyreResult;
use rig::completion::ToolDefinition;
use rig::tool::{Tool, ToolError};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

/// Wrapper type to enable auto-impl of rig::Tool for AomiTool.
///
/// This solves the Rust orphan rule problem - we own this type, so we can
/// implement foreign traits (rig::Tool) on it even though we don't own
/// either the AomiTool trait or the Tool trait.
///
/// This is a zero-cost abstraction (newtype pattern) that simply wraps
/// the underlying tool.
#[derive(Clone)]
pub struct AomiToolWrapper<T: AomiTool> {
    pub inner: T,
}

impl<T: AomiTool> AomiToolWrapper<T> {
    /// Create a new wrapper around a tool
    pub fn new(tool: T) -> Self {
        Self { inner: tool }
    }
}

/// Auto-implementation of rig::Tool for any AomiTool.
///
/// This is where the magic happens - any type implementing AomiTool
/// automatically gets a rig::Tool implementation via this wrapper,
/// enabling unified execution through Rig's infrastructure.
///
/// The implementation:
/// 1. Wraps args with session_id
/// 2. Checks if tool supports async
/// 3. Spawns execution (sync or async)
/// 4. Returns immediate "queued" response
/// 5. (Future: registers with session handler for polling)
impl<T: AomiTool> Tool for AomiToolWrapper<T> {
    const NAME: &'static str = T::NAME;

    type Args = AomiToolArgs<T::Args>;
    type Output = Value;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let mut schema = self.inner.parameters_schema();

        // Inject session_id into schema (optional, auto-provided)
        if let Some(obj) = schema.as_object_mut()
            && let Some(props) = obj.get_mut("properties").and_then(|v| v.as_object_mut())
        {
            props.insert(
                "session_id".to_string(),
                json!({
                    "type": "string",
                    "description": "Internal session identifier (auto-injected)"
                }),
            );
        }

        ToolDefinition {
            name: Self::NAME.to_string(),
            description: self.inner.description().to_string(),
            parameters: schema,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let _session_id = args.session_id();
        let tool_args = args.args;

        // Generate unique call_id
        let call_id = format!("{}_{}", T::NAME, uuid::Uuid::new_v4());

        // TODO Phase 1: Get scheduler and session handler
        // let scheduler = ToolScheduler::get_or_init().await
        //     .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        // let handler = scheduler.get_session_handler(session_id.clone(), namespaces);

        // Create metadata (unused in Phase 0, will be used in Phase 1)
        let _metadata = CallMetadata::new(
            T::NAME.to_string(),
            call_id.clone(),
            None, // TODO: Extract from Rig context when available
            self.inner.support_async(),
        );

        // Execute based on sync/async support
        if self.inner.support_async() {
            // Async: spawn thread, stream results via mpsc
            let (tx, mut rx) = mpsc::channel::<EyreResult<Value>>(100);
            let tool = self.inner.clone();

            tokio::spawn(async move {
                tool.run_async(tx, tool_args).await;
            });

            // TODO Phase 1: Create ToolReceiver and register with handler
            // let tool_receiver = ToolReceiver::new_multi_step(metadata.clone(), rx);
            // handler.lock().await.register_receiver(tool_receiver);

            // For Phase 0: Just collect first result to prove it works
            match rx.recv().await {
                Some(Ok(value)) => Ok(value),
                Some(Err(e)) => Err(ToolError::ToolCallError(e.to_string().into())),
                None => Err(ToolError::ToolCallError("Channel closed".into())),
            }
        } else {
            // Sync: spawn thread, single result via oneshot
            let (tx, rx) = oneshot::channel::<EyreResult<Value>>();
            let tool = self.inner.clone();

            tokio::spawn(async move {
                tool.run_sync(tx, tool_args).await;
            });

            // TODO Phase 1: Create ToolReceiver and register with handler
            // let tool_receiver = ToolReceiver::new_single(metadata.clone(), rx);
            // handler.lock().await.register_receiver(tool_receiver);

            // For Phase 0: Just await the result to prove it works
            match rx.await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(e)) => Err(ToolError::ToolCallError(e.to_string().into())),
                Err(_) => Err(ToolError::ToolCallError("Channel closed".into())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::fmt;

    // Simple error type for testing
    #[derive(Debug, Clone)]
    struct MockError(String);

    impl fmt::Display for MockError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "MockError: {}", self.0)
        }
    }

    impl std::error::Error for MockError {}

    // Mock sync tool for testing
    #[derive(Clone)]
    struct MockSyncTool;

    #[derive(Debug, Clone, Deserialize, Serialize)]
    struct MockArgs {
        pub value: i32,
    }

    impl AomiTool for MockSyncTool {
        const NAME: &'static str = "mock_sync";
        const NAMESPACE: &'static str = "test";

        type Args = MockArgs;
        type Output = Value;
        type Error = MockError;

        fn support_async(&self) -> bool {
            false
        }

        fn description(&self) -> &'static str {
            "A mock sync tool for testing"
        }

        fn parameters_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "integer",
                        "description": "An integer value"
                    }
                },
                "required": ["value"]
            })
        }

        async fn run_sync(
            &self,
            result_sender: oneshot::Sender<EyreResult<Value>>,
            args: Self::Args,
        ) {
            let result = json!({
                "result": args.value * 2
            });
            let _ = result_sender.send(Ok(result));
        }
    }

    // Mock async tool for testing
    #[derive(Clone)]
    struct MockAsyncTool;

    impl AomiTool for MockAsyncTool {
        const NAME: &'static str = "mock_async";
        const NAMESPACE: &'static str = "test";

        type Args = MockArgs;
        type Output = Value;
        type Error = MockError;

        fn support_async(&self) -> bool {
            true
        }

        fn description(&self) -> &'static str {
            "A mock async tool for testing"
        }

        fn parameters_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "integer",
                        "description": "An integer value"
                    }
                },
                "required": ["value"]
            })
        }

        async fn run_async(
            &self,
            results_sender: mpsc::Sender<EyreResult<Value>>,
            args: Self::Args,
        ) {
            // Send multiple progress updates
            for i in 0..3 {
                let result = json!({
                    "step": i,
                    "value": args.value + i
                });
                let _ = results_sender.send(Ok(result)).await;
            }
            // Channel closes when sender is dropped
        }
    }

    #[tokio::test]
    async fn test_sync_tool_wrapper() {
        let tool = AomiToolWrapper::new(MockSyncTool);
        let args = AomiToolArgs::new(Some("test_session".to_string()), MockArgs { value: 10 });

        let result = tool.call(args).await.expect("Tool call should succeed");

        assert_eq!(result["result"], 20);
    }

    #[tokio::test]
    async fn test_async_tool_wrapper() {
        let tool = AomiToolWrapper::new(MockAsyncTool);
        let args = AomiToolArgs::new(Some("test_session".to_string()), MockArgs { value: 5 });

        // For Phase 0, we just get the first result
        let result = tool.call(args).await.expect("Tool call should succeed");

        assert_eq!(result["step"], 0);
        assert_eq!(result["value"], 5);
    }

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = AomiToolWrapper::new(MockSyncTool);
        let definition = tool.definition(String::new()).await;

        assert_eq!(definition.name, "mock_sync");
        assert_eq!(definition.description, "A mock sync tool for testing");

        // Verify session_id was injected
        let props = definition.parameters["properties"].as_object().unwrap();
        assert!(props.contains_key("session_id"));
        assert!(props.contains_key("value"));
    }
}
