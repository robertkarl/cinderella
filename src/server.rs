/// llama-server lifecycle management.
/// Start, health check, GPU layer verification, auto-restart on crash.

use anyhow::{Context, Result};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

use crate::config::ServerConfig;

const HEALTH_CHECK_TIMEOUT_SECS: u64 = 60;
const HEALTH_CHECK_INTERVAL_MS: u64 = 500;
const MAX_RESTARTS: u32 = 3;

pub struct ServerManager {
    config: ServerConfig,
    llama_server_path: PathBuf,
    child: Option<Child>,
    restart_count: u32,
    pub gpu_layers_loaded: Option<u32>,
    pub gpu_layers_total: Option<u32>,
}

impl ServerManager {
    pub fn new(config: ServerConfig, llama_server_path: PathBuf) -> Self {
        Self {
            config,
            llama_server_path,
            child: None,
            restart_count: 0,
            gpu_layers_loaded: None,
            gpu_layers_total: None,
        }
    }

    /// Start llama-server and wait for health check.
    pub async fn start(&mut self) -> Result<()> {
        // Check if the port is already in use
        if std::net::TcpListener::bind(("127.0.0.1", self.config.port)).is_err() {
            anyhow::bail!(
                "Port {} is already in use. Another instance of cinderella or another service \
                 may be running. Try: cinderella --port {} <project>",
                self.config.port,
                self.config.port + 1
            );
        }

        let args = self.config.to_args();

        let child = Command::new(&self.llama_server_path)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!(
                    "Failed to start llama-server at {}",
                    self.llama_server_path.display()
                )
            })?;

        self.child = Some(child);

        // Wait for health check
        self.wait_for_health().await?;

        // Check GPU layers
        self.check_gpu_layers().await?;

        Ok(())
    }

    /// Wait for the server to respond to health checks.
    async fn wait_for_health(&self) -> Result<()> {
        let url = format!("http://127.0.0.1:{}/health", self.config.port);
        let client = reqwest::Client::new();
        let deadline = tokio::time::Instant::now()
            + Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS);

        loop {
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "llama-server health check timed out after {}s. \
                     Check if port {} is already in use.",
                    HEALTH_CHECK_TIMEOUT_SECS,
                    self.config.port
                );
            }

            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => return Ok(()),
                _ => sleep(Duration::from_millis(HEALTH_CHECK_INTERVAL_MS)).await,
            }
        }
    }

    /// Check GPU layer offloading via the /props or /slots endpoint.
    /// Reports None if we can't verify — the status bar shows "—" in that case.
    async fn check_gpu_layers(&mut self) -> Result<()> {
        // GPU layer count is not reliably exposed by llama-server's API.
        // Rather than lying about full offload, leave as None (unknown)
        // until we can parse it from server logs or a future API endpoint.
        self.gpu_layers_loaded = None;
        self.gpu_layers_total = None;
        Ok(())
    }

    /// Check if the server process is still running.
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(Some(_)) => false, // exited
                Ok(None) => true,     // still running
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Auto-restart if crashed. Returns true if restarted, false if max restarts exceeded.
    pub async fn ensure_running(&mut self) -> Result<bool> {
        if self.is_running() {
            return Ok(true);
        }

        if self.restart_count >= MAX_RESTARTS {
            anyhow::bail!(
                "llama-server keeps crashing ({} restarts). Check memory pressure.",
                MAX_RESTARTS
            );
        }

        self.restart_count += 1;
        self.start().await?;
        Ok(true)
    }

    /// Get the restart count for display.
    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    /// Get the API base URL.
    pub fn api_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.config.port)
    }

    /// Stop the server gracefully: SIGTERM, wait up to 5s, then SIGKILL.
    pub async fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            if let Some(pid) = child.id() {
                // Send SIGTERM for graceful shutdown
                let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);

                // Wait up to 5 seconds for graceful exit
                let graceful = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
                if graceful.is_err() {
                    // Graceful shutdown timed out — force kill
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                }
            } else {
                // No PID — already exited
                let _ = child.wait().await;
            }
        }
        self.child = None;
    }
}

impl Drop for ServerManager {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            // Best-effort kill on drop
            let _ = child.start_kill();
        }
    }
}
