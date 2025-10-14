use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::{HashMap, VecDeque};
use tokio::sync::{oneshot, mpsc};
use tokio::task::JoinHandle;
use aomi_terminal::{BamlClient, ChatTerminal, ChatState, ChatInput, ChatStatus};

// Worker represents a logical worker that can handle tasks
#[derive(Debug)]
pub struct Worker {
    id: usize,
    status: WorkerStatus,
    current_task_id: Option<usize>,
    baml_client: BamlClient,
}

#[derive(Debug, Clone)]
pub enum WorkerStatus {
    Idle,
    Processing,
    Terminated,
}

impl Worker {
    fn new(id: usize) -> Self {
        Worker {
            id,
            status: WorkerStatus::Idle,
            current_task_id: None,
            baml_client: BamlClient::new(),
        }
    }
    
    fn assign_task(&mut self, task_id: usize) {
        self.status = WorkerStatus::Processing;
        self.current_task_id = Some(task_id);
    }
    
    fn complete_task(&mut self) {
        self.status = WorkerStatus::Idle;
        self.current_task_id = None;
    }
}

// Task represents a task with its state and result channel
#[derive(Debug)]
pub struct Task {
    id: usize,
    status: TaskStatus,
    input: ChatInput,
    result_tx: Option<oneshot::Sender<ChatState>>,
    tokio_handle: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub struct TaskHandle<T> {
    task_id: usize,
    receiver: oneshot::Receiver<T>,
}

impl<T> TaskHandle<T> {
    pub async fn await_result(self) -> Result<T, oneshot::error::RecvError> {
        self.receiver.await
    }
    
    pub fn try_recv(&mut self) -> Result<T, oneshot::error::TryRecvError> {
        self.receiver.try_recv()
    }
    
    pub fn task_id(&self) -> usize {
        self.task_id
    }
}
// Pool state for tracking workers and tasks
struct PoolState {
    workers: HashMap<usize, Worker>,
    tasks: HashMap<usize, Task>,
    next_task_id: usize,
    task_queue: VecDeque<usize>, // Queue of pending task IDs
}

pub struct ThreadPool {
    state: Arc<Mutex<PoolState>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    chat_terminal: Arc<ChatTerminal>,
    runtime: tokio::runtime::Runtime,
}

impl ThreadPool {

    pub fn new(worker_count: usize, max_concurrent: usize) -> Self {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let chat_terminal = Arc::new(ChatTerminal::new());
        
        // Initialize workers
        let mut workers = HashMap::new();
        for id in 0..worker_count {
            workers.insert(id, Worker::new(id));
        }
        
        let state = Arc::new(Mutex::new(PoolState {
            workers,
            tasks: HashMap::new(),
            next_task_id: 0,
            task_queue: VecDeque::new(),
        }));
        
        // Start persistent worker loops
        for worker_id in 0..worker_count {
            let pool_state = Arc::clone(&state);
            let pool_semaphore = Arc::clone(&semaphore);
            let pool_terminal = Arc::clone(&chat_terminal);
            
            runtime.spawn(async move {
                Self::worker_loop(worker_id, pool_state, pool_semaphore, pool_terminal).await;
            });
        }
        
        ThreadPool {
            state,
            semaphore,
            chat_terminal,
            runtime,
        }
    }


    pub fn spawn_with_task(&self, input: ChatInput) -> TaskHandle<ChatState> {
        let (tx, rx) = oneshot::channel();
        
        // Create task and add to queue
        let task_id = {
            let mut state = self.state.lock().unwrap();
            let task_id = state.next_task_id;
            state.next_task_id += 1;
            
            let task = Task {
                id: task_id,
                status: TaskStatus::Pending,
                input: input.clone(),
                result_tx: Some(tx),
                tokio_handle: None,
            };
            
            state.tasks.insert(task_id, task);
            state.task_queue.push_back(task_id); // Add to queue for workers to pick up
            task_id
        };
        
        TaskHandle {
            task_id,
            receiver: rx,
        }
    }
    
    // Persistent worker loop - each worker runs this continuously
    async fn worker_loop(
        worker_id: usize,
        state: Arc<Mutex<PoolState>>,
        semaphore: Arc<tokio::sync::Semaphore>,
        terminal: Arc<ChatTerminal>,
    ) {
        loop {
            // Try to get a task from the queue
            let task_info = {
                let mut pool_state = state.lock().unwrap();
                
                // Check if this worker is available and there's a pending task
                if let Some(worker) = pool_state.workers.get(&worker_id) {
                    if !matches!(worker.status, WorkerStatus::Idle) {
                        // Worker is busy or terminated
                        if matches!(worker.status, WorkerStatus::Terminated) {
                            break; // Exit worker loop
                        }
                        drop(pool_state);
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                        continue;
                    }
                }
                
                // Try to get next task from queue
                if let Some(task_id) = pool_state.task_queue.pop_front() {
                    if let Some(task) = pool_state.tasks.get(&task_id) {
                        // Assign task to this worker
                        if let Some(worker) = pool_state.workers.get_mut(&worker_id) {
                            worker.assign_task(task_id);
                        }
                        
                        Some((task_id, task.input.clone()))
                    } else {
                        None // Task was removed
                    }
                } else {
                    None // No tasks in queue
                }
            };
            
            if let Some((task_id, input)) = task_info {
                // Mark task as running
                {
                    let mut pool_state = state.lock().unwrap();
                    if let Some(task) = pool_state.tasks.get_mut(&task_id) {
                        task.status = TaskStatus::Running;
                    }
                }
                
                // CRITICAL: Free the worker immediately after starting the async operation
                {
                    let mut pool_state = state.lock().unwrap();
                    if let Some(worker) = pool_state.workers.get_mut(&worker_id) {
                        worker.complete_task(); // Worker becomes available for next task
                    }
                }
                
                // Spawn the long-running API call as independent task
                let state_clone = Arc::clone(&state);
                let terminal_clone = Arc::clone(&terminal);
                let semaphore_clone = Arc::clone(&semaphore);
                
                tokio::spawn(async move {
                    // Acquire semaphore permit to limit concurrent API calls
                    let _permit = semaphore_clone.acquire().await.unwrap();
                    
                    // This is the long-running API call - worker is already free!
                    let mut chat_state = ChatState::new();
                    chat_state.set_status(ChatStatus::Processing);
                    let result = terminal_clone.run_chat(input, chat_state).await;
                    
                    // Complete the task when API finishes
                    let tx = {
                        let mut pool_state = state_clone.lock().unwrap();
                        if let Some(mut task) = pool_state.tasks.remove(&task_id) {
                            task.status = TaskStatus::Completed;
                            task.result_tx.take()
                        } else {
                            None
                        }
                    };
                    
                    // Send result to the handle
                    if let Some(tx) = tx {
                        let _ = tx.send(result);
                    }
                });
                
                // Worker immediately continues to next iteration to pick up more tasks
            } else {
                // No tasks available, sleep briefly before checking again
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        }
    }



    // Get current pool statistics
    pub fn get_stats(&self) -> PoolStats {
        let state = self.state.lock().unwrap();
        
        let idle_workers = state.workers.values()
            .filter(|w| matches!(w.status, WorkerStatus::Idle))
            .count();
            
        let processing_workers = state.workers.values()
            .filter(|w| matches!(w.status, WorkerStatus::Processing))
            .count();
            
        let pending_tasks = state.tasks.values()
            .filter(|t| matches!(t.status, TaskStatus::Pending))
            .count();
            
        let running_tasks = state.tasks.values()
            .filter(|t| matches!(t.status, TaskStatus::Running))
            .count();
        
        PoolStats {
            total_workers: state.workers.len(),
            idle_workers,
            processing_workers,
            total_tasks: state.tasks.len(),
            pending_tasks,
            running_tasks,
            available_permits: self.semaphore.available_permits(),
        }
    }
    
    // Get task status by ID
    pub fn get_task_status(&self, task_id: usize) -> Option<TaskStatus> {
        let state = self.state.lock().unwrap();
        state.tasks.get(&task_id).map(|t| t.status.clone())
    }
    
    // Get worker statuses
    pub fn get_worker_statuses(&self) -> Vec<(usize, WorkerStatus)> {
        let state = self.state.lock().unwrap();
        state.workers.iter()
            .map(|(id, worker)| (*id, worker.status.clone()))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_workers: usize,
    pub idle_workers: usize,
    pub processing_workers: usize,
    pub total_tasks: usize,
    pub pending_tasks: usize,
    pub running_tasks: usize,
    pub available_permits: usize,
}



}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        // Mark all workers as terminated
        let mut state = self.state.lock().unwrap();
        for worker in state.workers.values_mut() {
            worker.status = WorkerStatus::Terminated;
        }
        
        // Abort any remaining tokio tasks
        for task in state.tasks.values_mut() {
            if let Some(handle) = &task.tokio_handle {
                handle.abort();
            }
        }
    }
}