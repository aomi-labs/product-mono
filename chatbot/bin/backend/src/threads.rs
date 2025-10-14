use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use tokio::sync::{mpsc, Mutex, Semaphore};

use aomi_terminal::{ChatTerminal, ChatInput, ChatStatus, ChatState};

#[derive(Debug, Clone)]
pub enum ThreadEvent {
    StreamingText(String),
    ToolCall {
        invocation: ToolInvocation,
        result: Option<String>,
    },
    Complete,
}

#[derive(Debug, Clone)]
pub struct ToolInvocation {
    pub name: String,
    pub args: String,
}

pub struct Worker {
    id: usize,
    app: Arc<ChatTerminal>,
    // Each worker will have its own BamlClient when integrated
    // baml_client: TerminalBamlClient,
}

impl Worker {
    fn new(id: usize, app: Arc<ChatTerminal>) -> Self {
        Self { id, app }
    }

    pub async fn run(
        &self,
        session_id: &str,
        state: ChatState,
        input: ChatInput,
    ) -> (ChatState, Vec<ThreadEvent>) {
        // TODO: This will integrate with BamlClient for actual LLM processing
        // For now, simulate async processing with Future-based tool calls
        let mut events = Vec::new();
        
        // Simulate streaming text
        events.push(ThreadEvent::StreamingText(format!(
            "Processing message for session {}: {}",
            session_id, input.latest_message
        )));
        
        // TODO: When BamlClient is integrated:
        // 1. Call app.process(input, state, baml_client)
        // 2. Match on TerminalOutput:
        //    - Processing(IntermediateOutput) -> await tool execution
        //    - Complete -> return final state
        
        // For now, just update state
        let mut new_state = state;
        new_state.history.push(format!("user: {}", input.latest_message));
        new_state.history.push(format!("assistant: Processed"));
        
        events.push(ThreadEvent::Complete);
        
        (new_state, events)
    }
}

pub struct ThreadHandle {
    thread: Arc<Worker>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl ThreadHandle {
    pub fn thread(&self) -> &Worker {
        &self.thread
    }
}

pub struct ThreadPool {
    workers: Vec<Arc<Worker>>,
    semaphore: Arc<Semaphore>,
    // Queue for sessions waiting to be processed
    session_queue: Arc<Mutex<VecDeque<SessionWork>>>,
}

#[derive(Debug)]
struct SessionWork {
    session_id: String,
    input: ChatInput,
    state: ChatState,
    result_tx: mpsc::Sender<(ChatState, Vec<ThreadEvent>)>,
}

impl ThreadPool {
    pub fn new(size: usize, app: Arc<ChatTerminal>) -> Self {
        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Arc::new(Worker::new(id, Arc::clone(&app))));
        }

        let pool = Self {
            workers,
            semaphore: Arc::new(Semaphore::new(size)),
            session_queue: Arc::new(Mutex::new(VecDeque::new())),
        };

        // Start the worker dispatch loop
        pool.start_dispatch_loop();
        
        pool
    }

    fn start_dispatch_loop(&self) {
        let workers = self.workers.clone();
        let semaphore = Arc::clone(&self.semaphore);
        let queue = Arc::clone(&self.session_queue);

        tokio::spawn(async move {
            loop {
                // Wait for available worker
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                
                // Check for work
                let work = {
                    let mut q = queue.lock().await;
                    q.pop_front()
                };

                if let Some(work) = work {
                    // Assign work to a worker
                    let worker = workers[0].clone(); // TODO: Round-robin or load balancing
                    
                    tokio::spawn(async move {
                        // Process the work asynchronously
                        let (state, events) = worker.run(&work.session_id, work.state, work.input).await;
                        
                        // Send result back
                        let _ = work.result_tx.send((state, events)).await;
                        
                        // Release permit when done
                        drop(permit);
                    });
                } else {
                    // No work, release permit and wait a bit
                    drop(permit);
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            }
        });
    }

    pub async fn acquire(&self) -> Result<ThreadHandle, anyhow::Error> {
        let permit = self.semaphore.clone()
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to acquire thread permit"))?;

        // For now, just return the first worker
        // TODO: Implement proper load balancing
        let thread = Arc::clone(&self.workers[0]);

        Ok(ThreadHandle {
            thread,
            _permit: permit,
        })
    }

    pub async fn submit_work(
        &self,
        session_id: String,
        state: ChatState,
        input: ChatInput,
    ) -> Result<(ChatState, Vec<ThreadEvent>), anyhow::Error> {
        let (tx, mut rx) = mpsc::channel(1);
        
        let work = SessionWork {
            session_id,
            input,
            state,
            result_tx: tx,
        };

        {
            let mut queue = self.session_queue.lock().await;
            queue.push_back(work);
        }

        rx.recv().await
            .ok_or_else(|| anyhow::anyhow!("Failed to receive processing result"))
    }
}

// Future implementation for async tool execution
pub struct ToolExecutionFuture {
    tool_name: String,
    inner: Pin<Box<dyn Future<Output = String> + Send>>,
}

impl Future for ToolExecutionFuture {
    type Output = String;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}