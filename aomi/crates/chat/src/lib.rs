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
pub use aomi_tools::{ToolCompletion, ToolResultStream};
pub use app::{ChatApp, ChatAppBuilder, LoadingProgress, run_chat};
pub use completion::{RespondStream, StreamingError, stream_completion};
pub use rig::message::{AssistantContent, Message, UserContent};

/// System-level events that travel outside the LLM chat stream.
///
/// Event routing:
/// - `InlineDisplay`: LLM → UI only (sync json event like wallet_tx_request)
/// - `SystemNotice`: System → UI only (like title updates)
/// - `SystemError`: System → UI & LLM (like connection errors)
/// - `AsyncUpdate`: System → UI & LLM (async tool results)
#[derive(Debug, Clone, Serialize)]
pub enum SystemEvent {
    /// LLM → UI only. Sync json event like wallet_tx_request.
    InlineDisplay(Value),
    /// System → UI only. Notices like title updates.
    SystemNotice(String),
    /// System → UI & LLM. Errors that both need to know about.
    SystemError(String),
    /// System → UI & LLM. Sync tool results (single or first chunk).
    SyncUpdate(Value),
    /// System → UI & LLM. Async tool results (follow-up chunks).
    AsyncUpdate(Value),
}

impl SystemEvent {
    /// Returns true if this event should be delivered to the LLM.
    pub fn is_llm_event(&self) -> bool {
        matches!(
            self,
            SystemEvent::SystemError(_) | SystemEvent::AsyncUpdate(_) | SystemEvent::SyncUpdate(_)
        )
    }

    /// Returns true if this event should be delivered to the frontend.
    pub fn is_frontend_event(&self) -> bool {
        match self {
            SystemEvent::SyncUpdate(value) => !is_queued_tool_ack(value),
            _ => true,
        }
    }
}

fn is_queued_tool_ack(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t == "tool_completion")
        && value
            .get("result")
            .and_then(|r| r.get("status"))
            .and_then(Value::as_str)
            .is_some_and(|s| s == "queued")
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
    /// Only returns SystemError and AsyncUpdate events.
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

    /// Push a tool completion event into the queue (sync or async).
    /// Convenience method for EventManager / scheduler poller.
    pub fn push_tool_update(&self, completion: aomi_tools::ToolCompletion) -> usize {
        let value = serde_json::json!({
            "type": "tool_completion",
            "call_id": completion.call_id,
            "tool_name": completion.tool_name,
            "sync": completion.sync,
            "result": completion.result.clone().unwrap_or_else(|e| serde_json::json!({"error": e})),
        });
        if completion.sync {
            self.push(SystemEvent::SyncUpdate(value))
        } else {
            self.push(SystemEvent::AsyncUpdate(value))
        }
    }
}

// Generic ChatCommand that can work with any stream type
#[derive(Debug)]
pub enum ChatCommand<S = Box<dyn std::any::Any + Send>> {
    StreamingText(String),
    ToolCall { topic: String, stream: S },
    Complete,
    Error(String),
    Interrupted,
}

impl<S> fmt::Display for ChatCommand<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChatCommand::StreamingText(text) => write!(f, "{}", text),
            ChatCommand::ToolCall { topic, .. } => write!(f, "Tool: {}", topic),
            ChatCommand::Error(error) => write!(f, "{}", error),
            _ => Ok(()),
        }
    }
}
