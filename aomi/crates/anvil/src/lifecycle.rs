use crate::config::{AnvilInstanceConfig, ExternalConfig, ProvidersConfig};
use crate::instance::ManagedInstance;
use crate::manager::ProviderManager;
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

impl ProviderManager {
    /// Create a ProviderManager from a ProvidersConfig
    ///
    /// This spawns all configured Anvil instances and registers external endpoints.
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

    /// Create a ProviderManager from a config file
    pub async fn from_config_file(path: impl AsRef<Path>) -> Result<Self> {
        let config = ProvidersConfig::from_file(path)?;
        config.validate()?;
        Self::from_config(config).await
    }

    /// Create a ProviderManager from the default providers.toml path
    pub async fn from_default_config() -> Result<Self> {
        Self::from_config_file("providers.toml").await
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
