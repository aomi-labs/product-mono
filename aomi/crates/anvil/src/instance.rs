use crate::config::{AnvilInstanceConfig, ExternalConfig};
use alloy::network::AnyNetwork;
use alloy_provider::RootProvider;
use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use serde_json::json;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

/// Source of a managed instance - either a spawned Anvil process or an external RPC
#[derive(Debug)]
pub enum InstanceSource {
    /// Managed Anvil process
    Anvil { child: Mutex<Child>, port: u16 },
    /// External RPC endpoint
    External,
}

/// Metrics for tracking instance usage
pub struct InstanceMetrics {
    provider_requests: AtomicU64,
    backend_requests: AtomicU64,
    last_provider_access: RwLock<Option<Instant>>,
    last_backend_access: RwLock<Option<Instant>>,
}

impl Default for InstanceMetrics {
    fn default() -> Self {
        Self {
            provider_requests: AtomicU64::new(0),
            backend_requests: AtomicU64::new(0),
            last_provider_access: RwLock::new(None),
            last_backend_access: RwLock::new(None),
        }
    }
}

impl InstanceMetrics {
    fn record_provider_access(&self) {
        self.provider_requests.fetch_add(1, Ordering::Relaxed);
        *self.last_provider_access.write().unwrap() = Some(Instant::now());
    }

    fn record_backend_access(&self) {
        self.backend_requests.fetch_add(1, Ordering::Relaxed);
        *self.last_backend_access.write().unwrap() = Some(Instant::now());
    }

    /// Create a snapshot of the current metrics
    pub fn snapshot(&self) -> InstanceMetricsSnapshot {
        InstanceMetricsSnapshot {
            provider_requests: self.provider_requests.load(Ordering::Relaxed),
            backend_requests: self.backend_requests.load(Ordering::Relaxed),
            last_provider_access: *self.last_provider_access.read().unwrap(),
            last_backend_access: *self.last_backend_access.read().unwrap(),
        }
    }
}

/// Clonable snapshot of instance metrics
#[derive(Clone, Debug)]
pub struct InstanceMetricsSnapshot {
    pub provider_requests: u64,
    pub backend_requests: u64,
    pub last_provider_access: Option<Instant>,
    pub last_backend_access: Option<Instant>,
}

/// A managed blockchain instance (Anvil or External)
pub struct ManagedInstance {
    /// Unique identifier
    id: Uuid,
    /// Human-readable name (from config key)
    name: String,
    /// Chain ID
    chain_id: u64,
    /// Block number at initialization
    block_number: u64,
    /// RPC endpoint URL
    endpoint: String,
    /// Source (Anvil or External)
    source: InstanceSource,
    /// Cached RootProvider (lazy-loaded)
    provider: OnceCell<Arc<RootProvider<AnyNetwork>>>,
    /// Creation timestamp
    created_at: Instant,
    /// Usage metrics
    metrics: InstanceMetrics,
}

impl ManagedInstance {
    pub async fn spawn_anvil(name: String, config: AnvilInstanceConfig) -> Result<Self> {
        let (child, endpoint, port, block_number) = spawn_anvil_process(&config).await?;
        Ok(Self {
            id: Uuid::new_v4(),
            name,
            chain_id: config.chain_id,
            block_number,
            endpoint,
            source: InstanceSource::Anvil {
                child: Mutex::new(child),
                port,
            },
            provider: OnceCell::new(),
            created_at: Instant::now(),
            metrics: InstanceMetrics::default(),
        })
    }

    pub async fn from_external(name: String, config: ExternalConfig) -> Result<Self> {
        let block_number = fetch_block_number(&config.rpc_url, None).await?;
        Ok(Self {
            id: Uuid::new_v4(),
            name,
            chain_id: config.chain_id,
            block_number,
            endpoint: config.rpc_url,
            source: InstanceSource::External,
            provider: OnceCell::new(),
            created_at: Instant::now(),
            metrics: InstanceMetrics::default(),
        })
    }

    /// Get the instance UUID
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get the instance name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the chain ID
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Get the block number
    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    /// Get the RPC endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Check if this is a managed (Anvil) instance
    pub fn is_managed(&self) -> bool {
        matches!(self.source, InstanceSource::Anvil { .. })
    }

    /// Get the creation timestamp
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    /// Get a snapshot of the metrics
    pub fn metrics_snapshot(&self) -> InstanceMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn record_backend_access(&self) {
        self.metrics.record_backend_access();
    }

    /// Get or create a cached RootProvider
    pub fn get_or_create_provider(&self) -> Result<Arc<RootProvider<AnyNetwork>>> {
        self.metrics.record_provider_access();

        let provider = self.provider.get_or_try_init(|| {
            let url = self.endpoint().parse().context("Invalid RPC URL")?;
            let provider = RootProvider::<AnyNetwork>::new_http(url);
            Ok::<_, anyhow::Error>(Arc::new(provider))
        })?;

        Ok(Arc::clone(provider))
    }

    /// Shutdown the instance (kills Anvil process if managed)
    pub async fn shutdown(&self) -> Result<()> {
        if let InstanceSource::Anvil { child, port } = &self.source {
            let mut guard = child.lock().await;
            tracing::info!("Killing anvil process on port {}", port);
            guard.kill().await.context("Failed to kill anvil")?;
        }
        Ok(())
    }
}

/// Read-only snapshot of instance information
#[derive(Clone, Debug)]
pub struct InstanceInfo {
    pub id: Uuid,
    pub name: String,
    pub chain_id: u64,
    pub block_number: u64,
    pub is_managed: bool,
    pub endpoint: String,
    pub created_at: Instant,
}

impl From<&ManagedInstance> for InstanceInfo {
    fn from(instance: &ManagedInstance) -> Self {
        Self {
            id: instance.id,
            name: instance.name.clone(),
            chain_id: instance.chain_id,
            block_number: instance.block_number,
            is_managed: instance.is_managed(),
            endpoint: instance.endpoint().to_string(),
            created_at: instance.created_at,
        }
    }
}

struct FetchRetry {
    timeout: Duration,
    delay: Duration,
}

async fn spawn_anvil_process(
    config: &AnvilInstanceConfig,
) -> Result<(Child, String, u16, u64)> {
    let anvil_bin = config.anvil_bin.as_deref().unwrap_or("anvil");

    if !is_anvil_available(anvil_bin).await {
        anyhow::bail!(
            "Anvil binary '{}' not found. Install Foundry: https://getfoundry.sh",
            anvil_bin
        );
    }

    let mut cmd = Command::new(anvil_bin);

    // Remove proxy env vars to avoid Backend::spawn deadlock
    cmd.env_remove("http_proxy");
    cmd.env_remove("https_proxy");
    cmd.env_remove("HTTP_PROXY");
    cmd.env_remove("HTTPS_PROXY");
    cmd.env_remove("all_proxy");
    cmd.env_remove("ALL_PROXY");

    let requested_port = if config.port == 0 {
        find_available_port().await?
    } else {
        ensure_port_available(config.port)?;
        config.port
    };
    cmd.arg("--port").arg(requested_port.to_string());

    let host = std::env::var("ANVIL_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    cmd.arg("--host").arg(host);
    cmd.arg("--chain-id").arg(config.chain_id.to_string());

    if let Some(ref fork_url) = config.fork_url {
        cmd.arg("--fork-url").arg(fork_url);
        if let Some(block) = config.fork_block_number {
            cmd.arg("--fork-block-number").arg(block.to_string());
        }
    }

    if let Some(block_time) = config.block_time {
        cmd.arg("--block-time").arg(block_time.to_string());
    }

    cmd.arg("--accounts").arg(config.accounts.to_string());

    if let Some(ref mnemonic) = config.mnemonic {
        cmd.arg("--mnemonic").arg(mnemonic);
    }

    if let Some(ref state_path) = config.load_state {
        cmd.arg("--load-state").arg(state_path);
    }

    if let Some(ref dump_state) = config.dump_state {
        cmd.arg("--dump-state").arg(dump_state);
    }

    if config.steps_tracing {
        cmd.arg("--steps-tracing");
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    tracing::info!("Spawning anvil: {:?}", cmd);

    let mut child = cmd.spawn().context("Failed to spawn anvil process")?;
    let stdout = child.stdout.take().context("Failed to get stdout")?;

    // Drain stderr in background
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!("anvil stderr: {}", line);
            }
        });
    }

    let mut reader = BufReader::new(stdout).lines();
    let port = wait_for_ready(
        &mut reader,
        requested_port,
        &mut child,
        Duration::from_secs(30),
    )
    .await
    .context("Anvil failed to start")?;

    let endpoint = format!("http://127.0.0.1:{}", port);
    let block_number = fetch_block_number(
        &endpoint,
        Some(FetchRetry {
            timeout: Duration::from_secs(20),
            delay: Duration::from_millis(200),
        }),
    )
    .await
    .context("failed to fetch block number after anvil ready")?;

    tracing::info!(
        "Anvil ready at {} (chain_id: {})",
        endpoint,
        config.chain_id
    );

    Ok((child, endpoint, port, block_number))
}

async fn fetch_block_number(endpoint: &str, retry: Option<FetchRetry>) -> Result<u64> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .context("failed to build reqwest client")?;

    let fetch_once = || async {
        let response = client
            .post(endpoint)
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "eth_blockNumber",
                "params": [],
                "id": 1
            }))
            .send()
            .await
            .context("failed to request eth_blockNumber")?;

        let status = response.status();
        let body = response
            .bytes()
            .await
            .context("failed to read eth_blockNumber response body")?;

        if !status.is_success() {
            if body.is_empty() {
                anyhow::bail!("HTTP error {} with empty body", status);
            } else {
                anyhow::bail!(
                    "HTTP error {}: {}",
                    status,
                    String::from_utf8_lossy(&body)
                );
            }
        }

        let json_response: serde_json::Value =
            serde_json::from_slice(&body).context("invalid json from eth_blockNumber")?;
        let hex_block = json_response
            .get("result")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing result in eth_blockNumber response"))?;
        let trimmed = hex_block.trim_start_matches("0x");
        u64::from_str_radix(trimmed, 16)
            .context("failed to parse eth_blockNumber response as hex u64")
    };

    match retry {
        None => fetch_once().await,
        Some(config) => {
            let start = Instant::now();
            let mut last_err = None;
            loop {
                match fetch_once().await {
                    Ok(block_number) => return Ok(block_number),
                    Err(err) => {
                        last_err = Some(err);
                        if start.elapsed() >= config.timeout {
                            break;
                        }
                    }
                }
                sleep(config.delay).await;
            }
            Err(last_err.unwrap_or_else(|| anyhow::anyhow!("failed to fetch block number")))
        }
    }
}

async fn is_anvil_available(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn find_available_port() -> Result<u16> {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

fn ensure_port_available(port: u16) -> Result<()> {
    use std::net::TcpListener;
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(err) => anyhow::bail!(
            "Port {} is already in use ({}). Stop the existing process or choose another port.",
            port,
            err
        ),
    }
}

async fn wait_for_ready(
    reader: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    requested_port: u16,
    child: &mut Child,
    timeout_duration: Duration,
) -> Result<u16> {
    let result = timeout(timeout_duration, async {
        while let Some(line) = reader.next_line().await? {
            if line.contains("Listening on") {
                if let Some(addr) = line.split("Listening on").nth(1) {
                    let addr = addr.trim();
                    if let Some(port_str) = addr.rsplit(':').next() {
                        if let Ok(port) = port_str.parse::<u16>() {
                            return Ok(port);
                        }
                    }
                }
                return Ok(requested_port);
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            anyhow::bail!("Anvil exited with status: {:?}", status);
        }
        anyhow::bail!("Anvil stdout closed unexpectedly")
    })
    .await;

    match result {
        Ok(Ok(port)) => Ok(port),
        Ok(Err(e)) => Err(e),
        Err(_) => anyhow::bail!("Timeout waiting for Anvil to start"),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_and_kill() {
        if !is_anvil_available("anvil").await {
            eprintln!("Skipping test: anvil not installed");
            return;
        }

        let config = AnvilInstanceConfig::local(31337);
        let instance = ManagedInstance::spawn_anvil("ethereum".to_string(), config)
            .await
            .expect("spawn failed");

        assert!(instance.is_managed());
        instance.shutdown().await.expect("kill failed");
    }

    #[tokio::test]
    async fn test_spawn_with_specific_port() {
        if !is_anvil_available("anvil").await {
            eprintln!("Skipping test: anvil not installed");
            return;
        }

        let port = find_available_port().await.expect("get port");
        let config = AnvilInstanceConfig::local(31337).with_port(port);
        let instance = ManagedInstance::spawn_anvil("ethereum".to_string(), config)
            .await
            .expect("spawn failed");

        assert!(instance.endpoint().starts_with("http://127.0.0.1:"));
        assert!(instance.endpoint().ends_with(&format!(":{}", port)));
        instance.shutdown().await.expect("kill failed");
    }
}
