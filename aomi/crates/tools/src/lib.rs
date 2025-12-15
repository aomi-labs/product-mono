pub mod execution;
pub mod types;
pub mod db;
pub mod register;
pub mod clients;

pub use execution::{
    abi_encoder, account, brave_search, cast, db_tools, docs, etherscan,
    time, wallet,
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

// Re-export stream/future types
pub use streams::{
    ToolCompletion, ToolReciever, ToolResultSender, ToolResultStream,
};

// Re-export types
pub use types::{AnyApiTool, AomiApiTool, MultiStepApiTool};

#[cfg(test)]
mod tests;

pub mod scheduler;
pub mod streams;

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
