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

/// Internal type that holds the actual channel receivers.
/// Use `into_shared_streams()` to convert to UI-consumable `ToolResultStream`.
pub struct ToolResultFuture {
    call_id: String,
    finished: bool,
    /// Multi-step tools use mpsc receiver for streaming chunks
    multi_step_rx: Option<mpsc::Receiver<Result<Value>>>,
    /// Single-result tools use oneshot receiver
    single_rx: Option<oneshot::Receiver<Result<Value>>>,
}

impl ToolResultFuture {
    pub fn new_single(
        call_id: String,
        single_rx: oneshot::Receiver<Result<Value>>,
    ) -> Self {
        Self {
            call_id,
            finished: false,
            multi_step_rx: None,
            single_rx: Some(single_rx),
        }
    }

    pub fn new_multi_step(
        call_id: String,
        multi_step_rx: mpsc::Receiver<Result<Value>>,
    ) -> Self {
        Self {
            call_id,
            finished: false,
            multi_step_rx: Some(multi_step_rx),
            single_rx: None,
        }
    }

    pub fn tool_call_id(&self) -> &str {
        &self.call_id
    }


    pub fn into_multi_stream(&mut self) -> (ToolResultStream, ToolResultStream) {
        self.into_shared_streams()
    }

    pub fn into_stream(&mut self) -> ToolResultStream {
        self.into_shared_streams().0
    }

    /// Convert this future into two shared streams for UI and pending_streams.
    /// Both streams yield the same single value via Shared<Future>.
    pub fn into_shared_streams(&mut self) -> (ToolResultStream, ToolResultStream) {
        let call_id = self.call_id.clone();

        if self.multi_step_rx.is_some() {
            let multi_rx = self.multi_step_rx.take().unwrap();
            let call_id_clone = call_id.clone();

            // Fan out the first chunk into a oneshot for the UI ack stream while
            // forwarding the entire sequence into a dedicated channel-backed stream.
            let (first_tx, first_rx) =
                oneshot::channel::<(String, Result<Value, String>)>();
            let (fanout_tx, fanout_rx) =
                mpsc::channel::<(String, Result<Value, String>)>(100);

            tokio::spawn(async move {
                let mut rx = multi_rx;
                // Capture the first chunk (or channel-close) for both streams.
                let first = rx
                    .recv()
                    .await
                    .map(|r| (call_id_clone.clone(), r.map_err(|e| e.to_string())))
                    .unwrap_or_else(|| {
                        (call_id_clone.clone(), Err("Channel closed".to_string()))
                    });

                let _ = first_tx.send(first.clone());
                if fanout_tx.send(first).await.is_err() {
                    return;
                }

                // Forward remaining chunks into the fanout channel for pending polling.
                while let Some(item) = rx.recv().await {
                    let mapped = (call_id_clone.clone(), item.map_err(|e| e.to_string()));
                    if fanout_tx.send(mapped).await.is_err() {
                        break;
                    }
                }
            });

            let shared = async move {
                match first_rx.await {
                    Ok(r) => r,
                    Err(_) => (call_id.clone(), Err("Channel closed".to_string())),
                }
            }
            .boxed()
            .shared();

            (
                ToolResultStream::from_channel(fanout_rx),
                ToolResultStream::from_shared(shared),
            )
        } else if self.single_rx.is_some() {
            let single_rx = self.single_rx.take().unwrap();
            let call_id_clone = call_id.clone();
            let shared = async move {
                match single_rx.await {
                    Ok(r) => (call_id_clone.clone(), r.map_err(|e| e.to_string())),
                    Err(_) => (call_id_clone.clone(), Err("Channel closed".to_string())),
                }
            }
            .boxed()
            .shared();

            (
                ToolResultStream::from_shared(shared.clone()),
                ToolResultStream::from_shared(shared),
            )
        } else {
            // Error case - no receiver available
            let call_id_clone = call_id.clone();
            let err_future = async move {
                (call_id_clone, Err("No receiver".to_string()))
            }.boxed().shared();

            (
                ToolResultStream::from_shared(err_future.clone()),
                ToolResultStream::from_shared(err_future),
            )
        }
    }
    
}

impl Debug for ToolResultFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ToolResultFuture({}, multi_step={})",
            self.call_id,
            self.multi_step_rx.is_some()
        )
    }
}


impl Future for ToolResultFuture {
    type Output = (String, Result<Value, String>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.multi_step_rx.as_mut() {
            Some(rx) => {
                match rx.poll_recv(cx) {
                    Poll::Ready(Some(result)) => Poll::Ready((this.call_id.clone(), result.map_err(|e| e.to_string()))),
                    Poll::Ready(None) => Poll::Ready((this.call_id.clone(), Err("Channel closed".to_string()))),
                    Poll::Pending => Poll::Pending,
                }
            },
            None => {
                if this.finished {
                    Poll::Ready((this.call_id.clone(), Err("Channel closed".to_string())))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

// impl Stream for ToolResultFuture {
//     type Item = (String, Result<Value, String>);

//     fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         let this = self.get_mut();
//         match this.multi_step_rx.as_mut() {
//             Some(rx) => match rx.poll_recv(cx) {
//                 Poll::Ready(Some(result)) => {
//                     Poll::Ready(Some((this.call_id.clone(), result.map_err(|e| e.to_string()))))
//                 }
//                 Poll::Ready(None) => Poll::Ready(None),
//                 Poll::Pending => Poll::Pending,
//             },
//             None => Poll::Ready(None),
//         }
//     }
// }
/// Type alias for the shared future used in streams
pub type SharedToolFuture = Shared<BoxFuture<'static, (String, Result<Value, String>)>>;

/// UI-facing stream that yields (call_id, Result<Value>) items.
/// Uses Shared<BoxFuture> internally for Sync - yields one item then completes.
/// For multi-step tools, each chunk becomes a separate stream via broadcast.
#[derive(Default)]
pub struct ToolResultStream {
    inner: Option<StreamInner>,
}

enum StreamInner {
    Single(SharedToolFuture),
    Multi(mpsc::Receiver<(String, Result<Value, String>)>),
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
        match this.inner.as_mut() {
            Some(StreamInner::Single(shared)) => {
                let pinned = Pin::new(shared);
                match pinned.poll(cx) {
                    Poll::Ready(result) => {
                        this.inner = None;
                        Poll::Ready(Some(result))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
            Some(StreamInner::Multi(rx)) => match Pin::new(rx).poll_recv(cx) {
                Poll::Ready(Some(item)) => Poll::Ready(Some(item)),
                Poll::Ready(None) => {
                    this.inner = None;
                    Poll::Ready(None)
                }
                Poll::Pending => Poll::Pending,
            },
            None => Poll::Ready(None),
        }
    }
}

impl ToolResultStream {
    /// Create from a shared future (both consumers get same value)
    pub fn from_shared(shared: SharedToolFuture) -> Self {
        Self {
            inner: Some(StreamInner::Single(shared)),
        }
    }

    /// Create from a boxed future directly (converts to shared internally)
    pub fn from_future(future: BoxFuture<'static, (String, Result<Value, String>)>) -> Self {
        Self {
            inner: Some(StreamInner::Single(future.shared())),
        }
    }

    pub fn from_channel(rx: mpsc::Receiver<(String, Result<Value, String>)>) -> Self {
        Self {
            inner: Some(StreamInner::Multi(rx)),
        }
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
