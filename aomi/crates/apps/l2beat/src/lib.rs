mod adapter;
pub mod app;
mod discovered;
mod handlers;
pub mod l2b_tools;
mod runner;

pub use adapter::etherscan_to_contract_info;
pub use app::{L2BeatApp, L2BeatCommand};
pub use handlers::{
    array::ArrayHandler,
    call::CallHandler,
    config::HandlerDefinition,
    event::EventHandler,
    storage::StorageHandler,
    types::{Handler, HandlerResult},
};
pub use runner::DiscoveryRunner;
