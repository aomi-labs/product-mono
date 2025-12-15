mod utils;

use aomi_backend::session::{AomiBackend, DefaultSessionState, MessageSender};
use aomi_chat::{ChatCommand, Message, SystemEvent, SystemEventQueue, ToolResultStream};
use aomi_tools::{ToolScheduler, wallet};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use utils::flush_state;

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
impl AomiBackend for WalletToolBackend {
    type Command = ChatCommand<ToolResultStream>;

    async fn process_message(
        &self,
        _history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        _input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand<ToolResultStream>>,
        _interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        // Mirror completion.rs: enqueue wallet request immediately for UI
        system_events.push(SystemEvent::WalletTxRequest {
            payload: self.payload.clone(),
        });

        // Ensure the real wallet tool is registered and schedule the call
        let scheduler =
            ToolScheduler::get_or_init().await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let _ = scheduler.register_tool(wallet::SendTransactionToWallet);

        let mut handler = scheduler.get_handler();
        let tool_name = "send_transaction_to_wallet".to_string();
        handler
            .request(tool_name.clone(), self.payload.clone(), "wallet_call".to_string())
            .await;

        let (_internal_stream, ui_stream) = handler
            .take_last_call_as_streams()
            .expect("wallet tool streams available");

        sender_to_ui
            .send(ChatCommand::ToolCall {
                topic: tool_name,
                stream: ui_stream,
            })
            .await
            .expect("send tool call");

        sender_to_ui
            .send(ChatCommand::Complete)
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
        .process_user_message("please send".into())
        .await
        .expect("process user message");

    flush_state(&mut state).await;

    // Wallet request should be surfaced to the UI
    let wallet_event = state.active_system_events.iter().find_map(|event| {
        if let SystemEvent::WalletTxRequest { payload } = event {
            Some(payload.clone())
        } else {
            None
        }
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
        .find(|m| matches!(m.sender, MessageSender::Assistant) && m.tool_stream.is_some())
        .cloned()
        .expect("tool message exists");

    let (_, content) = tool_message.tool_stream.expect("tool stream content");
    assert!(
        content.contains("0x742d35Cc6634C0532925a3b844Bc9e7595f33749"),
        "tool output should include destination address"
    );
    assert!(
        content.contains("description"),
        "tool output should include description payload"
    );
    assert!(!tool_message.is_streaming, "tool stream should be marked complete");
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
        .process_user_message("please send bad tx".into())
        .await
        .expect("process user message");

    flush_state(&mut state).await;

    // Wallet request event still surfaces to UI (matches completion.rs behavior)
    let wallet_event = state.active_system_events.iter().find_map(|event| {
        if let SystemEvent::WalletTxRequest { payload } = event {
            Some(payload.clone())
        } else {
            None
        }
    });
    assert!(wallet_event.is_some(), "wallet request event should still surface");

    // Tool result should contain the validation error
    let tool_message = state
        .messages
        .iter()
        .find(|m| matches!(m.sender, MessageSender::Assistant) && m.tool_stream.is_some())
        .cloned()
        .expect("tool message exists");

    let (_, content) = tool_message.tool_stream.expect("tool stream content");
    assert!(
        content.to_lowercase().contains("invalid 'to' address"),
        "tool output should mention invalid address: {content}"
    );
}
