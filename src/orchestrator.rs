/// Orchestrator: RAM check → find/extract model → start server → launch agent + TUI.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::agent::Agent;
use crate::config::{self, BUNDLED_MODEL};
use crate::hw::{self, HardwareInfo};
use crate::server::ServerManager;
use crate::tui::{self, StatusBar, TuiCommand};
use tokio::sync::mpsc;

pub struct OrchestratorConfig {
    pub project_dir: PathBuf,
    pub model_path: Option<PathBuf>,
    pub port: u16,
    /// Path to llama-server binary (bundled or system).
    pub llama_server_path: PathBuf,
}

/// Run the full orchestration flow.
pub async fn run(cfg: OrchestratorConfig) -> Result<()> {
    // Step 1: Detect hardware
    let hw = hw::detect().context("Hardware detection failed")?;
    println!("Hardware: {}", hw);

    // Step 2: Find model
    let model_path = match cfg.model_path {
        Some(p) => {
            if !p.exists() {
                anyhow::bail!("Model file not found: {}", p.display());
            }
            println!("Model: {} (user-provided)", p.display());
            p
        }
        None => {
            let path = find_or_extract_bundled_model(&hw)?;
            println!(
                "Model: {} {} ({:.1} GiB) \u{2713}",
                BUNDLED_MODEL.name, BUNDLED_MODEL.quant, BUNDLED_MODEL.size_gb
            );
            path
        }
    };

    // Step 3: Start llama-server
    let server_config = config::ServerConfig::from_model(model_path, cfg.port, &BUNDLED_MODEL);
    let mut server = ServerManager::new(server_config, cfg.llama_server_path);

    println!("Starting llama-server...");
    server.start().await.context("Failed to start llama-server")?;
    println!("\u{2713} Health check: ok");

    if let (Some(loaded), Some(total)) = (server.gpu_layers_loaded, server.gpu_layers_total) {
        println!("\u{2713} GPU layers: {}/{}", loaded, total);
    }

    // Step 4: Launch agent + TUI
    let (agent_tx, agent_rx) = mpsc::channel::<crate::agent::AgentEvent>(256);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<TuiCommand>(32);

    let project_name = cfg
        .project_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();

    let initial_status = StatusBar {
        model_name: BUNDLED_MODEL.name.to_string(),
        quant: BUNDLED_MODEL.quant.to_string(),
        tok_per_sec: None,
        ram_used_gb: hw.total_ram_gb - hw.available_ram_gb,
        ram_total_gb: hw.total_ram_gb,
        ctx_used: 0,
        ctx_max: BUNDLED_MODEL.ctx_size as usize,
        gpu_layers: server
            .gpu_layers_loaded
            .map(|l| {
                format!(
                    "{}/{}",
                    l,
                    server.gpu_layers_total.unwrap_or(l)
                )
            })
            .unwrap_or_else(|| "\u{2014}".to_string()),
    };

    let api_url = server.api_url();
    let project_dir = cfg.project_dir.clone();
    let ctx_size = BUNDLED_MODEL.ctx_size;

    // Agent task
    let agent_handle = tokio::spawn(async move {
        let mut agent = Agent::new(&api_url, project_dir, ctx_size);

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                TuiCommand::SendMessage(msg) => {
                    let tx = agent_tx.clone();
                    if let Err(e) = agent
                        .process_message(&msg, |event| {
                            let _ = tx.blocking_send(event);
                        })
                        .await
                    {
                        let _ = agent_tx.send(crate::agent::AgentEvent::Warning(
                            format!("Error: {}", e),
                        )).await;
                    }
                }
                TuiCommand::Clear => {
                    agent.clear();
                }
                TuiCommand::Cancel => {
                    // TODO: cancel running tool execution
                }
                TuiCommand::Quit => break,
            }
        }
    });

    // TUI task (runs on main thread — crossterm needs it)
    tui::run(agent_rx, cmd_tx, &project_name, initial_status).await?;

    // Cleanup
    agent_handle.abort();
    server.stop().await;

    Ok(())
}

/// Find the bundled model in ~/.cinderella/models/ or extract from release archive.
fn find_or_extract_bundled_model(hw: &HardwareInfo) -> Result<PathBuf> {
    // Check RAM
    if hw.available_ram_gb < BUNDLED_MODEL.total_ram_required_gb {
        if hw.total_ram_gb < 16.0 {
            anyhow::bail!(
                "Cannot fit bundled model ({} needs ~{:.0} GiB free).\n\
                 Your Mac has {:.0} GB total. Cinderella needs at least 16 GB.\n\
                 Try: cinderella --model /path/to/smaller-model.gguf <project>",
                BUNDLED_MODEL.name,
                BUNDLED_MODEL.total_ram_required_gb,
                hw.total_ram_gb
            );
        } else {
            anyhow::bail!(
                "Not enough memory. Need ~{:.0} GiB free, you have {:.1} GiB.\n\
                 Close other applications and try again.",
                BUNDLED_MODEL.total_ram_required_gb,
                hw.available_ram_gb
            );
        }
    }

    let models_dir = config::models_dir();
    let model_path = models_dir.join(BUNDLED_MODEL.filename);

    if model_path.exists() {
        return Ok(model_path);
    }

    // Model not yet extracted — in v1 bundled release, copy from archive location
    // For development, suggest downloading manually
    anyhow::bail!(
        "Model not found at {}.\n\
         For development: download {} and place it there.\n\
         In the release build, the model is bundled in the archive.",
        model_path.display(),
        BUNDLED_MODEL.filename
    );
}

/// Find the llama-server binary.
pub fn find_llama_server(custom_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = custom_path {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        anyhow::bail!("llama-server not found at {}", p.display());
    }

    // Check bundled location (next to cinderella binary)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("llama-server");
            if bundled.exists() {
                return Ok(bundled);
            }
        }
    }

    // Check ~/.cinderella/bin/
    let home_bin = config::cinderella_home().join("bin").join("llama-server");
    if home_bin.exists() {
        return Ok(home_bin);
    }

    // Check PATH
    if let Ok(output) = std::process::Command::new("which")
        .arg("llama-server")
        .output()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }

    anyhow::bail!(
        "llama-server not found. Install it or provide --llama-server <path>.\n\
         For development: brew install llama.cpp"
    );
}
