// Allow manual_async_fn for trait methods using `impl Future` pattern
// This is required because the AomiTool trait uses this pattern for run_sync/run_async
#![allow(clippy::manual_async_fn)]

pub mod clients;
pub mod db;
pub mod ethereum;
pub mod queries;
pub mod scheduler;
pub mod streams;
pub mod types;
pub mod wrapper;

pub use ethereum::{abi_encoder, account, cast, etherscan, wallet};
pub use queries::{brave_search, context, db_tools, docs};

// Re-export the tool types and their parameter types for convenience
pub use abi_encoder::{EncodeFunctionCall, EncodeFunctionCallParameters};
pub use account::{GetAccountInfo, GetAccountTransactionHistory};
pub use context::{GetTimeAndOnchainCtx, GetTimeAndOnchainCtxParameters};
pub use db_tools::{GetContractABI, GetContractSourceCode};
pub use etherscan::*;
pub use wallet::{SendTransactionToWallet, SendTransactionToWalletParameters};

// Re-export scheduler types
pub use scheduler::ToolScheduler;
pub use wrapper::AomiToolWrapper;

// Re-export stream/future types
pub use streams::{ToolCompletion, ToolReciever, ToolReturn};
pub use types::{
    AomiToolArgs, CallMetadata, RuntimeEnvelope, ToolCallCtx, ToolMetadata, WithTopic, with_topic,
};

// Re-export types
pub use types::AomiTool;

#[cfg(test)]
mod tests;

// Expose test utilities to dependent crates for integration tests when requested.
// This reuses the same helpers that the aomi-tools crate uses internally.
#[cfg(any(test, feature = "test-utils"))]
#[path = "tests/utils.rs"]
pub mod test_utils;
