pub mod abi_encoder;
pub mod account;
pub mod brave_search;
pub mod cast;
pub mod clients;
pub mod contract;
pub mod db;
pub mod db_tools;
pub mod docs;
pub mod etherscan;
pub mod forge_executor;
pub mod scheduler;
pub mod time;
pub mod tool_stream;
pub mod tools;
pub mod types;
pub mod wallet;

// Re-export the tool types and their parameter types for convenience
pub use abi_encoder::{EncodeFunctionCall, EncodeFunctionCallParameters};
pub use account::{GetAccountInfo, GetAccountTransactionHistory};
pub use db_tools::{GetContractABI, GetContractSourceCode};
pub use etherscan::*;
pub use forge_executor::assembler::{AssemblyConfig, FundingRequirement, ScriptAssembler};
pub use time::{GetCurrentTime, GetCurrentTimeParameters};
pub use wallet::{SendTransactionToWallet, SendTransactionToWalletParameters};

// Re-export scheduler types
pub use scheduler::ToolScheduler;

// Re-export stream/future types
pub use tool_stream::{
    ToolCompletion, ToolReciever, ToolResultSender, ToolResultStream,
};

// Re-export types
pub use types::{AnyApiTool, AomiApiTool};

#[cfg(test)]
mod tests;

#[macro_export]
macro_rules! impl_rig_tool_clone {
    ($tool:ident, $params:ident, []) => {
        impl Clone for $tool {
            fn clone(&self) -> Self {
                Self
            }
        }

        impl Clone for $params {
            fn clone(&self) -> Self {
                Self {}
            }
        }
    };
    ($tool:ident, $params:ident, [$($field:ident),+ $(,)?]) => {
        impl Clone for $tool {
            fn clone(&self) -> Self {
                Self
            }
        }

        impl Clone for $params {
            fn clone(&self) -> Self {
                Self {
                    $( $field: self.$field.clone(), )*
                }
            }
        }
    };
}
