mod utils;

use aomi_backend::session::{AomiApp, DefaultSessionState, MessageSender};
use aomi_core::{
    app::{CoreCtx, CoreState},
    CallMetadata, CoreCommand, SystemEvent, ToolReturn,
};
use aomi_tools::ToolScheduler;
use async_trait::async_trait;
use eyre::Result;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

// Drive session state forward for async backends
async fn pump_state(state: &mut DefaultSessionState) {
    for _ in 0..50 {
        state.sync_state().await;
        sleep(Duration::from_millis(20)).await;
    }
}

#[derive(Clone)]
struct WalletToolBackend {
    payload: Value,
}

impl WalletToolBackend {
    fn new(payload: Value) -> Self {
        Self { payload }
    }
}

#[async_trait]
impl AomiApp for WalletToolBackend {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        _input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        // Mirror completion.rs: enqueue wallet request immediately for UI
        if let Some(system_events) = state.system_events.as_ref() {
            system_events.push(SystemEvent::InlineCall(json!({
                "type": "wallet_tx_request",
                "payload": self.payload.clone(),
            })));
        }

        // Use the real scheduler, but the test helper keeps ExternalClients in test mode.
        let scheduler = ToolScheduler::new_for_test()
            .await
            .map_err(|e| eyre::eyre!(e.to_string()))?;
        let handler = scheduler
            .get_session_handler("wallet_session".to_string(), vec!["default".to_string()]);
        let tool_name = "send_transaction_to_wallet".to_string();
        let metadata = CallMetadata::new(
            tool_name.clone(),
            "default".to_string(),
            "wallet_call".to_string(),
            None,
            false,
        );
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(Ok(self.payload.clone()));
        let mut guard = handler.lock().await;
        guard.register_receiver(aomi_tools::ToolReciever::new_single(metadata.clone(), rx));
        drop(guard);

        let sync_ack = ToolReturn {
            metadata,
            inner: self.payload.clone(),
            is_sync_ack: true,
        };

        ctx.command_sender
            .send(CoreCommand::ToolCall {
                topic: tool_name,
                stream: sync_ack,
            })
            .await
            .expect("send tool call");

        ctx.command_sender
            .send(CoreCommand::Complete)
            .await
            .expect("send complete");

        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn wallet_tool_emits_request_and_result() {
    let payload = json!({
        "topic": "Send a test transaction",
        "to": "0x742d35Cc6634C0532925a3b844Bc9e7595f33749",
        "value": "0",
        "data": "0x",
        "gas_limit": "21000",
        "description": "Send a zero value test transaction"
    });

    let backend = Arc::new(WalletToolBackend::new(payload.clone()));
    let mut state = DefaultSessionState::new(backend, Vec::new())
        .await
        .expect("session init");

    state
        .send_user_input("please send".into())
        .await
        .expect("process user message");

    pump_state(&mut state).await;

    // Wallet request should be surfaced to the UI
    let wallet_event = state.advance_http_events().into_iter().find_map(|event| {
        if let SystemEvent::InlineCall(payload) = event {
            if payload.get("type").and_then(Value::as_str) == Some("wallet_tx_request") {
                return payload.get("payload").cloned();
            }
        }
        None
    });

    let request = wallet_event.expect("wallet request event present");
    assert_eq!(
        request.get("to").and_then(Value::as_str),
        Some("0x742d35Cc6634C0532925a3b844Bc9e7595f33749")
    );
    assert_eq!(request.get("value").and_then(Value::as_str), Some("0"));

    // Tool result should render in the assistant message stream
    let tool_message = state
        .messages
        .iter()
        .find(|m| matches!(m.sender, MessageSender::Assistant) && m.tool_result.is_some())
        .cloned()
        .expect("tool message exists");

    let (_, content) = tool_message.tool_result.expect("tool stream content");
    assert!(
        content.contains("0x742d35Cc6634C0532925a3b844Bc9e7595f33749"),
        "tool output should include destination address"
    );
    assert!(
        content.contains("description"),
        "tool output should include description payload"
    );
    assert!(
        !tool_message.is_streaming,
        "tool stream should be marked complete"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn wallet_tool_reports_validation_errors() {
    let payload = json!({
        "topic": "Bad tx",
        "to": "not_an_address",
        "value": "0",
        "data": "0x",
        "description": "Should fail validation"
    });

    let backend = Arc::new(WalletToolBackend::new(payload.clone()));
    let mut state = DefaultSessionState::new(backend, Vec::new())
        .await
        .expect("session init");

    state
        .send_user_input("please send bad tx".into())
        .await
        .expect("process user message");

    pump_state(&mut state).await;

    // Wallet request event still surfaces to UI (matches completion.rs behavior)
    let events = state.advance_http_events();
    let wallet_event = events.iter().find_map(|event| {
        if let SystemEvent::InlineCall(payload) = event {
            if payload.get("type").and_then(Value::as_str) == Some("wallet_tx_request") {
                return payload.get("payload").cloned();
            }
        }
        None
    });
    if wallet_event.is_none() {
        eprintln!(
            "wallet event missing. events: {:?}, messages: {:?}",
            events, state.messages
        );
    }
    assert!(
        wallet_event.is_some(),
        "wallet request event should still surface"
    );

    // Tool result should surface the payload for inspection.
    let tool_message = state
        .messages
        .iter()
        .find(|m| matches!(m.sender, MessageSender::Assistant) && m.tool_result.is_some())
        .cloned()
        .expect("tool message exists");

    let (_, content) = tool_message.tool_result.expect("tool stream content");
    assert!(
        content.contains("not_an_address"),
        "tool output should include payload to-address: {content}"
    );
}
