use foundry_evm::opts::EvmOpts;
use serde::{Deserialize, Serialize};

use crate::contract::session::ContractConfig;

pub type ChainId = u64;

/// Per-node compilation configuration (NOT shared, owned by each GroupNode)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupConfig {
    /// Foundry project configuration (NOT Arc - owned by each node)
    pub foundry_config: foundry_config::Config,

    /// EVM runtime options (copied from backend per target chain)
    pub evm_opts: EvmOpts,

    /// Compiler: disable solc auto-detection
    pub no_auto_detect: bool,

    /// Session identifier
    pub id: Option<String>,
}

impl GroupConfig {
    /// Create from ContractConfig
    pub fn from_contract_config(config: &ContractConfig) -> Self {
        Self {
            foundry_config: (*config.foundry_config).clone(), // unwrap Arc and clone
            evm_opts: config.evm_opts.clone(),
            no_auto_detect: config.no_auto_detect,
            id: config.id.clone(),
        }
    }

    /// Create with target chain's EvmOpts from backend
    pub fn for_chain(
        foundry_config: foundry_config::Config,
        evm_opts: EvmOpts,
        no_auto_detect: bool,
        id: Option<String>,
    ) -> Self {
        Self {
            foundry_config,
            evm_opts,
            no_auto_detect,
            id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_contract_config_converts_correctly() {
        let contract_config = ContractConfig::default();
        let group_config = GroupConfig::from_contract_config(&contract_config);

        assert_eq!(
            group_config.foundry_config.evm_version,
            contract_config.foundry_config.evm_version
        );
        assert_eq!(group_config.no_auto_detect, contract_config.no_auto_detect);
        assert_eq!(group_config.id, contract_config.id);
    }

    #[test]
    fn for_chain_creates_config() {
        let foundry_config = foundry_config::Config::default();
        let evm_opts = EvmOpts::default();
        let group_config =
            GroupConfig::for_chain(foundry_config.clone(), evm_opts.clone(), false, None);

        assert_eq!(group_config.foundry_config.evm_version, foundry_config.evm_version);
        assert!(!group_config.no_auto_detect);
        assert!(group_config.id.is_none());
    }
}
