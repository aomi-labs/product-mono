use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use eyre::Result;
use tokio::sync::Mutex;

use super::OperationGroup;
use super::executor::ForgeExecutor;
use super::plan::GroupStatus;
use super::resources::SharedForgeResources;
use super::types::GroupResult;

static PLAN_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct ForgeManager {
    executors: DashMap<String, Arc<Mutex<ForgeExecutor>>>,
    shared: Arc<SharedForgeResources>,
}

impl ForgeManager {
    pub async fn new() -> Result<Self> {
        let shared = SharedForgeResources::new().await?;
        Ok(Self {
            executors: DashMap::new(),
            shared: Arc::new(shared),
        })
    }

    pub async fn create_plan(&self, groups: Vec<OperationGroup>) -> Result<(String, usize)> {
        let total_groups = groups.len();
        let executor = ForgeExecutor::new_with_resources(groups, Arc::clone(&self.shared)).await?;
        let plan_id = Self::next_plan_id();

        self.executors
            .insert(plan_id.clone(), Arc::new(Mutex::new(executor)));

        Ok((plan_id, total_groups))
    }

    pub async fn next_groups(&self, plan_id: &str) -> Result<(Vec<GroupResult>, usize)> {
        let executor = self
            .executors
            .get(plan_id)
            .ok_or_else(|| eyre::eyre!("No execution plan found for plan_id: {plan_id}"))?;
        let mut executor = executor.value().lock().await;

        let results = executor.next_groups().await?;
        let remaining_groups = executor
            .plan
            .statuses
            .iter()
            .filter(|s| matches!(s, GroupStatus::Todo))
            .count();

        drop(executor);

        Ok((results, remaining_groups))
    }

    fn next_plan_id() -> String {
        let counter = PLAN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("plan-{}-{}", nanos, counter)
    }
}
