use std::sync::Arc;

use aomi_backend::AomiApp;
use aomi_core::{
    CallMetadata, CoreCommand, SystemEvent, ToolReceiver, ToolReturn, ToolScheduler,
    app::{CoreCtx, CoreState},
};
use aomi_tools::test_utils::{register_mock_async_tool, register_mock_tools};
use async_trait::async_trait;
use eyre::{Result, eyre};
use serde_json::json;

/// Lightweight backend that exercises the tool scheduler with shared mock tools.
/// Used by the CLI to provide an interactive, dependency-free test harness.
pub struct TestSchedulerBackend {
    scheduler: Arc<ToolScheduler>,
}

impl TestSchedulerBackend {
    pub async fn new() -> Result<Self> {
        let scheduler = ToolScheduler::new_for_test().await.map_err(|e| eyre!(e))?;
        register_mock_tools(&scheduler);
        register_mock_async_tool(&scheduler, None);
        Ok(Self { scheduler })
    }
}

#[async_trait]
impl AomiApp for TestSchedulerBackend {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        let handler = self
            .scheduler
            .get_session_handler("test_session".to_string(), vec!["default".to_string()]);
        let payload = json!({ "input": input });

        let single_meta = CallMetadata::new(
            "mock_single".to_string(),
            "default".to_string(),
            "mock_single_call".to_string(),
            None,
            false,
        );
        let (single_tx, single_rx) = tokio::sync::oneshot::channel();
        let _ = single_tx.send(Ok(json!({ "result": payload })));

        let multi_meta = CallMetadata::new(
            "mock_async".to_string(),
            "default".to_string(),
            "mock_multi_call".to_string(),
            None,
            true,
        );
        let (multi_tx, multi_rx) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            let _ = multi_tx.send(Ok(json!({ "step": 1 }))).await;
            let _ = multi_tx.send(Ok(json!({ "step": 2 }))).await;
        });

        let mut guard = handler.lock().await;
        guard.register_receiver(ToolReceiver::new_single(single_meta.clone(), single_rx));
        guard.register_receiver(ToolReceiver::new_async(multi_meta.clone(), multi_rx));

        let single_ack = ToolReturn {
            metadata: single_meta.clone(),
            inner: json!({ "status": "queued", "id": single_meta.id }),
            is_sync_ack: true,
        };
        ctx.command_sender
            .send(CoreCommand::ToolCall {
                topic: single_meta.name.clone(),
                stream: single_ack,
            })
            .await?;

        let multi_ack = ToolReturn {
            metadata: multi_meta.clone(),
            inner: json!({ "status": "queued", "id": multi_meta.id }),
            is_sync_ack: true,
        };
        ctx.command_sender
            .send(CoreCommand::ToolCall {
                topic: multi_meta.name.clone(),
                stream: multi_ack,
            })
            .await?;

        if let Some(system_events) = state.system_events.as_ref() {
            system_events.push(SystemEvent::InlineCall(json!({
                "type": "test_backend",
                "message": "Dispatched mock tool calls",
            })));
            system_events.push(SystemEvent::AsyncCallback(json!({
                "type": "test_backend_async",
                "message": "Mock async update",
            })));
        }

        ctx.command_sender.send(CoreCommand::Complete).await?;
        Ok(())
    }
}
