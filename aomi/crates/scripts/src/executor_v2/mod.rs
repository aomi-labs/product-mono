pub mod config;
pub mod artifacts;
pub mod backend;

pub use config::GroupConfig;
pub use artifacts::GroupArtifacts;
pub use backend::ExecutionBackend;



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
}