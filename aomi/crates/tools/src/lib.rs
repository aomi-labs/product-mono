pub mod abi_encoder;
pub mod db;
pub mod db_tools;
pub mod etherscan;
pub mod scheduler;
pub mod time;
pub mod types;
pub mod wallet;
pub mod docs;

// Re-export the tool types and their parameter types for convenience
pub use abi_encoder::{EncodeFunctionCall, EncodeFunctionCallParameters};
pub use db::{Contract, ContractStore, ContractStoreApi};
pub use db_tools::{get_contract_info, store_contract_info};
pub use etherscan::*;
pub use time::{GetCurrentTime, GetCurrentTimeParameters};
pub use wallet::{SendTransactionToWallet, SendTransactionToWalletParameters};

// Re-export scheduler types
pub use scheduler::{ToolScheduler, ToolResultStream, ToolResultFuture, ToolResultFutureInner};

// Re-export types
pub use types::AomiApiTool;

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