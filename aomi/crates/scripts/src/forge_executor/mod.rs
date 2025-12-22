pub mod assembler;
pub mod executor;
pub mod manager;
pub mod plan;
pub mod resources;
pub mod source_fetcher;
pub mod tools;
pub mod types;

#[cfg(test)]
pub mod tests;

// Re-export main types for convenience
pub use executor::ForgeExecutor;
pub use manager::ForgeManager;
pub use plan::{ExecutionPlan, GroupStatus, OperationGroup};
pub use resources::SharedForgeResources;
pub use source_fetcher::SourceFetcher;
pub use tools::{NextGroups, SetExecutionPlan};
pub use types::{GroupResult, GroupResultInner, TransactionData};
