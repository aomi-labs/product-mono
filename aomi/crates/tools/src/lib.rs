pub mod clients;
pub mod db;
pub mod execution;
pub mod scheduler;
pub mod streams;
pub mod types;
pub mod wrapper;

pub use execution::{
    abi_encoder, account, brave_search, cast, db_tools, docs, etherscan, time, wallet,
};

// Re-export the tool types and their parameter types for convenience
pub use abi_encoder::{EncodeFunctionCall, EncodeFunctionCallParameters};
pub use account::{GetAccountInfo, GetAccountTransactionHistory};
pub use db_tools::{GetContractABI, GetContractSourceCode};
pub use etherscan::*;
pub use time::{GetCurrentTime, GetCurrentTimeParameters};
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
