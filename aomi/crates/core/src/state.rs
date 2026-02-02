use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use aomi_tools::{CallMetadata, ToolReturn};
use rig::{
    OneOrMany,
    message::{AssistantContent, Message},
};

use crate::context_window::{ContextWindow, DEFAULT_CONTEXT_BUDGET};
use crate::events::{SystemEvent, SystemEventQueue};

/// User wallet state synced from frontend
#[derive(Clone, Debug, Default)]
pub struct UserState {
    /// Connected wallet address (0x...)
    pub address: Option<String>,
    /// Chain ID the wallet is connected to
    pub chain_id: Option<u64>,
    /// Whether the wallet is currently connected
    pub is_connected: bool,
    /// ENS name if resolved
    pub ens_name: Option<String>,
}

impl UserState {
    /// Format user state as a system message for the LLM
    pub fn format_message(&self) -> String {
        if !self.is_connected {
            return "[[USER_STATE]] Wallet not connected".to_string();
        }

        let address = self.address.as_deref().unwrap_or("unknown");
        let chain_id = self
            .chain_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let ens = self.ens_name.as_deref().unwrap_or("none");

        format!(
            "[[USER_STATE]] Connected wallet: {} | Chain ID: {} | ENS: {}",
            address, chain_id, ens
        )
    }
}

#[derive(Clone)]
pub struct CoreState {
    /// User wallet state synced from frontend
    pub user_state: UserState,
    /// Full conversation history (for persistence and backward compatibility)
    pub history: Vec<Message>,
    /// Sliding window context manager for efficient LLM calls (lazily initialized)
    context_window: Option<ContextWindow>,
    pub system_events: Option<SystemEventQueue>,
    /// Session identifier for session-aware tool execution
    pub session_id: String,
    /// Tool namespaces allowed for this session
    pub namespaces: Vec<String>,
    /// Aomi tool name to namespace map for runtime envelope handling
    pub tool_namespaces: Arc<HashMap<String, String>>,
}

impl CoreState {
    /// Creates a new CoreState with context window enabled.
    pub fn new(
        user_state: UserState,
        history: Vec<Message>,
        system_events: Option<SystemEventQueue>,
        session_id: String,
        namespaces: Vec<String>,
        tool_namespaces: Arc<HashMap<String, String>>,
    ) -> Self {
        Self {
            user_state,
            history,
            context_window: None,
            system_events,
            session_id,
            namespaces,
            tool_namespaces,
        }
    }

    /// Enables the sliding window context manager.
    /// Call this to optimize LLM context for long conversations.
    pub fn enable_context_window(&mut self, budget: Option<usize>) {
        let budget = budget.unwrap_or(DEFAULT_CONTEXT_BUDGET);
        self.context_window = Some(ContextWindow::from_messages(self.history.clone(), budget));
    }

    /// Returns the context-limited history for LLM calls.
    /// If context window is enabled, returns only messages within the token budget.
    /// Otherwise, returns the full history (backward compatible).
    pub fn get_llm_context(&self) -> Vec<Message> {
        if let Some(ref window) = self.context_window {
            window.get_context()
        } else {
            self.history.clone()
        }
    }

    /// Returns context window stats if enabled.
    pub fn context_stats(&self) -> Option<(usize, usize, usize)> {
        self.context_window.as_ref().map(|w| {
            (w.context_len(), w.total_len(), w.window_tokens())
        })
    }

    /// Pushes a message to both history and context window (if enabled).
    pub fn push_message(&mut self, message: Message) {
        self.history.push(message.clone());
        if let Some(ref mut window) = self.context_window {
            window.push(message);
        }
    }

    /// Pops the last message from both history and context window.
    pub fn pop_message(&mut self) -> Option<Message> {
        let msg = self.history.pop();
        if let Some(ref mut window) = self.context_window {
            window.pop();
        }
        msg
    }

    pub fn push_tool_call(&mut self, tool_call: &rig::message::ToolCall) {
        self.push_message(Message::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::ToolCall(tool_call.clone())),
        });
    }

    pub fn push_async_update(
        &mut self,
        tool_name: String,
        call_id: CallMetadata,
        result_text: String,
    ) {
        let call_id_text = call_id.call_id.as_deref().unwrap_or("none").to_string();
        self.push_message(Message::user(format!(
            "[[SYSTEM]] Tool result for {} with id {} (call_id={}): {}",
            tool_name, call_id.id, call_id_text, result_text
        )));
    }

    pub fn push_tool_results(&mut self, tool_returns: Vec<ToolReturn>) {
        for tool_return in tool_returns {
            let ToolReturn {
                metadata, inner, ..
            } = tool_return;
            let CallMetadata { id, call_id, .. } = metadata;
            if let Some(call_id) = call_id {
                self.push_message(Message::User {
                    content: OneOrMany::one(rig::message::UserContent::tool_result_with_call_id(
                        id,
                        call_id,
                        OneOrMany::one(rig::message::ToolResultContent::text(inner.to_string())),
                    )),
                });
            } else {
                self.push_message(Message::User {
                    content: OneOrMany::one(rig::message::UserContent::tool_result(
                        id,
                        OneOrMany::one(rig::message::ToolResultContent::text(inner.to_string())),
                    )),
                });
            }
        }
    }

    pub fn push_user(&mut self, content: impl Into<String>) {
        self.push_message(Message::user(content));
    }

    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.push_message(Message::assistant(content));
    }

    /// Ingest LLM-relevant events from the system event queue into history.
    /// This processes SystemError and AsyncCallback events that the LLM needs to see.
    pub fn ingest_events(&mut self) {
        let mut seen_updates = HashSet::new();
        for event in self
            .system_events
            .as_ref()
            .map(|events| events.advance_llm_events())
            .unwrap_or_default()
        {
            match &event {
                SystemEvent::SystemError(message) => {
                    self.push_message(Message::user(format!("[[SYSTEM]] {}", message)));
                }
                SystemEvent::AsyncCallback(value) => {
                    if let Some((call_id, tool_name, result_text)) = recover_tool_from_value(value)
                    {
                        let update_key = format!("{}:{}", call_id.key(), result_text);
                        if !seen_updates.insert(update_key) {
                            continue;
                        }
                        self.push_async_update(tool_name, call_id, result_text);
                    }
                }
                _ => {}
            }
        }
    }
}

fn recover_tool_from_value(value: &serde_json::Value) -> Option<(CallMetadata, String, String)> {
    let id = value
        .get("id")
        .and_then(|value| value.as_str())
        .or_else(|| value.get("call_id").and_then(|value| value.as_str()))?
        .to_string();
    let call_id = value
        .get("call_id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let tool_name = value.get("tool_name")?.as_str()?.to_string();
    let result = value.get("result")?.clone();
    let result_text = if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        format!("tool_error: {}", error)
    } else {
        serde_json::to_string_pretty(&result)
            .unwrap_or_else(|err| format!("tool_error: failed to serialize tool result: {}", err))
    };
    Some((
        CallMetadata::new(tool_name.clone(), "default".to_string(), id, call_id, false),
        tool_name,
        result_text,
    ))
}

pub struct CoreCtx<'a> {
    pub command_sender: tokio::sync::mpsc::Sender<crate::CoreCommand>,
    pub interrupt_receiver: Option<&'a mut tokio::sync::mpsc::Receiver<()>>,
}

impl<'a> CoreCtx<'a> {
    pub(crate) async fn post_completion<S>(
        &mut self,
        response: &mut String,
        mut stream: S,
    ) -> eyre::Result<bool>
    where
        S: futures::Stream<Item = Result<crate::CoreCommand, crate::completion::StreamingError>>
            + Unpin,
    {
        use crate::CoreCommand;
        use crate::completion::StreamingError;
        use futures::{StreamExt, future};

        let mut interrupted = false;

        loop {
            tokio::select! {
                content = stream.next() => {
                    match content {
                        Some(Ok(command)) => {
                            if let CoreCommand::StreamingText(text) = &command {
                                response.push_str(text);
                            }
                            let _ = self.command_sender.send(command).await;
                        },
                        Some(Err(err)) => {
                            let is_completion_error = matches!(err, StreamingError::Completion(_));
                            let message = err.to_string();
                            let _ = self.command_sender.send(CoreCommand::Error(message)).await;
                            if is_completion_error {
                                return Err(eyre::eyre!(err));
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }
                _ = async {
                    if let Some(interrupt_receiver) = self.interrupt_receiver.as_mut() {
                        interrupt_receiver.recv().await;
                    } else {
                        future::pending::<()>().await;
                    }
                } => {
                    interrupted = true;
                    let _ = self.command_sender.send(CoreCommand::Interrupted).await;
                    break;
                }
            }
        }
        Ok(interrupted)
    }
}

#[cfg(test)]
mod tests {
    use super::recover_tool_from_value;
    use serde_json::json;

    #[test]
    fn tool_update_from_value_parses_completion_payload() {
        let payload = json!({
            "id": "req_1",
            "call_id": "call_99",
            "tool_name": "sample_tool",
            "result": { "ok": true }
        });

        let (meta, tool_name, text) =
            recover_tool_from_value(&payload).expect("parsed completion payload");
        assert_eq!(tool_name, "sample_tool");
        assert_eq!(meta.name, "sample_tool");
        assert_eq!(meta.id, "req_1");
        assert_eq!(meta.call_id.as_deref(), Some("call_99"));
        assert!(text.contains("\"ok\": true"));
    }

    #[test]
    fn tool_update_from_value_reports_error_payloads() {
        let payload = json!({
            "id": "req_2",
            "tool_name": "error_tool",
            "result": { "error": "boom" }
        });

        let (_meta, _tool_name, text) =
            recover_tool_from_value(&payload).expect("parsed error payload");
        assert!(text.contains("tool_error: boom"));
    }

    #[test]
    fn tool_update_from_value_returns_none_on_missing_fields() {
        let payload = json!({
            "id": "req_3",
            "result": { "ok": true }
        });

        assert!(recover_tool_from_value(&payload).is_none());
    }
}
