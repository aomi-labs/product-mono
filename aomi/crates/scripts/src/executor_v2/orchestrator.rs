use dashmap::DashMap;
use eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

use super::{ExecutionBackend, GroupConfig, GroupNode};
use crate::forge_executor::{
    plan::{ExecutionPlan, GroupStatus, OperationGroup},
    source_fetcher::SourceFetcher,
    types::{GroupResult, GroupResultInner},
};
use aomi_tools::clients::external_clients;

pub type ExecutionId = String;
pub type NodeHandle = tokio::task::JoinHandle<GroupResult>;
pub type GroupNodes = Vec<Option<GroupNode>>;
pub type ChainId = u64;

/// Result state for async dispatch
#[derive(Clone)]
struct ResultState {
    sender: mpsc::UnboundedSender<GroupResult>,
    results: Vec<GroupResult>,
}

/// Receipt returned when groups are spawned
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupReceipt {
    pub execution_id: String,
    pub node_id: usize,
    pub description: String,
    pub ops_count: usize,
}

/// 1 plan → 1 backend → [node1, node2, ...]
///                    → [chain1, chain2, ...]
/// Multiple plans → multiple backends
pub struct ForgeOrchestrator {
    /// Lightweight plans (structure + status only)
    plans: DashMap<ExecutionId, ExecutionPlan>,

    /// In-memory heavy tree states (pre-created nodes before spawning)
    execution_trees: Arc<DashMap<ExecutionId, GroupNodes>>,

    /// Active group nodes (tracking only)
    active_nodes: Arc<DashMap<(ExecutionId, usize), ()>>,

    /// Result dispatch
    results: DashMap<ExecutionId, ResultState>,

    /// Per-plan backends (one backend per execution plan)
    backends: Arc<DashMap<ExecutionId, Arc<ExecutionBackend>>>,

    /// Shared global resources
    source_fetcher: Arc<SourceFetcher>,
    baml_client: Arc<aomi_baml::BamlClient>,
}

impl ForgeOrchestrator {
    pub async fn new() -> Result<Self> {
        let source_fetcher = Arc::new(SourceFetcher::new());

        // Get BAML client from external clients
        let clients = external_clients().await;
        let baml_client = clients.baml_client()?;

        Ok(Self {
            plans: DashMap::new(),
            execution_trees: Arc::new(DashMap::new()),
            active_nodes: Arc::new(DashMap::new()),
            results: DashMap::new(),
            backends: Arc::new(DashMap::new()),
            source_fetcher,
            baml_client,
        })
    }

    /// Create a new execution plan
    /// 1 plan → 1 backend → [nodes...]
    ///
    /// The caller provides a sender for async result dispatch.
    /// Results will be sent through this channel as groups complete.
    pub async fn create_plan(
        &self,
        execution_id: ExecutionId,
        groups: Vec<OperationGroup>,
        result_sender: mpsc::UnboundedSender<GroupResult>,
    ) -> Result<usize> {
        let total_groups = groups.len();

        // Collect target chains
        let target_chains = Self::collect_target_chain_ids(&groups);

        // Create per-plan backend
        let base_foundry_config = foundry_config::Config::default();
        let backend = Arc::new(
            ExecutionBackend::new(&target_chains, &base_foundry_config).await?
        );
        self.backends.insert(execution_id.clone(), backend.clone());

        // Start background source fetching
        let all_contracts = Self::collect_unique_contracts(&groups);
        self.source_fetcher.request_fetch(all_contracts);

        // Create lightweight plan
        let plan = ExecutionPlan::from(groups.clone());
        self.plans.insert(execution_id.clone(), plan);

        // Pre-create GroupNodes (in-memory tree state)
        let mut nodes = Vec::new();
        for (idx, group) in groups.into_iter().enumerate() {
            let primary_chain = Self::get_primary_chain_id(&group)?;
            let config = GroupConfig::for_chain(
                base_foundry_config.clone(),
                backend.get_evm_opts(primary_chain)?,
                false,
                None,
            );

            let node = GroupNode::new(
                idx.to_string(),
                group,
                config,  // owned by node
                backend.clone(),
                self.source_fetcher.clone(),
                self.baml_client.clone(),
            );
            nodes.push(Some(node));
        }
        self.execution_trees.insert(execution_id.clone(), nodes);

        // Store result state with caller-provided sender
        self.results.insert(
            execution_id.clone(),
            ResultState {
                sender: result_sender,
                results: Vec::new(),
            },
        );

        Ok(total_groups)
    }

    fn get_primary_chain_id(group: &OperationGroup) -> Result<ChainId> {
        group
            .contracts
            .first()
            .map(|(chain_id, _, _)| chain_id.parse::<ChainId>())
            .unwrap_or(Ok(1))
            .map_err(|e| eyre!("Invalid chain_id: {}", e))
    }

    /// Execute next batch of ready groups
    /// Pull pre-created nodes from execution_trees and spawn them
    pub async fn next_groups(&self, execution_id: &str) -> Result<Vec<GroupReceipt>> {
        let mut plan = self.plans.get_mut(execution_id)
            .ok_or_else(|| eyre!("No plan found for {}", execution_id))?;

        // Find ready groups
        let ready_indices = plan.next_ready_batch();
        if ready_indices.is_empty() {
            return Ok(vec![]);
        }

        // Mark in progress
        plan.mark_in_progress(&ready_indices);

        // Wait for sources
        let ready_groups: Vec<&OperationGroup> = ready_indices
            .iter()
            .map(|&idx| &plan.groups[idx])
            .collect();
        Self::wait_for_sources(&self.source_fetcher, &ready_groups).await?;

        // Get pre-created nodes from execution_tree
        let mut execution_tree = self.execution_trees
            .get_mut(execution_id)
            .ok_or_else(|| eyre!("No execution tree for {}", execution_id))?;

        // Spawn nodes
        let mut receipts = Vec::new();

        for &group_idx in &ready_indices {
            // Take ownership of node from tree (nodes are moved, not cloned)
            let node = execution_tree[group_idx]
                .take()
                .ok_or_else(|| eyre!("GroupNode already moved for {}", group_idx))?;

            let description = node.group.description.clone();
            let ops_count = node.group.operations.len();

            let handle = tokio::spawn(async move { node.run().await });

            self.active_nodes
                .insert((execution_id.to_string(), group_idx), ());
            self.spawn_result_handler(execution_id.to_string(), group_idx, handle);

            receipts.push(GroupReceipt {
                execution_id: execution_id.to_string(),
                node_id: group_idx,
                description,
                ops_count,
            });
        }

        Ok(receipts)
    }

    /// Spawn per-node result handler (avoids global polling)
    fn spawn_result_handler(
        &self,
        exec_id: ExecutionId,
        node_id: usize,
        handle: NodeHandle,
    ) {
        let active_nodes = self.active_nodes.clone();
        let plans = self.plans.clone();
        let results = self.results.clone();
        let backends = self.backends.clone();
        let execution_trees = self.execution_trees.clone();

        tokio::spawn(async move {
            let result = handle.await;
            active_nodes.remove(&(exec_id.clone(), node_id));

            let mut plan_complete = false;

            match result {
                Ok(result) => {
                    // Update plan status
                    if let Some(mut plan) = plans.get_mut(&exec_id) {
                        match &result.inner {
                            GroupResultInner::Done { transactions, generated_code } => {
                                plan.mark_done(node_id, transactions.clone(), generated_code.clone());
                            }
                            GroupResultInner::Failed { error, .. } => {
                                plan.mark_failed(node_id, error.clone());
                            }
                        }

                        // Check if plan is complete (all groups Done or Failed)
                        plan_complete = plan.statuses.iter().all(|status| {
                            matches!(status, GroupStatus::Done { .. } | GroupStatus::Failed { .. })
                        });
                    }

                    // Append result and dispatch
                    if let Some(mut state) = results.get_mut(&exec_id) {
                        state.results.push(result.clone());
                        let _ = state.sender.send(result);
                    }
                }
                Err(e) => {
                    tracing::error!("Node task panicked: {}", e);
                    if let Some(mut plan) = plans.get_mut(&exec_id) {
                        plan.mark_failed(node_id, e.to_string());
                    }
                }
            }

            // Cleanup when plan is complete
            if plan_complete {
                tracing::info!(execution_id = %exec_id, "Plan complete, cleaning up resources");

                // Remove plan
                plans.remove(&exec_id);

                // Remove execution tree
                execution_trees.remove(&exec_id);

                // Remove backend
                backends.remove(&exec_id);

                // Remove results (this drops the sender, closing the channel)
                results.remove(&exec_id);

                tracing::debug!(execution_id = %exec_id, "Cleanup complete");
            }
        });
    }

    /// Get accumulated results for an execution
    /// Note: Results are also sent through the channel provided to create_plan()
    pub fn get_results(&self, execution_id: &str) -> Vec<GroupResult> {
        self.results
            .get(execution_id)
            .map(|state| state.results.clone())
            .unwrap_or_default()
    }

    /// Check if plan is complete
    pub fn is_plan_complete(&self, execution_id: &str) -> bool {
        if let Some(plan) = self.plans.get(execution_id) {
            plan.statuses.iter().all(|status| {
                matches!(status, GroupStatus::Done { .. } | GroupStatus::Failed { .. })
            })
        } else {
            false
        }
    }

    /// Get remaining groups count
    pub fn remaining_groups(&self, execution_id: &str) -> usize {
        if let Some(plan) = self.plans.get(execution_id) {
            plan.statuses.iter().filter(|status| {
                matches!(status, GroupStatus::Todo | GroupStatus::InProgress)
            }).count()
        } else {
            0
        }
    }

    fn collect_unique_contracts(groups: &[OperationGroup]) -> Vec<(String, String, String)> {
        groups
            .iter()
            .flat_map(|g| g.contracts.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    fn collect_target_chain_ids(groups: &[OperationGroup]) -> HashSet<ChainId> {
        groups
            .iter()
            .flat_map(|g| g.contracts.iter().map(|(chain_id, _, _)| chain_id.parse::<ChainId>().unwrap_or(1)))
            .collect()
    }

    async fn wait_for_sources(
        source_fetcher: &SourceFetcher,
        groups: &[&OperationGroup],
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(60);

        while !source_fetcher.are_contracts_ready(groups).await {
            if Instant::now() > deadline {
                let missing = source_fetcher
                    .missing_contracts(groups)
                    .await
                    .iter()
                    .map(|(chain, addr, name)| format!("{}:{} ({})", chain, addr, name))
                    .collect::<Vec<_>>()
                    .join(", ");
                eyre::bail!("Timeout waiting for contract sources. Missing: {}", missing);
            }

            let missing = source_fetcher.missing_contracts(groups).await;
            if !missing.is_empty() {
                source_fetcher.request_fetch(missing);
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let orchestrator = ForgeOrchestrator::new().await;
        assert!(orchestrator.is_ok());
    }

    #[tokio::test]
    async fn test_create_plan_no_groups() {
        let orchestrator = ForgeOrchestrator::new().await.unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel();
        let result = orchestrator.create_plan("test_plan".to_string(), vec![], sender).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }
}
