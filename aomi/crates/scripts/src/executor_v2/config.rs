use foundry_evm::opts::EvmOpts;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GroupConfig {
    pub foundry_config: Arc<foundry_config::Config>,
    pub evm_opts: EvmOpts,
    pub no_auto_detect: bool,
    pub id: Option<String>,
}

impl GroupConfig {
    pub fn new(foundry_config: foundry_config::Config, id: Option<String>) -> Self {
        Self {
            foundry_config: Arc::new(foundry_config),
            evm_opts: EvmOpts::default(),
            no_auto_detect: false,
            id,
        }
    }
}
