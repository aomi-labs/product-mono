use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Root configuration loaded from providers.toml
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ProvidersConfig {
    /// Managed Anvil instances - will be auto-spawned
    #[serde(default, rename = "anvil-instances")]
    pub anvil_instances: HashMap<String, AnvilInstanceConfig>,
    /// External RPC endpoints - no anvil process, just connect
    #[serde(default)]
    pub external: HashMap<String, ExternalConfig>,
    /// Private keys for wallets that should auto-sign transactions in eval-test mode.
    /// The public addresses are derived from these keys at runtime.
    /// Format: hex string with or without 0x prefix (e.g., "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
    #[serde(default)]
    pub autosign_keys: Vec<String>,
}

/// Configuration for a managed Anvil instance
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnvilInstanceConfig {
    /// Chain ID for this fork
    pub chain_id: u64,
    /// Fork URL (supports {ENV_VAR} substitution)
    #[serde(default)]
    pub fork_url: Option<String>,
    /// Fallback fork URLs to try if primary fails (supports {ENV_VAR} substitution)
    #[serde(default)]
    pub fallback_urls: Option<Vec<String>>,
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
    /// Path to dump state to
    #[serde(default)]
    pub dump_state: Option<String>,
    /// Enable steps tracing
    #[serde(default)]
    pub steps_tracing: bool,
}

/// Configuration for an external RPC endpoint (no anvil process)
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExternalConfig {
    /// Chain ID for this endpoint
    pub chain_id: u64,
    /// RPC URL (supports {ENV_VAR} substitution)
    pub rpc_url: String,
    /// Whether this is a local endpoint (e.g., locally-run Anvil for testing)
    /// Local endpoints are used by LocalGateway for eval-test mode.
    #[serde(default)]
    pub local: bool,
}

fn default_accounts() -> u32 {
    10
}

impl AnvilInstanceConfig {
    pub fn new(chain_id: u64, fork_url: impl Into<String>) -> Self {
        Self {
            chain_id,
            fork_url: Some(fork_url.into()),
            fallback_urls: None,
            fork_block_number: None,
            port: 0,
            block_time: None,
            accounts: default_accounts(),
            mnemonic: None,
            anvil_bin: None,
            load_state: None,
            dump_state: None,
            steps_tracing: false,
        }
    }

    pub fn local(chain_id: u64) -> Self {
        Self {
            chain_id,
            fork_url: None,
            fallback_urls: None,
            fork_block_number: None,
            port: 0,
            block_time: None,
            accounts: default_accounts(),
            mnemonic: None,
            anvil_bin: None,
            load_state: None,
            dump_state: None,
            steps_tracing: false,
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
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

    pub fn with_dump_state(mut self, path: impl Into<String>) -> Self {
        self.dump_state = Some(path.into());
        self
    }

    pub fn with_steps_tracing(mut self, enabled: bool) -> Self {
        self.steps_tracing = enabled;
        self
    }
}

impl ProvidersConfig {
    /// Load configuration from a TOML file
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {:?}", path.as_ref()))?;
        Self::from_toml_str(&content)
    }

    /// Parse configuration from a TOML string
    pub fn from_toml_str(content: &str) -> Result<Self> {
        let mut config: ProvidersConfig =
            toml::from_str(content).context("Failed to parse TOML config")?;
        config.substitute_env_vars()?;
        Ok(config)
    }

    /// Substitute {ENV_VAR} placeholders with actual environment variable values
    fn substitute_env_vars(&mut self) -> Result<()> {
        for (name, instance) in &mut self.anvil_instances {
            if let Some(fork_url) = instance.fork_url.as_mut() {
                *fork_url = substitute_env_vars(fork_url).with_context(|| {
                    format!(
                        "Failed to substitute env vars in fork_url for instance '{}'",
                        name
                    )
                })?;
            }
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
        // Check for duplicate chain_ids within anvil instances
        let mut seen_anvil_chain_ids: HashMap<u64, String> = HashMap::new();
        for (name, instance) in &self.anvil_instances {
            if let Some(existing) = seen_anvil_chain_ids.get(&instance.chain_id) {
                anyhow::bail!(
                    "Duplicate chain_id {} found in anvil instances '{}' and '{}'",
                    instance.chain_id,
                    existing,
                    name
                );
            }
            seen_anvil_chain_ids.insert(instance.chain_id, name.clone());
        }

        // Check for duplicate chain_ids within external endpoints
        let mut seen_external_chain_ids: HashMap<u64, String> = HashMap::new();
        for (name, external) in &self.external {
            if let Some(existing) = seen_external_chain_ids.get(&external.chain_id) {
                anyhow::bail!(
                    "Duplicate chain_id {} found in external endpoints '{}' and '{}'",
                    external.chain_id,
                    existing,
                    name
                );
            }
            seen_external_chain_ids.insert(external.chain_id, name.clone());
        }

        // Note: We intentionally allow an anvil-instance to share a chain_id with an
        // external endpoint if the anvil-instance forks from that external endpoint.
        // This is required because forks should replicate the exact chain_id of the
        // source chain for accurate simulation.

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

impl std::str::FromStr for ProvidersConfig {
    type Err = anyhow::Error;

    fn from_str(content: &str) -> Result<Self, Self::Err> {
        ProvidersConfig::from_toml_str(content)
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
        anyhow::bail!("Missing environment variable(s): {}", errors.join(", "));
    }

    Ok(result)
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

        let config = ProvidersConfig::from_toml_str(toml_content).expect("Should parse config");

        assert_eq!(config.anvil_instances.len(), 2);
        assert_eq!(config.external.len(), 1);

        let eth = config.anvil_instances.get("ethereum").unwrap();
        assert_eq!(eth.chain_id, 1);
        assert_eq!(
            eth.fork_url.as_deref(),
            Some("https://eth-mainnet.example.com")
        );
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

        let config = ProvidersConfig::from_toml_str(toml_content).expect("Should parse config");
        let eth = config.anvil_instances.get("ethereum").unwrap();
        assert_eq!(
            eth.fork_url.as_deref(),
            Some("https://eth.example.com/v2/secret123")
        );

        std::env::remove_var("TEST_API_KEY");
    }

    #[test]
    fn test_missing_env_var() {
        std::env::remove_var("NONEXISTENT_VAR");

        let toml_content = r#"
[anvil-instances]
ethereum = { chain_id = 1, fork_url = "https://eth.example.com/{NONEXISTENT_VAR}" }
"#;

        let result = ProvidersConfig::from_toml_str(toml_content);
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

        let config = ProvidersConfig::from_toml_str(toml_content).expect("Should parse config");
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Duplicate chain_id"));
    }

    #[test]
    fn test_anvil_instance_config_builder() {
        let config = AnvilInstanceConfig::new(1, "https://eth.example.com")
            .with_port(8545)
            .with_fork_block_number(12345)
            .with_block_time(12)
            .with_accounts(5)
            .with_mnemonic("test mnemonic")
            .with_anvil_bin("/usr/local/bin/anvil")
            .with_load_state("state.json")
            .with_dump_state("dump.json")
            .with_steps_tracing(true);

        assert_eq!(config.chain_id, 1);
        assert_eq!(config.fork_url.as_deref(), Some("https://eth.example.com"));
        assert_eq!(config.fork_block_number, Some(12345));
        assert_eq!(config.port, 8545);
        assert_eq!(config.block_time, Some(12));
        assert_eq!(config.accounts, 5);
        assert_eq!(config.mnemonic.as_deref(), Some("test mnemonic"));
        assert_eq!(config.anvil_bin.as_deref(), Some("/usr/local/bin/anvil"));
        assert_eq!(config.load_state.as_deref(), Some("state.json"));
        assert_eq!(config.dump_state.as_deref(), Some("dump.json"));
        assert!(config.steps_tracing);
    }

    #[test]
    fn test_default_accounts() {
        let toml_content = r#"
[anvil-instances]
ethereum = { chain_id = 1, fork_url = "https://eth.example.com" }
"#;

        let config = ProvidersConfig::from_toml_str(toml_content).expect("Should parse config");
        let eth = config.anvil_instances.get("ethereum").unwrap();
        assert_eq!(eth.accounts, 10); // default value
    }
}
