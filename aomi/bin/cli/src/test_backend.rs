use std::sync::Arc;

use anyhow::{Result, anyhow};
use aomi_backend::AomiBackend;
use aomi_chat::{CoreCommand, Message, SystemEvent, SystemEventQueue, ToolStream};
use aomi_tools::{
    ToolScheduler,
    test_utils::{register_mock_multi_step_tool, register_mock_tools},
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::{RwLock, mpsc};

/// Lightweight backend that exercises the tool scheduler with shared mock tools.
/// Used by the CLI to provide an interactive, dependency-free test harness.
pub struct TestBackend {
    scheduler: Arc<ToolScheduler>,
}

impl TestBackend {
    pub async fn new() -> Result<Self> {
        let scheduler = ToolScheduler::new_for_test()
            .await
            .map_err(|e| anyhow!(e))?;
        register_mock_tools(&scheduler);
        register_mock_multi_step_tool(&scheduler, None);
        Ok(Self { scheduler })
    }
}

#[async_trait]
impl AomiBackend for TestBackend {
    type Command = CoreCommand<ToolStream>;

    async fn process_message(
        &self,
        _history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        _handler: Arc<tokio::sync::Mutex<aomi_tools::scheduler::ToolHandler>>,
        input: String,
        sender_to_ui: &mpsc::Sender<CoreCommand<ToolStream>>,
        _interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let mut handler = self.scheduler.get_handler();
        let payload = json!({ "input": input });

        handler
            .request(
                "mock_single".to_string(),
                payload.clone(),
                "mock_single_call".into(),
            )
            .await;
        handler
            .request(
                "mock_multi_step".to_string(),
                payload,
                "mock_multi_call".into(),
            )
            .await;
        // resolve_calls returns UI streams and adds bg streams to ongoing_streams internally
        if let Some(mut ui_streams) = handler.resolve_calls().await {
            while let Some(stream) = ui_streams.pop() {
                let topic = stream.tool_name.clone();
                sender_to_ui
                    .send(CoreCommand::ToolCall { topic, stream })
                    .await?;
            }
        }

        system_events.push(SystemEvent::InlineDisplay(json!({
            "type": "test_backend",
            "message": "Dispatched mock tool calls",
        })));
        system_events.push(SystemEvent::AsyncUpdate(json!({
            "type": "test_backend_async",
            "message": "Mock async update",
        })));

        sender_to_ui.send(CoreCommand::Complete).await?;
        Ok(())
    }
}
