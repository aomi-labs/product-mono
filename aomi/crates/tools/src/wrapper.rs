use crate::scheduler::ToolScheduler;
use crate::streams::ToolReciever;
use crate::{AomiTool, CallMetadata, RuntimeEnvelope, ToolCallCtx};
use eyre::Result as EyreResult;
use rig::completion::ToolDefinition;
use rig::tool::{Tool, ToolError};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct AomiToolWrapper<T: AomiTool> {
    pub inner: T,
}

impl<T: AomiTool> AomiToolWrapper<T> {
    pub fn new(tool: T) -> Self {
        Self { inner: tool }
    }
}

impl<T: AomiTool> Tool for AomiToolWrapper<T> {
    const NAME: &'static str = T::NAME;

    type Args = RuntimeEnvelope<T::Args>;
    type Output = Value;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let schema = self.inner.parameters_schema();
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: self.inner.description().to_string(),
            parameters: schema,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let RuntimeEnvelope { ctx, args: tool_args } = args;
        let session_id = ctx.session_id.clone();
        let metadata = CallMetadata::new(
            T::NAME.to_string(),
            T::NAMESPACE.to_string(),
            ctx.metadata.id.clone(),
            ctx.metadata.call_id.clone(),
            self.inner.is_async(),
        );
        let ctx = ToolCallCtx {
            session_id: ctx.session_id.clone(),
            metadata: metadata.clone(),
        };

        if metadata.is_async {
            // Async tools: register receiver for polling, return immediate ACK
            let scheduler = ToolScheduler::get_or_init()
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            let handler = scheduler.get_session_handler(session_id, vec![T::NAMESPACE.to_string()]);

            let (tx, rx) = mpsc::channel::<EyreResult<Value>>(100);
            let tool = self.inner.clone();
            let ctx = ctx.clone();

            tokio::spawn(async move {
                tool.run_async(tx, ctx, tool_args).await;
            });

            handler
                .lock()
                .await
                .register_receiver(ToolReciever::new_async(metadata.clone(), rx));

            Ok(json!({
                "status": "queued",
                "id": metadata.id,
            }))
        } else {
            // Sync tools: wait for result directly, do NOT register with handler
            let (tx, rx) = oneshot::channel::<EyreResult<Value>>();
            let tool = self.inner.clone();

            tokio::spawn(async move {
                tool.run_sync(tx, ctx, tool_args).await;
            });

            // Wait for the result directly
            match rx.await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(e)) => Err(ToolError::ToolCallError(e.to_string().into())),
                Err(_) => Err(ToolError::ToolCallError("Tool channel closed".into())),
            }
        }
    }
}
