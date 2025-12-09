use eyre::eyre;
use eyre::Result;
use futures::{
    future::{BoxFuture, FutureExt, Shared},
    Stream,
};
use serde_json::Value;
use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, oneshot};

/// Drives tool output for both single-result and multi-step tools.
pub struct ToolResultFuture {
    call_id: String,
    finished: bool,
    single: Option<Shared<BoxFuture<'static, (String, Result<Value, String>)>>>,
    stream_rx: Option<mpsc::Receiver<Result<Value>>>,
    first_chunk_tx: Option<oneshot::Sender<Result<Value>>>,
}

impl ToolResultFuture {
    pub fn new_single(
        call_id: String,
        future: Shared<BoxFuture<'static, (String, Result<Value, String>)>>,
    ) -> Self {
        Self {
            call_id,
            finished: false,
            single: Some(future),
            stream_rx: None,
            first_chunk_tx: None,
        }
    }

    pub fn new_multi_step(
        call_id: String,
        stream_rx: mpsc::Receiver<Result<Value>>,
        first_chunk_tx: oneshot::Sender<Result<Value>>,
    ) -> Self {
        Self {
            call_id,
            finished: false,
            single: None,
            stream_rx: Some(stream_rx),
            first_chunk_tx: Some(first_chunk_tx),
        }
    }

    pub fn tool_call_id(&self) -> &str {
        &self.call_id
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

impl Debug for ToolResultFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ToolResultFuture({}, finished={}, streaming={})",
            self.call_id,
            self.finished,
            self.stream_rx.is_some()
        )
    }
}

impl futures::Future for ToolResultFuture {
    type Output = (String, Result<Value, String>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        if let Some(rx) = this.stream_rx.as_mut() {
            match rx.poll_recv(cx) {
                Poll::Ready(Some(result)) => {
                    if let Some(tx) = this.first_chunk_tx.take() {
                        let first = result
                            .as_ref()
                            .map(|v| v.clone())
                            .map_err(|e| eyre!(e.to_string()));
                        let _ = tx.send(first);
                    }

                    let output = match result {
                        Ok(value) => {
                            this.finished = value
                                .get("finished")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            (this.call_id.clone(), Ok(value))
                        }
                        Err(e) => {
                            this.finished = true;
                            (this.call_id.clone(), Err(e.to_string()))
                        }
                    };
                    return Poll::Ready(output);
                }
                Poll::Ready(None) => {
                    this.finished = true;
                    return Poll::Ready((this.call_id.clone(), Ok(Value::Null)));
                }
                Poll::Pending => {}
            }
        }

        if let Some(future) = this.single.as_mut() {
            if let Poll::Ready(result) = Pin::new(future).poll(cx) {
                this.finished = true;
                this.single = None;
                return Poll::Ready(result);
            }
        }

        if this.stream_rx.is_some() {
            Poll::Pending
        } else {
            this.finished = true;
            Poll::Ready((this.call_id.clone(), Ok(Value::Null)))
        }
    }
}

/// Single-item stream for the first result chunk (UI ACK).
pub struct ToolResultStream {
    future: Option<BoxFuture<'static, (String, Result<Value, String>)>>,
}

impl Debug for ToolResultStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ToolResultStream")
    }
}

impl Stream for ToolResultStream {
    type Item = (String, Result<Value, String>);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.future.as_mut() {
            Some(fut) => {
                // SAFETY: We're not moving the future, just polling it
                let pinned = unsafe { Pin::new_unchecked(fut) };
                match pinned.poll(cx) {
                    Poll::Ready(result) => {
                        this.future = None;
                        Poll::Ready(Some(result))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
            None => Poll::Ready(None),
        }
    }
}

impl ToolResultStream {
    pub fn from_shared(
        future: Shared<BoxFuture<'static, (String, Result<Value, String>)>>,
    ) -> Self {
        Self {
            future: Some(async move { future.await }.boxed()),
        }
    }

    pub fn from_future(future: BoxFuture<'static, (String, Result<Value, String>)>) -> Self {
        Self { future: Some(future) }
    }
}


// ============================================================================
// Channel Types for Tool Results
// ============================================================================

/// Sender side for tool results - either oneshot (single result) or mpsc (multi-step)
pub enum ToolResultSender {
    /// Single result - low overhead, for most tools
    Oneshot(oneshot::Sender<Result<Value>>),
    /// Multi-step results - tool owns this and sends multiple chunks
    MultiStep(mpsc::Sender<Result<Value>>),
}

/// Receiver side for tool results
pub enum ToolResultReceiver {
    /// Single result receiver
    Oneshot(oneshot::Receiver<Result<Value>>),
    /// Multi-step receiver - yields multiple results over time
    MultiStep(mpsc::Receiver<Result<Value>>),
}

/// Type-erased request that can hold any tool request as JSON
#[derive(Debug, Clone)]
pub struct SchedulerRequest {
    pub tool_name: String,
    pub payload: Value,
}