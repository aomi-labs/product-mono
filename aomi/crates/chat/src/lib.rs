use serde::Serialize;
use serde_json::Value;
use std::{
    fmt,
    sync::{Arc, Mutex},
};

pub mod accounts;
pub mod app;
pub mod completion;
pub mod connections;
pub mod prompts;

// Re-exports
pub use accounts::generate_account_context;
pub use aomi_tools::{CallMetadata, ToolCompletion, ToolReturn};
pub use app::{CoreApp, CoreAppBuilder};
pub use completion::{CoreCommandStream, StreamingError, stream_completion};
pub use rig::message::{AssistantContent, Message, UserContent};

/// System-level events that travel outside the LLM chat stream.
///
/// Event routing:
/// - `InlineDisplay`: LLM → UI only (sync json event like wallet_tx_request)
/// - `SystemNotice`: System → UI only (like title updates)
/// - `SystemError`: System → UI & LLM (like connection errors)
/// - `AsyncCallback`: System → UI & LLM (async tool results)
#[derive(Debug, Clone, Serialize)]
pub enum SystemEvent {
    /// LLM → UI or UI -> LLM. Sync json event like wallet_tx_request and wallet_tx_response.
    /// defferentiate between wallet_tx_request and wallet_tx_response by the type field.
    InlineDisplay(Value),
    /// System → UI only. Notices like title updates.
    SystemNotice(String),
    /// System → UI & LLM. Errors that both need to know about.
    SystemError(String),
    /// System → UI & LLM. Async tool results (tool callbacks).
    AsyncCallback(Value),
}

impl SystemEvent {
    /// Returns true if this event should be delivered to the LLM.
    pub fn is_llm_event(&self) -> bool {
        matches!(
            self,
            SystemEvent::SystemError(_) | SystemEvent::AsyncCallback(_)
        ) || matches!(self, SystemEvent::InlineDisplay(value) if is_wallet_tx_response(value))
    }

    /// Returns true if this event should be delivered to the frontend.
    pub fn is_frontend_event(&self) -> bool {
        true
    }
}

fn is_wallet_tx_response(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t == "wallet_tx_response")
}

/// Internal state for SystemEventQueue with per-consumer counters.
#[derive(Debug, Default)]
struct SystemEventQueueInner {
    events: Vec<SystemEvent>,
    /// Counter for frontend consumption (UI path)
    frontend_event_cnt: usize,
    /// Counter for LLM consumption (stream_completion path)
    llm_event_cnt: usize,
}

#[derive(Clone, Debug, Default)]
pub struct SystemEventQueue {
    inner: Arc<Mutex<SystemEventQueueInner>>,
}

impl SystemEventQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SystemEventQueueInner::default())),
        }
    }

    /// Append an event to the queue. Returns the index of the new event.
    pub fn push(&self, event: SystemEvent) -> usize {
        if let Ok(mut guard) = self.inner.lock() {
            let idx = guard.events.len();
            guard.events.push(event);
            idx
        } else {
            0
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.events.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clone all events from the provided index onward (legacy API).
    pub fn slice_from(&self, start: usize) -> Vec<SystemEvent> {
        if let Ok(guard) = self.inner.lock() {
            return guard.events.get(start..).unwrap_or(&[]).to_vec();
        }
        Vec::new()
    }

    /// Advance frontend counter and return new events since last call.
    /// Used by SessionState::sync_system_events for UI delivery.
    pub fn advance_frontend_events(&self) -> Vec<SystemEvent> {
        if let Ok(mut guard) = self.inner.lock() {
            let start = guard.frontend_event_cnt;
            let events = guard
                .events
                .get(start..)
                .unwrap_or(&[])
                .iter()
                .filter(|event| event.is_frontend_event())
                .cloned()
                .collect();
            guard.frontend_event_cnt = guard.events.len();
            events
        } else {
            Vec::new()
        }
    }

    /// Advance frontend counter and return all events since last call.
    /// Used by SessionState to process events before UI filtering.
    pub fn advance_all_events(&self) -> Vec<SystemEvent> {
        if let Ok(mut guard) = self.inner.lock() {
            let start = guard.frontend_event_cnt;
            let events = guard.events.get(start..).unwrap_or(&[]).to_vec();
            guard.frontend_event_cnt = guard.events.len();
            events
        } else {
            Vec::new()
        }
    }

    /// Advance LLM counter and return new LLM-relevant events since last call.
    /// Only returns SystemError and AsyncCallback events.
    /// Used by stream_completion for injecting tool results into prompts.
    pub fn advance_llm_events(&self) -> Vec<SystemEvent> {
        if let Ok(mut guard) = self.inner.lock() {
            let start = guard.llm_event_cnt;
            let events: Vec<SystemEvent> = guard
                .events
                .get(start..)
                .unwrap_or(&[])
                .iter()
                .filter(|e| e.is_llm_event())
                .cloned()
                .collect();
            guard.llm_event_cnt = guard.events.len();
            events
        } else {
            Vec::new()
        }
    }

    /// Check if there are unread events for the frontend.
    pub fn has_frontend_events(&self) -> bool {
        if let Ok(guard) = self.inner.lock() {
            guard
                .events
                .get(guard.frontend_event_cnt..)
                .unwrap_or(&[])
                .iter()
                .any(|e| e.is_frontend_event())
        } else {
            false
        }
    }

    /// Check if there are unread LLM-relevant events.
    pub fn has_llm_events(&self) -> bool {
        if let Ok(guard) = self.inner.lock() {
            guard
                .events
                .get(guard.llm_event_cnt..)
                .unwrap_or(&[])
                .iter()
                .any(|e| e.is_llm_event())
        } else {
            false
        }
    }

    /// Push a tool completion event into the queue (async callbacks only).
    /// Convenience method for EventManager / scheduler poller.
    pub fn push_tool_update(&self, completion: aomi_tools::ToolCompletion) -> usize {
        let value = serde_json::json!({
            "type": "tool_completion",
            "id": completion.metadata.id,
            "call_id": completion.metadata.call_id,
            "tool_name": completion.metadata.name,
            "result": completion.result.clone().unwrap_or_else(|e| serde_json::json!({"error": e})),
        });
        self.push(SystemEvent::AsyncCallback(value))
    }
}

// CoreCommand for tool results and streaming text
#[derive(Debug)]
pub enum CoreCommand {
    StreamingText(String),
    ToolCall {
        topic: String,
        stream: aomi_tools::ToolReturn,
    },
    Complete,
    Error(String),
    Interrupted,
}

impl fmt::Display for CoreCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreCommand::StreamingText(text) => write!(f, "{}", text),
            CoreCommand::ToolCall { topic, .. } => write!(f, "Tool: {}", topic),
            CoreCommand::Error(error) => write!(f, "{}", error),
            _ => Ok(()),
        }
    }
}
