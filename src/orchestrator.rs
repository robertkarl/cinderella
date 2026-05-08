/// Orchestrator: RAM check → find/extract model → start server → launch agent + TUI.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::agent::Agent;
use crate::config::{self, SafetyProfile};
use crate::model_manifest::ModelDef;
use crate::hw;
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

    // Step 2: Load manifest and select model for this machine's RAM
    let manifest = crate::model_manifest::find_manifest()
        .context("Failed to load model manifest")?;
    let active_model = manifest.model_for_ram(hw.total_ram_gb as u32)
        .context("No model fits this machine's RAM")?;

    // Step 3: Find model file
    let model_path = match cfg.model_path {
        Some(p) => {
            if !p.exists() {
                anyhow::bail!("Model file not found: {}", p.display());
            }
            println!("Model: {} (user-provided)", p.display());
            p
        }
        None => {
            let path = find_model_file(active_model)?;
            let size_gb = active_model.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            println!(
                "Model: {} {} ({:.1} GiB) \u{2713}",
                active_model.name, active_model.quant, size_gb
            );
            path
        }
    };

    // Step 4: Start llama-server
    let server_config = config::ServerConfig::from_model_def(model_path, cfg.port, active_model);
    let mut server = ServerManager::new(server_config, cfg.llama_server_path);

    println!("Starting llama-server...");
    server.start().await.context("Failed to start llama-server")?;
    println!("\u{2713} Health check: ok");

    if let (Some(loaded), Some(total)) = (server.gpu_layers_loaded, server.gpu_layers_total) {
        println!("\u{2713} GPU layers: {}/{}", loaded, total);
    }

    // Step 5: Prompt mode or interactive TUI
    if let Some(ref prompt) = cfg.prompt {
        let result = run_prompt(
            &server.api_url(),
            cfg.project_dir.clone(),
            &cfg.model_name,
            cfg.safety_profile,
            prompt,
            cfg.format_json,
            active_model.ctx_size,
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
        model_name: active_model.name.clone(),
        quant: active_model.quant.clone(),
        tok_per_sec: None,
        ram_used_gb: hw.total_ram_gb - hw.available_ram_gb,
        ram_total_gb: hw.total_ram_gb,
        ctx_used: 0,
        ctx_max: active_model.ctx_size as usize,
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
        active_model.ctx_size,
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
    // Load manifest to get ctx_size even in remote mode
    let manifest = crate::model_manifest::find_manifest()
        .context("Failed to load model manifest")?;
    let default_model = manifest.default_model()
        .context("No default model in manifest")?;
    let ctx_size = default_model.ctx_size;

    // Prompt mode
    if let Some(ref prompt) = cfg.prompt {
        return run_prompt(
            api_url,
            cfg.project_dir.clone(),
            &cfg.model_name,
            cfg.safety_profile,
            prompt,
            cfg.format_json,
            ctx_size,
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
        ctx_max: ctx_size as usize,
        gpu_layers: "remote".to_string(),
    };

    let agent_handle = spawn_agent_loop(
        api_url,
        cfg.project_dir.clone(),
        ctx_size,
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
    ctx_size: u32,
) -> Result<()> {
    let mut agent = Agent::new(api_url, project_dir, ctx_size, model_name, safety_profile, format_json);

    if format_json {
        // Emit hardware info before agent starts
        if let Ok(hw) = hw::detect() {
            tui::json_hw_info(
                &hw.chip_name,
                hw.total_ram_gb - hw.available_ram_gb,
                hw.total_ram_gb,
                "remote",
            );
        }
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

/// Find the model file on disk.
/// Checks the manifest-defined path (Application Support) first,
/// then falls back to ~/models/ for development convenience.
fn find_model_file(model: &ModelDef) -> Result<PathBuf> {
    // Primary location: Application Support (where the GUI downloader puts it)
    let app_support_path = model.model_path();
    if app_support_path.exists() {
        return Ok(app_support_path);
    }

    // Development fallback: ~/models/ (legacy location)
    if !is_release_bundle() {
        let home = std::env::var("HOME").unwrap_or_default();
        let legacy_path = PathBuf::from(&home).join("models").join(&model.filename);
        if legacy_path.exists() {
            return Ok(legacy_path);
        }
    }

    anyhow::bail!(
        "Model not found at {}.\n\
         The Glass Slipper app downloads the model on first launch.\n\
         For development: download {} and place it in ~/Library/Application Support/{}/",
        app_support_path.display(),
        model.filename,
        model.app_support_subdir
    );
}

/// Find the llama-server binary.
///
/// In release mode (inside an app bundle), only the bundled copy is accepted.
/// Development mode additionally checks ~/.cinderella/bin and PATH.
pub fn find_llama_server(custom_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = custom_path {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        anyhow::bail!("llama-server not found at {}", p.display());
    }

    if let Some(bundled) = find_bundled_llama_server() {
        return Ok(bundled);
    }

    if is_release_bundle() {
        anyhow::bail!(
            "llama-server not found in app bundle.\n\
             The release build requires llama-server at Contents/MacOS/llama-server.\n\
             This is a packaging error — run scripts/package-macos.sh to rebuild."
        );
    }

    find_dev_llama_server()
}

/// Check for llama-server bundled next to the current executable.
fn find_bundled_llama_server() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundled = exe.parent()?.join("llama-server");
    bundled.exists().then_some(bundled)
}

/// Development-only fallbacks: ~/.cinderella/bin/ and PATH.
fn find_dev_llama_server() -> Result<PathBuf> {
    let home_bin = config::cinderella_home().join("bin").join("llama-server");
    if home_bin.exists() {
        return Ok(home_bin);
    }

    if let Some(path) = find_in_path("llama-server") {
        return Ok(path);
    }

    anyhow::bail!(
        "llama-server not found. Install it or provide --llama-server <path>.\n\
         For development: brew install llama.cpp"
    );
}

/// Find a binary in PATH using `which`.
fn find_in_path(name: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("which").arg(name).output().ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(PathBuf::from(path)) }
}

/// Returns true if the current binary is running from inside a macOS app bundle.
/// Detected by checking if the executable's great-grandparent is a .app directory.
fn is_release_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            // exe = Foo.app/Contents/MacOS/cinderella-agent
            // parent = Foo.app/Contents/MacOS
            // grandparent = Foo.app/Contents
            // great-grandparent = Foo.app
            let macos_dir = exe.parent()?;
            let contents_dir = macos_dir.parent()?;
            let bundle = contents_dir.parent()?;
            let bundle_name = bundle.file_name()?.to_str()?;
            Some(bundle_name.ends_with(".app"))
        })
        .unwrap_or(false)
}
