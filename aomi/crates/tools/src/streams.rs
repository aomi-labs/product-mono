use crate::CallMetadata;
use eyre::Result as EyreResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, oneshot};

type ToolResult = EyreResult<Value>;

type ToolStreamItem = (CallMetadata, Result<Value, String>);

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
    pub sync: bool,
    pub result: Result<Value, String>,
}

/// Internal type that holds the actual channel receivers.
pub struct ToolReciever {
    metadata: CallMetadata,
    finished: bool,
    ack_sent: bool,
    /// Async tools use mpsc receiver for streaming chunks
    async_rx: Option<mpsc::Receiver<ToolResult>>,
    /// Single-result tools use oneshot receiver
    oneshot_rx: Option<oneshot::Receiver<ToolResult>>,
}

impl ToolReciever {
    pub fn new_single(metadata: CallMetadata, single_rx: oneshot::Receiver<ToolResult>) -> Self {
        Self {
            metadata,
            finished: false,
            ack_sent: false,
            async_rx: None,
            oneshot_rx: Some(single_rx),
        }
    }

    pub fn new_multi_step(metadata: CallMetadata, multi_step_rx: mpsc::Receiver<ToolResult>) -> Self {
        Self {
            metadata,
            finished: false,
            ack_sent: false,
            async_rx: Some(multi_step_rx),
            oneshot_rx: None,
        }
    }

    pub fn metadata(&self) -> &CallMetadata {
        &self.metadata
    }

    pub fn is_async(&self) -> bool {
        self.async_rx.is_some()
    }

    pub fn mark_acked(&mut self) {
        self.ack_sent = true;
    }

    pub fn has_acked(&self) -> bool {
        self.ack_sent
    }

    pub fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<ToolStreamItem>> {
        if let Some(rx) = self.async_rx.as_mut() {
            match rx.poll_recv(cx) {
                Poll::Ready(Some(result)) => {
                    let mapped = result.map_err(|e| e.to_string());
                    return Poll::Ready(Some((self.metadata.clone(), mapped)));
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
                    Poll::Ready(Some((self.metadata.clone(), mapped)))
                }
                Poll::Ready(Err(_)) => {
                    self.finished = true;
                    Poll::Ready(Some((self.metadata.clone(), Err("Channel closed".to_string()))))
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
            "ToolReciever({}, multi_step={})",
            self.metadata.id,
            self.async_rx.is_some()
        )
    }
}
