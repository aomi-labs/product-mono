use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// System-level events that travel outside the LLM chat stream.
///
/// Event routing:
/// - HTTP (sync, delivered with state polling):
///   - `InlineCall`: LLM ↔ UI (sync json event like wallet_tx_request/response)
///   - `SystemError`: System → UI & LLM (connection errors, etc.)
/// - SSE (async, broadcast immediately):
///   - `SystemNotice`: System → UI only (title updates, etc.)
///   - `AsyncCallback`: System → UI & LLM (async tool results)
#[derive(Debug, Clone, Serialize)]
pub enum SystemEvent { 
    /// LLM → UI or UI -> LLM. Sync json event like wallet_tx_request and wallet_tx_response.
    /// defferentiate between wallet_tx_request and wallet_tx_response by the type field.
    InlineCall(Value),
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
        ) || self.is_wallet_tx_response()
    }

    /// Returns true if this event should be delivered via HTTP (sync, with state polling).
    /// InlineCall and SystemError are sync events that block until handled.
    pub fn is_http_event(&self) -> bool {
        matches!(self, SystemEvent::InlineCall(_) | SystemEvent::SystemError(_))
    }

    /// Returns true if this event should be delivered via SSE (async, broadcast immediately).
    /// SystemNotice and AsyncCallback are async events.
    pub fn is_sse_event(&self) -> bool {
        matches!(self, SystemEvent::SystemNotice(_) | SystemEvent::AsyncCallback(_))
    }

    fn is_wallet_tx_response(&self) -> bool {
        let value = match &self {
            SystemEvent::AsyncCallback(value) => value,
            SystemEvent::InlineCall(value) => value,
            _ => &serde_json::Value::Null
        };
        value
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| t == "wallet_tx_response")
    }
}



/// Internal state for SystemEventQueue with per-consumer counters.
#[derive(Debug, Default)]
struct SystemEventQueueInner {
    events: Vec<SystemEvent>,
    /// Counter for HTTP events (InlineCall, SystemError) - delivered with state polling
    http_event_cnt: usize,
    /// Counter for SSE events (SystemNotice, AsyncCallback) - broadcast immediately
    sse_event_cnt: usize,
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

    /// Advance HTTP counter and return new HTTP events (InlineCall, SystemError) since last call.
    /// Used by get_session_response for sync event delivery with state polling.
    pub fn advance_http_events(&self) -> Vec<SystemEvent> {
        if let Ok(mut guard) = self.inner.lock() {
            let start = guard.http_event_cnt;
            let events = guard
                .events
                .get(start..)
                .unwrap_or(&[])
                .iter()
                .filter(|event| event.is_http_event())
                .cloned()
                .collect();
            guard.http_event_cnt = guard.events.len();
            events
        } else {
            Vec::new()
        }
    }

    /// Advance SSE counter and return new SSE events (SystemNotice, AsyncCallback) since last call.
    /// Used by broadcast_async_notifications for async event delivery via SSE.
    pub fn advance_sse_events(&self) -> Vec<SystemEvent> {
        if let Ok(mut guard) = self.inner.lock() {
            let start = guard.sse_event_cnt;
            let events = guard
                .events
                .get(start..)
                .unwrap_or(&[])
                .iter()
                .filter(|event| event.is_sse_event())
                .cloned()
                .collect();
            guard.sse_event_cnt = guard.events.len();
            events
        } else {
            Vec::new()
        }
    }

    /// Get SSE events (SystemNotice, AsyncCallback) without advancing the counter.
    /// If `count` is Some(n), returns the last n events; otherwise returns all.
    /// Used by get_events_endpoint for historical event retrieval.
    pub fn get_sse_events(&self, count: Option<usize>) -> Vec<SystemEvent> {
        if let Ok(guard) = self.inner.lock() {
            let sse_events: Vec<SystemEvent> = guard
                .events
                .iter()
                .filter(|event| event.is_sse_event())
                .cloned()
                .collect();
            match count {
                Some(n) => sse_events.into_iter().rev().take(n).rev().collect(),
                None => sse_events,
            }
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


    /// Push a tool completion event into the queue (async callbacks only).
    /// Convenience method for EventManager / scheduler poller.
    pub fn push_tool_update(&self, completion: aomi_tools::ToolCompletion) -> usize {
        let event_type = if completion.has_more {
            "tool_update"
        } else {
            "tool_complete"
        };
        let value = serde_json::json!({
            "type": event_type,
            "id": completion.metadata.id,
            "call_id": completion.metadata.call_id,
            "tool_name": completion.metadata.name,
            "result": completion.result.clone().unwrap_or_else(|e| serde_json::json!({"error": e})),
        });
        self.push(SystemEvent::AsyncCallback(value))
    }
}
