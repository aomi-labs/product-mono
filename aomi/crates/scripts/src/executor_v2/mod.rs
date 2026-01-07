pub mod config;
pub mod artifacts;
pub mod backend;
pub mod node;
pub mod orchestrator;

use cast::backend::Backend;
pub use config::GroupConfig;
pub use artifacts::GroupArtifacts;
pub use backend::ExecutionBackend;
pub use node::GroupNode;
pub use script_app_context::*;
pub use orchestrator::{ForgeOrchestrator, GroupReceipt};



// temp
use foundry_evm::opts::EvmOpts;
pub struct ProviderManager;
impl ProviderManager {
    pub fn new() -> Self {
        Self {}
    }
    pub fn get_evm_opts(&self, chain_id: config::ChainId) -> EvmOpts {
        unimplemented!()
    }

    pub fn get_backend(&self, chain_ids: &[config::ChainId]) -> Backend {
        unimplemented!()
    }
}
