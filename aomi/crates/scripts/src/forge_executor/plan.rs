use serde::{Deserialize, Serialize};

use super::types::TransactionData;

/// An operation group with dependencies
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationGroup {
    pub description: String,
    pub operations: Vec<String>,
    pub dependencies: Vec<usize>,
    pub contracts: Vec<(String, String, String)>, // (chain_id, address, name)
}

/// Execution status for a group
#[derive(Clone, Debug, PartialEq)]
pub enum GroupStatus {
    Todo,
    InProgress,
    Done {
        transactions: Vec<TransactionData>,
        generated_code: String,
    },
    Failed {
        error: String,
    },
}

/// Execution plan with dependency tracking
#[derive(Clone, Debug)]
pub struct ExecutionPlan {
    pub groups: Vec<OperationGroup>,
    pub statuses: Vec<GroupStatus>,
}

impl ExecutionPlan {
    /// Create execution plan from operation groups
    pub fn from(groups: Vec<OperationGroup>) -> Self {
        let statuses = vec![GroupStatus::Todo; groups.len()];
        Self { groups, statuses }
    }

    /// Get next batch of ready groups (dependencies satisfied, status is Todo)
    pub fn next_ready_batch(&self) -> Vec<usize> {
        self.groups
            .iter()
            .enumerate()
            .filter(|(idx, group)| {
                // Must be Todo
                matches!(self.statuses[*idx], GroupStatus::Todo)
                    // All dependencies must be Done
                    && group.dependencies.iter().all(|dep_idx| {
                        matches!(self.statuses[*dep_idx], GroupStatus::Done { .. })
                    })
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Mark groups as in progress
    pub fn mark_in_progress(&mut self, indices: &[usize]) {
        for &idx in indices {
            self.statuses[idx] = GroupStatus::InProgress;
        }
    }

    /// Mark group as done with transactions and generated code
    pub fn mark_done(
        &mut self,
        idx: usize,
        transactions: Vec<TransactionData>,
        generated_code: String,
    ) {
        self.statuses[idx] = GroupStatus::Done {
            transactions,
            generated_code,
        };
    }

    /// Mark group as failed
    pub fn mark_failed(&mut self, idx: usize, error: String) {
        self.statuses[idx] = GroupStatus::Failed { error };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_ready_batch_no_dependencies() {
        let groups = vec![
            OperationGroup {
                description: "Group 0".to_string(),
                operations: vec!["op1".to_string()],
                dependencies: vec![],
                contracts: vec![],
            },
            OperationGroup {
                description: "Group 1".to_string(),
                operations: vec!["op2".to_string()],
                dependencies: vec![],
                contracts: vec![],
            },
        ];

        let plan = ExecutionPlan::from(groups);
        let ready = plan.next_ready_batch();

        assert_eq!(ready, vec![0, 1]); // Both ready
    }

    #[test]
    fn test_next_ready_batch_with_dependencies() {
        let groups = vec![
            OperationGroup {
                description: "Group 0".to_string(),
                operations: vec!["op1".to_string()],
                dependencies: vec![],
                contracts: vec![],
            },
            OperationGroup {
                description: "Group 1".to_string(),
                operations: vec!["op2".to_string()],
                dependencies: vec![0], // Depends on group 0
                contracts: vec![],
            },
        ];

        let mut plan = ExecutionPlan::from(groups);
        let ready = plan.next_ready_batch();

        assert_eq!(ready, vec![0]); // Only group 0 is ready

        // Mark group 0 as done
        plan.mark_done(0, vec![], String::new());

        let ready = plan.next_ready_batch();
        assert_eq!(ready, vec![1]); // Now group 1 is ready
    }

    #[test]
    fn test_next_ready_batch_skip_in_progress() {
        let groups = vec![OperationGroup {
            description: "Group 0".to_string(),
            operations: vec!["op1".to_string()],
            dependencies: vec![],
            contracts: vec![],
        }];

        let mut plan = ExecutionPlan::from(groups);
        plan.mark_in_progress(&[0]);

        let ready = plan.next_ready_batch();
        assert_eq!(ready, Vec::<usize>::new()); // In progress, not ready
    }

    #[test]
    fn test_complex_dependency_chain() {
        // 0 (no deps)
        // 1 depends on 0
        // 2 depends on 0
        // 3 depends on 1 and 2
        let groups = vec![
            OperationGroup {
                description: "Group 0".to_string(),
                operations: vec!["op1".to_string()],
                dependencies: vec![],
                contracts: vec![],
            },
            OperationGroup {
                description: "Group 1".to_string(),
                operations: vec!["op2".to_string()],
                dependencies: vec![0],
                contracts: vec![],
            },
            OperationGroup {
                description: "Group 2".to_string(),
                operations: vec!["op3".to_string()],
                dependencies: vec![0],
                contracts: vec![],
            },
            OperationGroup {
                description: "Group 3".to_string(),
                operations: vec!["op4".to_string()],
                dependencies: vec![1, 2],
                contracts: vec![],
            },
        ];

        let mut plan = ExecutionPlan::from(groups);

        // Initially only group 0 is ready
        assert_eq!(plan.next_ready_batch(), vec![0]);

        // Complete group 0
        plan.mark_done(0, vec![], String::new());

        // Now groups 1 and 2 are ready
        assert_eq!(plan.next_ready_batch(), vec![1, 2]);

        // Complete groups 1 and 2
        plan.mark_done(1, vec![], String::new());
        plan.mark_done(2, vec![], String::new());

        // Now group 3 is ready
        assert_eq!(plan.next_ready_batch(), vec![3]);
    }
}
