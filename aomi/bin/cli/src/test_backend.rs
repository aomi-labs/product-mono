use std::sync::Arc;

use aomi_backend::AomiApp;
use aomi_chat::{
    CoreCommand, SystemEvent, ToolStream,
    app::{CoreCtx, CoreState},
};
use aomi_tools::{
    CallMetadata, ToolScheduler,
    test_utils::{register_mock_multi_step_tool, register_mock_tools},
};
use async_trait::async_trait;
use eyre::{Result, eyre};
use serde_json::json;

/// Lightweight backend that exercises the tool scheduler with shared mock tools.
/// Used by the CLI to provide an interactive, dependency-free test harness.
pub struct TestBackend {
    scheduler: Arc<ToolScheduler>,
}

impl TestBackend {
    pub async fn new() -> Result<Self> {
        let scheduler = ToolScheduler::new_for_test().await.map_err(|e| eyre!(e))?;
        register_mock_tools(&scheduler);
        register_mock_multi_step_tool(&scheduler, None);
        Ok(Self { scheduler })
    }
}

#[async_trait]
impl AomiApp for TestBackend {
    type Command = CoreCommand<ToolStream>;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        let mut handler = self.scheduler.get_handler();
        let payload = json!({ "input": input });

        handler
            .request(
                "mock_single".to_string(),
                payload.clone(),
                CallMetadata::new("mock_single_call", None),
            )
            .await;
        handler
            .request(
                "mock_multi_step".to_string(),
                payload,
                CallMetadata::new("mock_multi_call", None),
            )
            .await;
        // resolve_calls returns UI streams and adds bg streams to ongoing_streams internally
        if let Some(mut ui_streams) = handler.resolve_calls().await {
            while let Some(stream) = ui_streams.pop() {
                let topic = stream.tool_name.clone();
                ctx.command_sender
                    .send(CoreCommand::ToolCall { topic, stream })
                    .await?;
            }
        }

        if let Some(system_events) = state.system_events.as_ref() {
            system_events.push(SystemEvent::InlineDisplay(json!({
                "type": "test_backend",
                "message": "Dispatched mock tool calls",
            })));
            system_events.push(SystemEvent::AsyncUpdate(json!({
                "type": "test_backend_async",
                "message": "Mock async update",
            })));
        }

        ctx.command_sender.send(CoreCommand::Complete).await?;
        Ok(())
    }
}
