use crate::config::{AnvilInstanceConfig, ExternalConfig, ProvidersConfig};
use crate::get_providers_path;
use crate::instance::ManagedInstance;
use crate::manager::ProviderManager;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, io};
use uuid::Uuid;

impl ProviderManager {
    /// Create a ProviderManager by resolving and loading the providers config.
    ///
    /// Path resolution priority:
    /// 1. Path configured via `set_providers_path()`
    /// 2. `PROVIDERS_TOML` environment variable
    /// 3. Directory walk from current directory
    pub async fn from_default_config() -> Result<Self> {
        let path = resolve_providers_path()?;
        let config = ProvidersConfig::from_file(&path)
            .with_context(|| format!("Failed to load config from {}", path.display()))?;
        config.validate()?;
        Self::from_config(config).await
    }

    /// Create a ProviderManager from a ProvidersConfig (used by tests)
    pub async fn from_config(config: ProvidersConfig) -> Result<Self> {
        let manager = Self::new();

        for (name, instance_config) in config.anvil_instances {
            manager
                .spawn_anvil(name.clone(), instance_config)
                .await
                .with_context(|| format!("Failed to spawn Anvil instance '{}'", name))?;
        }

        for (name, external_config) in config.external {
            manager
                .register_external(name.clone(), external_config)
                .await
                .with_context(|| format!("Failed to register external endpoint '{}'", name))?;
        }

        Ok(manager)
    }

    /// Spawn a new Anvil instance
    pub async fn spawn_anvil(&self, name: String, config: AnvilInstanceConfig) -> Result<Uuid> {
        self.ensure_name_available(&name)?;
        let instance = ManagedInstance::spawn_anvil(name, config).await?;
        self.insert_instance(instance)
    }

    /// Register an external RPC endpoint
    pub async fn register_external(&self, name: String, config: ExternalConfig) -> Result<Uuid> {
        self.ensure_name_available(&name)?;
        let instance = ManagedInstance::from_external(name, config).await?;
        self.insert_instance(instance)
    }

    /// Shutdown a specific instance by ID
    pub async fn shutdown_instance(&self, id: Uuid) -> Result<()> {
        let instance = {
            let mut name_to_id = self.name_to_id.write().unwrap();
            name_to_id.retain(|_, v| *v != id);
            let mut instances = self.instances.write().unwrap();
            instances.remove(&id)
        };

        if let Some(instance) = instance {
            instance.shutdown().await?;
            tracing::info!(id = %id, "Shutdown instance");
        }

        Ok(())
    }

    /// Shutdown all instances
    pub async fn shutdown_all(&self) -> Result<()> {
        let instances: Vec<Arc<ManagedInstance>> = {
            let mut name_to_id = self.name_to_id.write().unwrap();
            name_to_id.clear();
            let mut guard = self.instances.write().unwrap();
            guard.drain().map(|(_, v)| v).collect()
        };

        for instance in instances {
            let _ = instance.shutdown().await;
        }

        tracing::info!("Shutdown all instances");
        Ok(())
    }

    fn ensure_name_available(&self, name: &str) -> Result<()> {
        let name_to_id = self.name_to_id.read().unwrap();
        if name_to_id.contains_key(name) {
            anyhow::bail!("Instance name '{}' already exists", name);
        }
        Ok(())
    }

    fn insert_instance(&self, instance: ManagedInstance) -> Result<Uuid> {
        let id = instance.id();
        let name = instance.name().to_string();
        let instance = Arc::new(instance);

        let mut name_to_id = self.name_to_id.write().unwrap();
        if name_to_id.contains_key(&name) {
            anyhow::bail!("Instance name '{}' already exists", name);
        }

        let mut instances = self.instances.write().unwrap();
        instances.insert(id, Arc::clone(&instance));
        name_to_id.insert(name, id);

        Ok(id)
    }
}

/// Resolve the providers.toml path with the following priority:
/// 1. Path configured via `set_providers_path()`
/// 2. `PROVIDERS_TOML` environment variable
/// 3. Directory walk from current directory
pub(crate) fn resolve_providers_path() -> Result<PathBuf> {
    // 1. Check for programmatically configured path (e.g., from CLI --providers flag)
    if let Some(path) = get_providers_path() {
        if path.exists() {
            tracing::info!(path = %path.display(), "Using configured providers path");
            return Ok(path.clone());
        }
        anyhow::bail!("Configured providers path not found: {}", path.display());
    }

    // 2. Check PROVIDERS_TOML environment variable
    if let Ok(path) = env::var("PROVIDERS_TOML") {
        let path = PathBuf::from(path);
        if path.exists() {
            tracing::info!(path = %path.display(), "Using PROVIDERS_TOML env var");
            return Ok(path);
        }
        anyhow::bail!("PROVIDERS_TOML was set but not found: {}", path.display());
    }

    // 3. Walk up from current directory looking for providers.toml
    let mut dir = env::current_dir().map_err(|e| {
        anyhow::anyhow!(io::Error::new(
            e.kind(),
            format!("Failed to read current dir: {}", e),
        ))
    })?;

    loop {
        let candidate = dir.join("providers.toml");
        if candidate.exists() {
            tracing::info!(path = %candidate.display(), "Found providers.toml via directory walk");
            return Ok(candidate);
        }

        if !dir.pop() {
            break;
        }
    }

    anyhow::bail!("providers.toml not found from current directory");
}
