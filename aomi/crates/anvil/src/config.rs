use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnvilParams {
    pub port: u16,
    pub chain_id: u64,
    pub fork_url: Option<String>,
    pub fork_block_number: Option<u64>,
    pub block_time: Option<u64>,
    pub accounts: u32,
    pub mnemonic: Option<String>,
    pub anvil_bin: Option<String>,
    pub load_state: Option<String>,
    pub dump_state: Option<String>,
    pub silent: bool,
    pub steps_tracing: bool,
}

impl Default for AnvilParams {
    fn default() -> Self {
        Self {
            port: 0,
            chain_id: 31337,
            fork_url: None,
            fork_block_number: None,
            block_time: None,
            accounts: 10,
            mnemonic: None,
            anvil_bin: None,
            load_state: None,
            dump_state: None,
            silent: false,
            steps_tracing: false,
        }
    }
}

impl AnvilParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }

    pub fn with_fork_url(mut self, url: impl Into<String>) -> Self {
        self.fork_url = Some(url.into());
        self
    }

    pub fn with_fork_block_number(mut self, block: u64) -> Self {
        self.fork_block_number = Some(block);
        self
    }

    pub fn with_block_time(mut self, seconds: u64) -> Self {
        self.block_time = Some(seconds);
        self
    }

    pub fn with_accounts(mut self, count: u32) -> Self {
        self.accounts = count;
        self
    }

    pub fn with_mnemonic(mut self, mnemonic: impl Into<String>) -> Self {
        self.mnemonic = Some(mnemonic.into());
        self
    }

    pub fn with_anvil_bin(mut self, path: impl Into<String>) -> Self {
        self.anvil_bin = Some(path.into());
        self
    }

    pub fn with_load_state(mut self, path: impl Into<String>) -> Self {
        self.load_state = Some(path.into());
        self
    }

    /// Alias for loading a pre-saved snapshot/state file.
    pub fn with_snapshot(mut self, path: impl Into<String>) -> Self {
        self.load_state = Some(path.into());
        self
    }

    /// Configure anvil to dump state to the provided path on shutdown.
    pub fn with_dump_state(mut self, path: impl Into<String>) -> Self {
        self.dump_state = Some(path.into());
        self
    }

    pub fn with_silent(mut self, silent: bool) -> Self {
        self.silent = silent;
        self
    }

    pub fn with_steps_tracing(mut self, enabled: bool) -> Self {
        self.steps_tracing = enabled;
        self
    }
}

#[derive(Clone, Debug)]
pub struct SpawnConfig {
    pub anvil_params: Vec<AnvilParams>,
    pub auto_spawn: bool,
    pub env_var: String,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            anvil_params: vec![AnvilParams::default()],
            auto_spawn: true,
            env_var: "ETH_RPC_URL".to_string(),
        }
    }
}

impl SpawnConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn single(config: AnvilParams) -> Self {
        Self {
            anvil_params: vec![config],
            auto_spawn: true,
            env_var: "ETH_RPC_URL".to_string(),
        }
    }

    pub fn multiple(configs: Vec<AnvilParams>) -> Self {
        Self {
            anvil_params: configs,
            auto_spawn: true,
            env_var: "ETH_RPC_URL".to_string(),
        }
    }

    pub fn external_only() -> Self {
        Self {
            anvil_params: vec![],
            auto_spawn: false,
            env_var: "ETH_RPC_URL".to_string(),
        }
    }

    pub fn with_fork(mut self, config: AnvilParams) -> Self {
        self.anvil_params.push(config);
        self
    }

    pub fn with_forks(mut self, configs: Vec<AnvilParams>) -> Self {
        self.anvil_params = configs;
        self
    }

    pub fn with_auto_spawn(mut self, auto: bool) -> Self {
        self.auto_spawn = auto;
        self
    }

    pub fn with_env_var(mut self, name: impl Into<String>) -> Self {
        self.env_var = name.into();
        self
    }

    /// Apply a snapshot (load_state) to all configured forks.
    pub fn with_snapshot(mut self, path: impl Into<String>) -> Self {
        let snapshot = path.into();
        for fork in self.anvil_params.iter_mut() {
            fork.load_state = Some(snapshot.clone());
        }
        self
    }

    pub fn num_forks(&self) -> usize {
        self.anvil_params.len()
    }
}
