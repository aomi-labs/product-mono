use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// System-level events that travel outside the LLM chat stream.
///
/// Event routing:
/// - `InlineCall`: LLM → UI only (sync json event like wallet_tx_request)
/// - `SystemNotice`: System → UI only (like title updates)
/// - `SystemError`: System → UI & LLM (like connection errors)
/// - `AsyncCallback`: System → UI & LLM (async tool results)
#[derive(Debug, Clone, Serialize)]
pub enum SystemEvent {
    /// LLM → UI or UI -> LLM. Sync json event like wallet_tx_request and wallet_tx_response.
    /// defferentiate between wallet_tx_request and wallet_tx_response by the type field.
    InlineCall(Value),
    /// System → UI only. Notices like title updates.
    SystemNotice(String),
    /// System → UI & LLM. Errors that both need to know about.
    SystemError(String),

    // -----👆 HTTP -----
    // -----👇 SSE -----

    
    /// System → UI & LLM. Async tool results (tool callbacks).
    AsyncCallback(Value), 
}

impl SystemEvent {
    /// Returns true if this event should be delivered to the LLM.
    pub fn is_llm_event(&self) -> bool {
        matches!(
            self,
            SystemEvent::SystemError(_) | SystemEvent::AsyncCallback(_)
        ) || matches!(self, SystemEvent::InlineCall(value) if is_wallet_tx_response(value))
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
