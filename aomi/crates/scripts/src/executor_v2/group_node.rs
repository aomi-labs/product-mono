use crate::executor_v2::{ExecutionBackend, GroupArtifacts, GroupConfig};
use crate::forge_executor::plan::OperationGroup;
use crate::forge_executor::types::{GroupResult, GroupResultInner};
use crate::forge_executor::SourceFetcher;
use anyhow::Result;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub type NodeId = usize;

pub struct GroupNode {
    pub execution_id: String,
    pub node_id: NodeId,
    pub group: OperationGroup,
    pub artifacts: GroupArtifacts,
    pub config: Arc<GroupConfig>,
    pub backend: Arc<ExecutionBackend>,
    pub source_fetcher: Arc<SourceFetcher>,
    pub baml_client: Arc<aomi_baml::BamlClient>,
}

impl GroupNode {
    pub fn new(
        execution_id: String,
        node_id: NodeId,
        group: OperationGroup,
        config: Arc<GroupConfig>,
        backend: Arc<ExecutionBackend>,
        source_fetcher: Arc<SourceFetcher>,
        baml_client: Arc<aomi_baml::BamlClient>,
    ) -> Self {
        Self {
            execution_id,
            node_id,
            group,
            artifacts: GroupArtifacts::new(),
            config,
            backend,
            source_fetcher,
            baml_client,
        }
    }

    pub async fn run(&mut self) -> Result<GroupResult> {
        Ok(GroupResult {
            group_index: self.node_id,
            description: self.group.description.clone(),
            operations: self.group.operations.clone(),
            inner: GroupResultInner::Failed {
                error: "executor_v2 not implemented".to_string(),
                generated_code: String::new(),
                transactions: vec![],
            },
        })
    }
}

pub struct GroupNodeHandle {
    pub execution_id: String,
    pub node_id: NodeId,
    join: JoinHandle<Result<GroupResult>>,
}

impl GroupNodeHandle {
    pub fn spawn(node: GroupNode) -> Self {
        let execution_id = node.execution_id.clone();
        let node_id = node.node_id;
        let join = tokio::spawn(async move {
            let mut node = node;
            node.run().await
        });
        Self {
            execution_id,
            node_id,
            join,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.join.is_finished()
    }

    pub async fn take_result(self) -> Result<GroupResult> {
        self.join.await?
    }
}
