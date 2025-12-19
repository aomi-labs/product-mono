use crate::config::AnvilParams;
use anyhow::{Context, Result};
use serde_json::json;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{timeout, Duration};

pub struct AnvilInstance {
    child: Option<Child>,
    endpoint: String,
    port: u16,
    chain_id: u64,
    block_number: u64,
}

impl AnvilInstance {
    pub async fn spawn(config: AnvilParams) -> Result<Self> {
        let anvil_bin = config.anvil_bin.as_deref().unwrap_or("anvil");

        if !Self::is_anvil_available(anvil_bin).await {
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

        // If port is 0, find an available port; otherwise use the specified port
        let requested_port = if config.port == 0 {
            Self::find_available_port().await?
        } else {
            config.port
        };
        cmd.arg("--port").arg(requested_port.to_string());

        cmd.arg("--host").arg("127.0.0.1");
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

        // Note: Don't use --silent as it suppresses "Listening on" message needed for ready detection

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
        let port = Self::wait_for_ready(
            &mut reader,
            requested_port,
            &mut child,
            Duration::from_secs(30),
        )
        .await
        .context("Anvil failed to start")?;

        let endpoint = format!("http://127.0.0.1:{}", port);
        let block_number = fetch_block_number(&endpoint).await?;
        tracing::info!(
            "Anvil ready at {} (chain_id: {})",
            endpoint,
            config.chain_id
        );

        Ok(Self {
            child: Some(child),
            endpoint,
            port,
            chain_id: config.chain_id,
            block_number,
        })
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
            // Stdout closed - check if process exited with error
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

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    pub async fn kill(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            tracing::info!("Killing anvil process on port {}", self.port);
            child.kill().await.context("Failed to kill anvil")?;
        }
        Ok(())
    }
}

pub async fn fetch_block_number(endpoint: &str) -> Result<u64> {
    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .post(endpoint)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_blockNumber",
            "params": []
        }))
        .send()
        .await
        .context("failed to request eth_blockNumber")?
        .json()
        .await
        .context("invalid json from eth_blockNumber")?;

    let result = resp
        .get("result")
        .and_then(|r| r.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing result in eth_blockNumber response"))?;

    let trimmed = result.trim_start_matches("0x");
    u64::from_str_radix(trimmed, 16).context("failed to parse eth_blockNumber response as hex u64")
}

impl Drop for AnvilInstance {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            tracing::info!(
                "Dropping AnvilInstance, killing process on port {}",
                self.port
            );
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_and_kill() {
        if !AnvilInstance::is_anvil_available("anvil").await {
            eprintln!("Skipping test: anvil not installed");
            return;
        }

        let config = AnvilParams::default();
        let mut instance = AnvilInstance::spawn(config).await.expect("spawn failed");

        assert!(instance.is_running());
        assert!(instance.port() > 0);
        assert!(instance.endpoint().starts_with("http://127.0.0.1:"));

        instance.kill().await.expect("kill failed");
        assert!(!instance.is_running());
    }

    #[tokio::test]
    async fn test_spawn_with_specific_port() {
        if !AnvilInstance::is_anvil_available("anvil").await {
            eprintln!("Skipping test: anvil not installed");
            return;
        }

        // Use a random available port for the test
        let port = AnvilInstance::find_available_port()
            .await
            .expect("find port");
        let config = AnvilParams::default().with_port(port);
        let instance = AnvilInstance::spawn(config).await.expect("spawn failed");

        assert_eq!(instance.port(), port);
        assert_eq!(instance.endpoint(), format!("http://127.0.0.1:{}", port));
    }
}
