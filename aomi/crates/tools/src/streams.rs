use crate::CallMetadata;
use eyre::Result as EyreResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::{Debug, Display};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, oneshot};

type ToolResult = EyreResult<Value>;
type AsyncToolResult = (ToolResult, bool);

type ToolStreamItem = (CallMetadata, Result<Value, String>, bool);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolReturn {
    pub metadata: CallMetadata,
    pub inner: Value,
    pub is_sync_ack: bool,
}

/// Result from polling a tool receiver - includes metadata for routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCompletion {
    pub metadata: CallMetadata,
    pub result: Result<Value, String>,
    pub has_more: bool,
}

impl Display for ToolCompletion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result_text = match self.result.as_ref() {
            Ok(value) => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
            Err(err) => format!("tool_error: {}", err),
        };
        let call_id = self.metadata.call_id.as_deref().unwrap_or("none");
        write!(
            f,
            "Tool result received for {} (id={}, call_id={}). Do not re-run this tool for the same request unless the user asks. Result: {}",
            self.metadata.name, self.metadata.id, call_id, result_text
        )
    }
}

/// Internal type that holds the actual channel receivers.
pub struct ToolReciever {
    metadata: CallMetadata,
    finished: bool,
    /// Async tools use mpsc receiver for streaming chunks
    async_rx: Option<mpsc::Receiver<AsyncToolResult>>,
    /// Single-result tools use oneshot receiver
    oneshot_rx: Option<oneshot::Receiver<ToolResult>>,
}

impl ToolReciever {
    pub fn new_single(metadata: CallMetadata, single_rx: oneshot::Receiver<ToolResult>) -> Self {
        Self {
            metadata,
            finished: false,
            async_rx: None,
            oneshot_rx: Some(single_rx),
        }
    }

    pub fn new_async(metadata: CallMetadata, async_rx: mpsc::Receiver<AsyncToolResult>) -> Self {
        Self {
            metadata,
            finished: false,
            async_rx: Some(async_rx),
            oneshot_rx: None,
        }
    }

    pub fn metadata(&self) -> &CallMetadata {
        &self.metadata
    }

    pub fn is_async(&self) -> bool {
        self.async_rx.is_some()
    }

    pub fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<ToolStreamItem>> {
        if let Some(rx) = self.async_rx.as_mut() {
            match rx.poll_recv(cx) {
                Poll::Ready(Some((result, has_more))) => {
                    let mapped = result.map_err(|e| e.to_string());
                    return Poll::Ready(Some((self.metadata.clone(), mapped, has_more)));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }

        if self.finished {
            return Poll::Ready(None);
        }

        match self.oneshot_rx.as_mut() {
            Some(rx) => match Pin::new(rx).poll(cx) {
                Poll::Ready(Ok(result)) => {
                    self.finished = true;
                    let mapped = result.map_err(|e| e.to_string());
                    Poll::Ready(Some((self.metadata.clone(), mapped, false)))
                }
                Poll::Ready(Err(_)) => {
                    self.finished = true;
                    Poll::Ready(Some((
                        self.metadata.clone(),
                        Err("Channel closed".to_string()),
                        false,
                    )))
                }
                Poll::Pending => Poll::Pending,
            },
            None => Poll::Ready(None),
        }
    }
}

impl Debug for ToolReciever {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ToolReciever({}, async={})",
            self.metadata.id,
            self.async_rx.is_some()
        )
    }
}
