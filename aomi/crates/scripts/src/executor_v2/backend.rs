use anyhow::Result;
use foundry_evm::backend::Backend;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ExecutionBackend {
    backend: Arc<Mutex<Backend>>,
}

impl ExecutionBackend {
    pub fn new(backend: Backend) -> Self {
        Self {
            backend: Arc::new(Mutex::new(backend)),
        }
    }

    pub fn shared(&self) -> Arc<Mutex<Backend>> {
        Arc::clone(&self.backend)
    }

    pub async fn execute_on_chain<F, R>(&self, _chain_id: u64, f: F) -> Result<R>
    where
        F: FnOnce(&mut Backend) -> Result<R>,
    {
        let mut backend = self.backend.lock().await;
        f(&mut backend)
    }
}
