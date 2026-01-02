use eyre::Result;
use futures::{
    Stream,
    future::{BoxFuture, FutureExt, Shared},
};
use serde_json::Value;
use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, oneshot};

/// Result from polling a tool stream - includes metadata for routing
#[derive(Debug, Clone)]
pub struct ToolCompletion {
    pub call_id: String,
    pub tool_name: String,
    /// true for sync completions (single-step or first chunk of a multi-step),
    /// false for async follow-up chunks from multi-step tools
    pub sync: bool,
    pub result: Result<Value, String>,
}

/// Internal type that holds the actual channel receivers.
/// Use `into_shared_streams()` to convert to UI-consumable `ToolResultStream`.
pub struct ToolReciever {
    call_id: String,
    tool_name: String,
    finished: bool,
    /// Multi-step tools use mpsc receiver for streaming chunks
    multi_step_rx: Option<mpsc::Receiver<Result<Value>>>,
    /// Single-result tools use oneshot receiver
    single_rx: Option<oneshot::Receiver<Result<Value>>>,
}

impl ToolReciever {
    pub fn new_single(
        call_id: String,
        tool_name: String,
        single_rx: oneshot::Receiver<Result<Value>>,
    ) -> Self {
        Self {
            call_id,
            tool_name,
            finished: false,
            multi_step_rx: None,
            single_rx: Some(single_rx),
        }
    }

    pub fn new_multi_step(
        call_id: String,
        tool_name: String,
        multi_step_rx: mpsc::Receiver<Result<Value>>,
    ) -> Self {
        Self {
            call_id,
            tool_name,
            finished: false,
            multi_step_rx: Some(multi_step_rx),
            single_rx: None,
        }
    }

    pub fn tool_call_id(&self) -> &str {
        &self.call_id
    }

    pub fn into_stream(&mut self) -> ToolResultStream {
        self.into_shared_streams().0
    }

    /// Convert this future into two shared streams for (ongoing_streams, UI).
    /// Both streams yield the same single value via Shared<Future>.
    pub fn into_shared_streams(&mut self) -> (ToolResultStream, ToolResultStream) {
        let call_id = self.call_id.clone();

        if self.multi_step_rx.is_some() {
            let multi_rx = self.multi_step_rx.take().unwrap();
            let (ui_rx, bg_rx) = split_ui_bg_recievers(call_id.clone(), multi_rx);

            let shared = async move {
                match ui_rx.await {
                    Ok(r) => r,
                    Err(_) => (call_id.clone(), Err("Channel closed".to_string())),
                }
            }
            .boxed()
            .shared();

            (
                ToolResultStream::from_mpsc(bg_rx, self.call_id.clone(), self.tool_name.clone()),
                {
                    let mut ui_stream = ToolResultStream::from_shared(
                        shared,
                        self.call_id.clone(),
                        self.tool_name.clone(),
                    );
                    // UI stream represents the first chunk for multi-step tools.
                    ui_stream.first_chunk_sent = true;
                    ui_stream
                },
            )
        } else if self.single_rx.is_some() {
            let single_rx = self.single_rx.take().unwrap();
            ToolResultStream::new_oneshot_shared(call_id.clone(), self.tool_name.clone(), single_rx)
        } else {
            // Error case - no receiver available
            let (tx, rx) = oneshot::channel::<Result<Value>>();
            let _ = tx.send(Err(eyre::eyre!("No receiver")));
            ToolResultStream::new_oneshot_shared(call_id.clone(), self.tool_name.clone(), rx)
        }
    }
}

type SplitReceivers = (
    oneshot::Receiver<(String, Result<Value, String>)>,
    mpsc::Receiver<(String, Result<Value, String>)>,
);

fn split_ui_bg_recievers(
    call_id: String,
    mut multi_rx: mpsc::Receiver<Result<Value>>,
) -> SplitReceivers {
    let (ui_tx, ui_rx) = oneshot::channel::<(String, Result<Value, String>)>();
    let (bg_tx, bg_rx) = mpsc::channel::<(String, Result<Value, String>)>(100);

    tokio::spawn(async move {
        // Capture the first chunk (or channel-close) for both streams.
        let first = multi_rx
            .recv()
            .await
            .map(|r| (call_id.clone(), r.map_err(|e| e.to_string())))
            .unwrap_or_else(|| (call_id.clone(), Err("Channel closed".to_string())));

        // Send ACK to UI stream only; bg stream will start from subsequent chunks
        let _ = ui_tx.send(first.clone());
        let _ = bg_tx.send(first.clone()).await;

        // Forward remaining chunks into the fanout channel for pending polling.
        while let Some(item) = multi_rx.recv().await {
            let mapped = (call_id.clone(), item.map_err(|e| e.to_string()));
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
            self.call_id,
            self.multi_step_rx.is_some()
        )
    }
}

impl Future for ToolReciever {
    type Output = (String, Result<Value, String>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.multi_step_rx.as_mut() {
            Some(rx) => match rx.poll_recv(cx) {
                Poll::Ready(Some(result)) => {
                    Poll::Ready((this.call_id.clone(), result.map_err(|e| e.to_string())))
                }
                Poll::Ready(None) => {
                    Poll::Ready((this.call_id.clone(), Err("Channel closed".to_string())))
                }
                Poll::Pending => Poll::Pending,
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

/// UI-facing stream that yields (call_id, Result<Value>) items.
/// Uses Shared<BoxFuture> internally for Sync - yields one item then completes.
/// For multi-step tools, each chunk becomes a separate stream via broadcast.
#[derive(Default)]
pub struct ToolResultStream {
    inner: Option<StreamInner>,
    pub call_id: String,
    pub tool_name: String,
    /// Marks whether the first chunk (sync ACK) has been seen for multi-step streams.
    pub first_chunk_sent: bool,
}

enum StreamInner {
    Single(SharedToolFuture),
    Multi(mpsc::Receiver<(String, Result<Value, String>)>),
}

pub type SharedToolFuture = Shared<BoxFuture<'static, (String, Result<Value, String>)>>;

impl Debug for ToolResultStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ToolResultStream(tool={}, multi={})",
            self.tool_name, self.first_chunk_sent
        )
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
    /// Empty stream (yields nothing).
    pub fn empty() -> Self {
        Self {
            inner: None,
            call_id: String::new(),
            tool_name: String::new(),
            first_chunk_sent: false,
        }
    }

    pub fn is_multi_step(&self) -> bool {
        matches!(self.inner.as_ref(), Some(StreamInner::Multi(_)))
    }

    /// Convenience to create a single-item stream from a ready result.
    pub fn from_result(call_id: String, result: Result<Value, String>, tool_name: String) -> Self {
        let call_id_ = call_id.clone();
        let future = async move { (call_id_, result) }.boxed();
        ToolResultStream::from_future(future, call_id, tool_name)
    }

    /// Create from a shared future (both consumers get same value)
    pub fn from_shared(shared: SharedToolFuture, call_id: String, tool_name: String) -> Self {
        Self {
            inner: Some(StreamInner::Single(shared)),
            call_id,
            tool_name,
            first_chunk_sent: false,
        }
    }

    /// Create from a boxed future directly (converts to shared internally)
    pub fn from_future(
        future: BoxFuture<'static, (String, Result<Value, String>)>,
        call_id: String,
        tool_name: String,
    ) -> Self {
        Self {
            inner: Some(StreamInner::Single(future.shared())),
            call_id,
            tool_name,
            first_chunk_sent: false,
        }
    }

    pub fn from_mpsc(
        rx: mpsc::Receiver<(String, Result<Value, String>)>,
        call_id: String,
        tool_name: String,
    ) -> Self {
        Self {
            inner: Some(StreamInner::Multi(rx)),
            call_id,
            tool_name,
            first_chunk_sent: false,
        }
    }

    /// Create a pair of streams from a one-shot future, cloning the shared future internally.
    pub fn new_oneshot_shared(
        call_id: String,
        tool_name: String,
        rx: oneshot::Receiver<Result<Value>>,
    ) -> (Self, Self) {
        let call_id_ = call_id.clone();
        let shared_future = async move {
            match rx.await {
                Ok(r) => (call_id_.clone(), r.map_err(|e| e.to_string())),
                Err(_) => (call_id_.clone(), Err("Channel closed".to_string())),
            }
        }
        .boxed()
        .shared();
        (
            ToolResultStream::from_shared(
                shared_future.clone(),
                call_id.clone(),
                tool_name.clone(),
            ),
            ToolResultStream::from_shared(shared_future, call_id, tool_name),
        )
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

/// Type-erased request that can hold any tool request as JSON
#[derive(Debug, Clone)]
pub struct SchedulerRequest {
    pub tool_name: String,
    pub payload: Value,
}
