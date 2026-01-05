use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Configuration for spawning an Anvil instance (runtime parameters)
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

// ============================================================================
// New Configuration Types for ProviderManager
// ============================================================================

/// Root configuration loaded from providers.toml
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ProvidersConfig {
    /// Managed Anvil instances - will be auto-spawned
    #[serde(default, rename = "anvil-instances")]
    pub anvil_instances: HashMap<String, AnvilInstanceConfig>,
    /// External RPC endpoints - no anvil process, just connect
    #[serde(default)]
    pub external: HashMap<String, ExternalConfig>,
}

/// Configuration for a managed Anvil instance
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnvilInstanceConfig {
    /// Chain ID for this fork
    pub chain_id: u64,
    /// Fork URL (supports {ENV_VAR} substitution)
    pub fork_url: String,
    /// Optional fork block number (latest if not specified)
    #[serde(default)]
    pub fork_block_number: Option<u64>,
    /// Port to run anvil on (0 = auto-assign)
    #[serde(default)]
    pub port: u16,
    /// Block time in seconds (None = instant mining)
    #[serde(default)]
    pub block_time: Option<u64>,
    /// Number of accounts to generate
    #[serde(default = "default_accounts")]
    pub accounts: u32,
    /// Custom mnemonic for account generation
    #[serde(default)]
    pub mnemonic: Option<String>,
    /// Path to anvil binary (uses "anvil" from PATH if not specified)
    #[serde(default)]
    pub anvil_bin: Option<String>,
    /// Path to load state from
    #[serde(default)]
    pub load_state: Option<String>,
    /// Enable steps tracing
    #[serde(default)]
    pub steps_tracing: bool,
}

fn default_accounts() -> u32 {
    10
}

/// Configuration for an external RPC endpoint (no anvil process)
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExternalConfig {
    /// Chain ID for this endpoint
    pub chain_id: u64,
    /// RPC URL (supports {ENV_VAR} substitution)
    pub rpc_url: String,
}

impl ProvidersConfig {
    /// Load configuration from a TOML file
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {:?}", path.as_ref()))?;
        Self::from_str(&content)
    }

    /// Parse configuration from a TOML string
    pub fn from_str(content: &str) -> Result<Self> {
        let mut config: ProvidersConfig =
            toml::from_str(content).context("Failed to parse TOML config")?;
        config.substitute_env_vars()?;
        Ok(config)
    }

    /// Substitute {ENV_VAR} placeholders with actual environment variable values
    fn substitute_env_vars(&mut self) -> Result<()> {
        for (name, instance) in &mut self.anvil_instances {
            instance.fork_url = substitute_env_vars(&instance.fork_url).with_context(|| {
                format!(
                    "Failed to substitute env vars in fork_url for instance '{}'",
                    name
                )
            })?;
        }

        for (name, external) in &mut self.external {
            external.rpc_url = substitute_env_vars(&external.rpc_url).with_context(|| {
                format!(
                    "Failed to substitute env vars in rpc_url for external '{}'",
                    name
                )
            })?;
        }

        Ok(())
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Check for duplicate chain_ids across all instances
        let mut seen_chain_ids: HashMap<u64, String> = HashMap::new();

        for (name, instance) in &self.anvil_instances {
            if let Some(existing) = seen_chain_ids.get(&instance.chain_id) {
                anyhow::bail!(
                    "Duplicate chain_id {} found in anvil instances '{}' and '{}'",
                    instance.chain_id,
                    existing,
                    name
                );
            }
            seen_chain_ids.insert(instance.chain_id, name.clone());
        }

        for (name, external) in &self.external {
            if let Some(existing) = seen_chain_ids.get(&external.chain_id) {
                anyhow::bail!(
                    "Duplicate chain_id {} found in '{}' and external '{}'",
                    external.chain_id,
                    existing,
                    name
                );
            }
            seen_chain_ids.insert(external.chain_id, name.clone());
        }

        // Check for duplicate names across sections
        for name in self.external.keys() {
            if self.anvil_instances.contains_key(name) {
                anyhow::bail!(
                    "Duplicate instance name '{}' found in both anvil-instances and external",
                    name
                );
            }
        }

        Ok(())
    }
}

impl AnvilInstanceConfig {
    /// Convert to AnvilParams for spawning
    pub fn to_anvil_params(&self) -> AnvilParams {
        AnvilParams {
            port: self.port,
            chain_id: self.chain_id,
            fork_url: Some(self.fork_url.clone()),
            fork_block_number: self.fork_block_number,
            block_time: self.block_time,
            accounts: self.accounts,
            mnemonic: self.mnemonic.clone(),
            anvil_bin: self.anvil_bin.clone(),
            load_state: self.load_state.clone(),
            dump_state: None,
            silent: false,
            steps_tracing: self.steps_tracing,
        }
    }
}

/// Substitute {ENV_VAR} placeholders in a string with environment variable values
fn substitute_env_vars(input: &str) -> Result<String> {
    let re = Regex::new(r"\{([A-Z_][A-Z0-9_]*)\}").expect("Invalid regex");
    let mut result = input.to_string();
    let mut errors = Vec::new();

    for cap in re.captures_iter(input) {
        let var_name = &cap[1];
        let placeholder = &cap[0];

        match std::env::var(var_name) {
            Ok(value) => {
                result = result.replace(placeholder, &value);
            }
            Err(_) => {
                errors.push(var_name.to_string());
            }
        }
    }

    if !errors.is_empty() {
        anyhow::bail!(
            "Missing environment variable(s): {}",
            errors.join(", ")
        );
    }

    Ok(result)
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_providers_config_parsing() {
        let toml_content = r#"
[anvil-instances]
ethereum = { chain_id = 1, fork_url = "https://eth-mainnet.example.com" }
optimism = { chain_id = 10, fork_url = "https://optimism.example.com", fork_block_number = 12345 }

[external]
base = { chain_id = 8453, rpc_url = "https://mainnet.base.org" }
"#;

        let config = ProvidersConfig::from_str(toml_content).expect("Should parse config");

        assert_eq!(config.anvil_instances.len(), 2);
        assert_eq!(config.external.len(), 1);

        let eth = config.anvil_instances.get("ethereum").unwrap();
        assert_eq!(eth.chain_id, 1);
        assert_eq!(eth.fork_url, "https://eth-mainnet.example.com");
        assert_eq!(eth.fork_block_number, None);

        let opt = config.anvil_instances.get("optimism").unwrap();
        assert_eq!(opt.chain_id, 10);
        assert_eq!(opt.fork_block_number, Some(12345));

        let base = config.external.get("base").unwrap();
        assert_eq!(base.chain_id, 8453);
        assert_eq!(base.rpc_url, "https://mainnet.base.org");
    }

    #[test]
    fn test_env_var_substitution() {
        std::env::set_var("TEST_API_KEY", "secret123");

        let toml_content = r#"
[anvil-instances]
ethereum = { chain_id = 1, fork_url = "https://eth.example.com/v2/{TEST_API_KEY}" }
"#;

        let config = ProvidersConfig::from_str(toml_content).expect("Should parse config");
        let eth = config.anvil_instances.get("ethereum").unwrap();
        assert_eq!(eth.fork_url, "https://eth.example.com/v2/secret123");

        std::env::remove_var("TEST_API_KEY");
    }

    #[test]
    fn test_missing_env_var() {
        std::env::remove_var("NONEXISTENT_VAR");

        let toml_content = r#"
[anvil-instances]
ethereum = { chain_id = 1, fork_url = "https://eth.example.com/{NONEXISTENT_VAR}" }
"#;

        let result = ProvidersConfig::from_str(toml_content);
        assert!(result.is_err(), "Expected error for missing env var");
        // The error wraps the inner error with context, so check for the context message
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("NONEXISTENT_VAR") || err_msg.contains("ethereum"),
            "Expected error about missing env var or instance name but got: {}",
            err_msg
        );
    }

    #[test]
    fn test_duplicate_chain_id_validation() {
        let toml_content = r#"
[anvil-instances]
ethereum = { chain_id = 1, fork_url = "https://eth.example.com" }
ethereum2 = { chain_id = 1, fork_url = "https://eth2.example.com" }
"#;

        let config = ProvidersConfig::from_str(toml_content).expect("Should parse config");
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate chain_id"));
    }

    #[test]
    fn test_anvil_instance_config_to_params() {
        let config = AnvilInstanceConfig {
            chain_id: 1,
            fork_url: "https://eth.example.com".to_string(),
            fork_block_number: Some(12345),
            port: 8545,
            block_time: Some(12),
            accounts: 5,
            mnemonic: Some("test mnemonic".to_string()),
            anvil_bin: None,
            load_state: None,
            steps_tracing: true,
        };

        let params = config.to_anvil_params();

        assert_eq!(params.chain_id, 1);
        assert_eq!(params.fork_url, Some("https://eth.example.com".to_string()));
        assert_eq!(params.fork_block_number, Some(12345));
        assert_eq!(params.port, 8545);
        assert_eq!(params.block_time, Some(12));
        assert_eq!(params.accounts, 5);
        assert!(params.steps_tracing);
    }

    #[test]
    fn test_default_accounts() {
        let toml_content = r#"
[anvil-instances]
ethereum = { chain_id = 1, fork_url = "https://eth.example.com" }
"#;

        let config = ProvidersConfig::from_str(toml_content).expect("Should parse config");
        let eth = config.anvil_instances.get("ethereum").unwrap();
        assert_eq!(eth.accounts, 10); // default value
    }
}
