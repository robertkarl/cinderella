/// Orchestrator: RAM check → find/extract model → start server → launch agent + TUI.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::agent::Agent;
use crate::config::{self, SafetyProfile, BUNDLED_MODEL};
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
    /// Remote API URL — skip local server if set.
    pub api_url: Option<String>,
    /// Model name for API requests (llama-swap routing).
    pub model_name: String,
    /// Safety profile for the agent.
    pub safety_profile: SafetyProfile,
    /// Non-interactive prompt mode: send one prompt, stream output, exit.
    pub prompt: Option<String>,
    /// Output format: true for JSON-lines, false for plain text.
    pub format_json: bool,
}

/// Run the full orchestration flow.
pub async fn run(cfg: OrchestratorConfig) -> Result<()> {
    // Remote mode: skip local server entirely
    if let Some(ref api_url) = cfg.api_url {
        println!("Connecting to remote API: {}", api_url);
        return run_remote(api_url, &cfg).await;
    }

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

    // Step 4: Prompt mode or interactive TUI
    if let Some(ref prompt) = cfg.prompt {
        let result = run_prompt(
            &server.api_url(),
            cfg.project_dir.clone(),
            &cfg.model_name,
            cfg.safety_profile,
            prompt,
            cfg.format_json,
        )
        .await;
        server.stop().await;
        return result;
    }

    // Interactive mode
    let (agent_tx, agent_rx) = mpsc::channel::<crate::agent::AgentEvent>(256);
    let (cmd_tx, cmd_rx) = mpsc::channel::<TuiCommand>(32);

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

    let agent_handle = spawn_agent_loop(
        &server.api_url(),
        cfg.project_dir.clone(),
        BUNDLED_MODEL.ctx_size,
        cfg.model_name.clone(),
        cfg.safety_profile,
        agent_tx,
        cmd_rx,
    );

    tui::run(agent_rx, cmd_tx, &project_name, initial_status).await?;

    agent_handle.abort();
    server.stop().await;

    Ok(())
}

/// Run with a remote API — no local server.
async fn run_remote(api_url: &str, cfg: &OrchestratorConfig) -> Result<()> {
    // Prompt mode
    if let Some(ref prompt) = cfg.prompt {
        return run_prompt(
            api_url,
            cfg.project_dir.clone(),
            &cfg.model_name,
            cfg.safety_profile,
            prompt,
            cfg.format_json,
        )
        .await;
    }

    // Interactive mode
    let (agent_tx, agent_rx) = mpsc::channel::<crate::agent::AgentEvent>(256);
    let (cmd_tx, cmd_rx) = mpsc::channel::<TuiCommand>(32);

    let project_name = cfg
        .project_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();

    let initial_status = tui::StatusBar {
        model_name: "remote".to_string(),
        quant: String::new(),
        tok_per_sec: None,
        ram_used_gb: 0.0,
        ram_total_gb: 0.0,
        ctx_used: 0,
        ctx_max: BUNDLED_MODEL.ctx_size as usize,
        gpu_layers: "remote".to_string(),
    };

    let agent_handle = spawn_agent_loop(
        api_url,
        cfg.project_dir.clone(),
        BUNDLED_MODEL.ctx_size,
        cfg.model_name.clone(),
        cfg.safety_profile,
        agent_tx,
        cmd_rx,
    );

    tui::run(agent_rx, cmd_tx, &project_name, initial_status).await?;

    agent_handle.abort();
    Ok(())
}

/// Non-interactive prompt mode: send one prompt, stream output, exit.
/// The agent loop handles tool calls internally — process_message keeps
/// iterating until the LLM produces a final text response.
async fn run_prompt(
    api_url: &str,
    project_dir: PathBuf,
    model_name: &str,
    safety_profile: SafetyProfile,
    prompt: &str,
    format_json: bool,
) -> Result<()> {
    let mut agent = Agent::new(api_url, project_dir, BUNDLED_MODEL.ctx_size, model_name, safety_profile, format_json);

    if format_json {
        agent
            .process_message(prompt, |event| {
                tui::json_event(event);
            })
            .await?;
    } else {
        let mut state = tui::OutputState::default();
        agent
            .process_message(prompt, |event| {
                tui::print_event(event, &mut state);
            })
            .await?;
    }

    Ok(())
}

/// Spawn the agent command loop as a tokio task.
/// Shared between local-server and remote-API modes.
fn spawn_agent_loop(
    api_url: &str,
    project_dir: PathBuf,
    ctx_size: u32,
    model_name: String,
    safety_profile: SafetyProfile,
    agent_tx: mpsc::Sender<crate::agent::AgentEvent>,
    mut cmd_rx: mpsc::Receiver<TuiCommand>,
) -> tokio::task::JoinHandle<()> {
    let api_url = api_url.to_string();
    tokio::spawn(async move {
        let mut agent = Agent::new(&api_url, project_dir, ctx_size, &model_name, safety_profile, false);

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                TuiCommand::SendMessage(msg) => {
                    let tx = agent_tx.clone();
                    if let Err(e) = agent
                        .process_message(&msg, |event| {
                            let _ = tx.try_send(event);
                        })
                        .await
                    {
                        let _ = agent_tx
                            .send(crate::agent::AgentEvent::Warning(format!("Error: {}", e)))
                            .await;
                    }
                }
                TuiCommand::Clear => {
                    agent.clear();
                }
                TuiCommand::Cancel => {
                    let _ = agent_tx.try_send(crate::agent::AgentEvent::Warning(
                        "Cancel requested but not yet implemented for in-flight operations."
                            .to_string(),
                    ));
                }
                TuiCommand::Quit => break,
            }
        }
    })
}

/// Find the bundled model in ~/.cinderella/models/ or extract from release archive.
fn find_or_extract_bundled_model(hw: &HardwareInfo) -> Result<PathBuf> {
    // Check RAM — use total RAM, not available. macOS unified memory will reclaim
    // inactive/purgeable pages under pressure, so "available" is misleadingly low.
    if hw.total_ram_gb < BUNDLED_MODEL.total_ram_required_gb {
        anyhow::bail!(
            "Cannot fit bundled model ({} needs ~{:.0} GiB).\n\
             Your Mac has {:.0} GB total.\n\
             Try: cinderella --model /path/to/smaller-model.gguf <project>",
            BUNDLED_MODEL.name,
            BUNDLED_MODEL.total_ram_required_gb,
            hw.total_ram_gb
        );
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
