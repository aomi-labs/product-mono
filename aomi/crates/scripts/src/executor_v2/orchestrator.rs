use crate::executor_v2::{ExecutionBackend, GroupConfig, GroupNode, GroupNodeHandle, NodeId};
use crate::forge_executor::plan::{ExecutionPlan, GroupStatus, OperationGroup};
use crate::forge_executor::types::{GroupResult, GroupResultInner};
use crate::forge_executor::SourceFetcher;
use anyhow::{Result, anyhow};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct GroupReceipt {
    pub execution_id: String,
    pub node_id: NodeId,
    pub ops_cnt: usize,
    pub description: String,
}

pub struct ResultState {
    pub sender: mpsc::Sender<GroupResult>,
    pub results: Vec<GroupResult>,
}

pub struct ForgeOrchestrator {
    plans: DashMap<String, ExecutionPlan>,
    results: DashMap<String, ResultState>,
    active_nodes: Vec<GroupNodeHandle>,
    backend: Arc<ExecutionBackend>,
    source_fetcher: Arc<SourceFetcher>,
    baml_client: Arc<aomi_baml::BamlClient>,
    config: Arc<GroupConfig>,
}

impl ForgeOrchestrator {
    pub fn new(
        backend: Arc<ExecutionBackend>,
        source_fetcher: Arc<SourceFetcher>,
        baml_client: Arc<aomi_baml::BamlClient>,
        config: Arc<GroupConfig>,
    ) -> Self {
        Self {
            plans: DashMap::new(),
            results: DashMap::new(),
            active_nodes: Vec::new(),
            backend,
            source_fetcher,
            baml_client,
            config,
        }
    }

    pub fn create_plan(
        &self,
        execution_id: String,
        groups: Vec<OperationGroup>,
        sender: mpsc::Sender<GroupResult>,
    ) -> Result<()> {
        let plan = ExecutionPlan::from(groups);
        self.plans.insert(execution_id.clone(), plan);
        self.results.insert(
            execution_id,
            ResultState {
                sender,
                results: Vec::new(),
            },
        );
        Ok(())
    }

    pub fn next_groups(&mut self, execution_id: &str) -> Result<Vec<GroupReceipt>> {
        let mut plan = self
            .plans
            .get_mut(execution_id)
            .ok_or_else(|| anyhow!("missing execution plan: {execution_id}"))?;
        let ready = plan.next_ready_batch();
        if ready.is_empty() {
            return Ok(Vec::new());
        }

        plan.mark_in_progress(&ready);

        let mut receipts = Vec::with_capacity(ready.len());
        for idx in ready {
            let group = plan
                .groups
                .get(idx)
                .cloned()
                .ok_or_else(|| anyhow!("missing group {idx}"))?;
            let node = GroupNode::new(
                execution_id.to_string(),
                idx,
                group.clone(),
                Arc::clone(&self.config),
                Arc::clone(&self.backend),
                Arc::clone(&self.source_fetcher),
                Arc::clone(&self.baml_client),
            );
            self.active_nodes.push(GroupNodeHandle::spawn(node));
            receipts.push(GroupReceipt {
                execution_id: execution_id.to_string(),
                node_id: idx,
                ops_cnt: group.operations.len(),
                description: group.description,
            });
        }

        Ok(receipts)
    }

    pub async fn poll_active_nodes(&mut self) -> Result<()> {
        let mut pending = Vec::new();
        for handle in self.active_nodes.drain(..) {
            if handle.is_finished() {
                let result = handle.take_result().await?;
                self.on_node_complete(handle.execution_id, handle.node_id, result)?;
            } else {
                pending.push(handle);
            }
        }
        self.active_nodes = pending;
        Ok(())
    }

    fn on_node_complete(
        &self,
        execution_id: String,
        node_id: NodeId,
        result: GroupResult,
    ) -> Result<()> {
        if let Some(mut plan) = self.plans.get_mut(&execution_id) {
            match &result.inner {
                GroupResultInner::Done {
                    transactions,
                    generated_code,
                } => {
                    plan.mark_done(node_id, transactions.clone(), generated_code.clone());
                }
                GroupResultInner::Failed { error, .. } => {
                    plan.mark_failed(node_id, error.clone());
                }
            }
        }

        if let Some(mut state) = self.results.get_mut(&execution_id) {
            state.results.push(result.clone());
            let _ = state.sender.try_send(result);
        }

        Ok(())
    }
}
