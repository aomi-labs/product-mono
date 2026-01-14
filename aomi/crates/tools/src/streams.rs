use aomi_tools_v2::CallMetadata;
use eyre::Result as EyreResult;
use futures::{
    Stream,
    future::{BoxFuture, FutureExt, Shared},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, oneshot};

type ToolStreamItem = (CallMetadata, Result<Value, String>);
type ToolResult = EyreResult<Value>;

/// Result from polling a tool stream - includes metadata for routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCompletion {
    pub metadata: CallMetadata,
    pub sync: bool,
    pub result: Result<Value, String>,
}

/// Internal type that holds the actual channel receivers.
/// Use `into_shared_streams()` to convert to UI-consumable `ToolStream`.
pub struct ToolReciever {
    metadata: CallMetadata,
    finished: bool,
    /// Multi-step tools use mpsc receiver for streaming chunks
    multi_step_rx: Option<mpsc::Receiver<ToolResult>>,
    /// Single-result tools use oneshot receiver
    single_rx: Option<oneshot::Receiver<ToolResult>>,
}

impl ToolReciever {
    pub fn new_single(
        metadata: CallMetadata,
        single_rx: oneshot::Receiver<ToolResult>,
    ) -> Self {
        Self {
            metadata,
            finished: false,
            multi_step_rx: None,
            single_rx: Some(single_rx),
        }
    }

    pub fn new_multi_step(
        metadata: CallMetadata,
        multi_step_rx: mpsc::Receiver<ToolResult>,
    ) -> Self {
        Self {
            metadata,
            finished: false,
            multi_step_rx: Some(multi_step_rx),
            single_rx: None,
        }
    }

    pub fn metadata(&self) -> &CallMetadata {
        &self.metadata
    }

    pub fn into_stream(&mut self) -> ToolStream {
        self.into_shared_streams().0
    }

    /// Convert this future into two shared streams for (ongoing_streams, UI).
    /// Both streams yield the same single value via Shared<Future>.
    pub fn into_shared_streams(&mut self) -> (ToolStream, ToolStream) {
        let metadata = self.metadata.clone();

        if self.multi_step_rx.is_some() {
            let multi_rx = self.multi_step_rx.take().unwrap();
            let (ui_rx, bg_rx) = split_ui_bg_recievers(metadata.clone(), multi_rx);

            let shared = async move {
                match ui_rx.await {
                    Ok(r) => r,
                    Err(_) => (metadata.clone(), Err("Channel closed".to_string())),
                }
            }
            .boxed()
            .shared();

            (
                ToolStream::from_mpsc(bg_rx, self.metadata.clone()),
                {
                    let mut ui_stream = ToolStream::from_shared(
                        shared,
                        self.metadata.clone(),
                    );
                    // UI stream represents the first chunk for multi-step tools.
                    ui_stream.first_chunk_sent = true;
                    ui_stream
                },
            )
        } else if self.single_rx.is_some() {
            let single_rx = self.single_rx.take().unwrap();
            ToolStream::new_oneshot_shared(metadata.clone(), single_rx)
        } else {
            // Error case - no receiver available
            let (tx, rx) = oneshot::channel::<ToolResult>();
            let _ = tx.send(Err(eyre::eyre!("No receiver")));
            ToolStream::new_oneshot_shared(metadata.clone(), rx)
        }
    }
}

type SplitReceivers = (
    oneshot::Receiver<(CallMetadata, Result<Value, String>)>,
    mpsc::Receiver<(CallMetadata, Result<Value, String>)>,
);

fn split_ui_bg_recievers(
    metadata: CallMetadata,
    mut multi_rx: mpsc::Receiver<ToolResult>,
) -> SplitReceivers {
    let (ui_tx, ui_rx) = oneshot::channel::<(CallMetadata, Result<Value, String>)>();
    let (bg_tx, bg_rx) = mpsc::channel::<(CallMetadata, Result<Value, String>)>(100);

    tokio::spawn(async move {
        // Capture the first chunk (or channel-close) for both streams.
        let first = multi_rx
            .recv()
            .await
            .map(|r| (metadata.clone(), r.map_err(|e| e.to_string())))
            .unwrap_or_else(|| (metadata.clone(), Err("Channel closed".to_string())));

        // Send ACK to UI stream only; bg stream will start from subsequent chunks
        let _ = ui_tx.send(first.clone());
        let _ = bg_tx.send(first.clone()).await;

        // Forward remaining chunks into the fanout channel for pending polling.
        while let Some(item) = multi_rx.recv().await {
            let mapped = (metadata.clone(), item.map_err(|e| e.to_string()));
            let _ = bg_tx.send(mapped).await;
        }
    });

    (ui_rx, bg_rx)
}

impl Debug for ToolReciever {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ToolReciever({}, multi_step={})",
            self.metadata.id,
            self.multi_step_rx.is_some()
        )
    }
}

impl Future for ToolReciever {
    type Output = ToolStreamItem;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.multi_step_rx.as_mut() {
            Some(rx) => match rx.poll_recv(cx) {
                Poll::Ready(Some(result)) => {
                    Poll::Ready((this.metadata.clone(), result.map_err(|e| e.to_string())))
                }
                Poll::Ready(None) => {
                    Poll::Ready((this.metadata.clone(), Err("Channel closed".to_string())))
                }
                Poll::Pending => Poll::Pending,
            },
            None => {
                if this.finished {
                    Poll::Ready((this.metadata.clone(), Err("Channel closed".to_string())))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

/// UI-facing stream that yields (metadata, Result<Value>) items.
/// Uses Shared<BoxFuture> internally for Sync - yields one item then completes.
/// For multi-step tools, each chunk becomes a separate stream via broadcast.
#[derive(Default)]
pub struct ToolStream {
    pub metadata: CallMetadata,
    inner: Option<StreamInner>,
    /// Marks whether the first chunk (sync ACK) has been seen for multi-step streams.
    pub first_chunk_sent: bool,
}

enum StreamInner {
    Single(SharedToolFuture),
    Multi(mpsc::Receiver<ToolStreamItem>),
}

pub type SharedToolFuture = Shared<BoxFuture<'static, ToolStreamItem>>;

impl Debug for ToolStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ToolStream(tool={}, multi={})",
            self.metadata.name, self.first_chunk_sent
        )
    }
}

impl Stream for ToolStream {
    type Item = ToolStreamItem;

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

impl ToolStream {
    /// Empty stream (yields nothing).
    pub fn empty() -> Self {
        Self {
            metadata: CallMetadata::default(),
            inner: None,
            first_chunk_sent: false,
        }
    }

    pub fn is_multi_step(&self) -> bool {
        matches!(self.inner.as_ref(), Some(StreamInner::Multi(_)))
    }

    /// Convenience to create a single-item stream from a ready result.
    pub fn from_result(
        metadata: CallMetadata,
        result: Result<Value, String>,
    ) -> Self {
        let metadata_ = metadata.clone();
        let future = async move { (metadata_, result) }.boxed();
        ToolStream::from_future(future, metadata)
    }

    /// Create from a shared future (both consumers get same value)
    pub fn from_shared(shared: SharedToolFuture, metadata: CallMetadata) -> Self {
        Self {
            metadata,
            inner: Some(StreamInner::Single(shared)),
            first_chunk_sent: false,
        }
    }

    /// Create from a boxed future directly (converts to shared internally)
    pub fn from_future(
        future: BoxFuture<'static, (CallMetadata, Result<Value, String>)>,
        metadata: CallMetadata,
    ) -> Self {
        Self {
            metadata,
            inner: Some(StreamInner::Single(future.shared())),
            first_chunk_sent: false,
        }
    }

    pub fn from_mpsc(
        rx: mpsc::Receiver<(CallMetadata, Result<Value, String>)>,
        metadata: CallMetadata,
    ) -> Self {
        Self {
            metadata,
            inner: Some(StreamInner::Multi(rx)),
            first_chunk_sent: false,
        }
    }

    /// Create a pair of streams from a one-shot future, cloning the shared future internally.
    pub fn new_oneshot_shared(
        metadata: CallMetadata,
        rx: oneshot::Receiver<ToolResult>,
    ) -> (Self, Self) {
        let metadata_ = metadata.clone();
        let shared_future = async move {
            match rx.await {
                Ok(r) => (metadata_.clone(), r.map_err(|e| e.to_string())),
                Err(_) => (metadata_.clone(), Err("Channel closed".to_string())),
            }
        }
        .boxed()
        .shared();
        (
            ToolStream::from_shared(shared_future.clone(), metadata.clone()),
            ToolStream::from_shared(shared_future, metadata),
        )
    }
}

// ============================================================================
// Channel Types for Tool Results
// ============================================================================

/// Sender side for tool results - either oneshot (single result) or mpsc (multi-step)
pub enum ToolResultSender {
    /// Single result - low overhead, for most tools
    Oneshot(oneshot::Sender<ToolResult>),
    /// Multi-step results - tool owns this and sends multiple chunks
    MultiStep(mpsc::Sender<ToolResult>),
}

/// Type-erased request that can hold any tool request as JSON
#[derive(Debug, Clone)]
pub struct SchedulerRequest {
    pub tool_name: String,
    pub payload: Value,
}
