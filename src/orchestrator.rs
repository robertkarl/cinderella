/// Orchestrator: RAM check → find/extract model → start server → launch agent + TUI.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agent::Agent;
use crate::config::{self, SafetyProfile};
use crate::model_manifest::ModelDef;
use crate::hw;
use crate::server::ServerManager;
use crate::tui::{self, StatusBar, TuiCommand};
use tokio::sync::{mpsc, Mutex};

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
    /// Max consecutive tool failures before stopping.
    pub max_tool_failures: Option<u32>,
    /// Skip all permission checks (auto-approve every tool call).
    pub skip_permissions: bool,
}

/// Run the full orchestration flow.
pub async fn run(cfg: OrchestratorConfig) -> Result<()> {
    let _ = crate::logging::init(&crate::logging::log_dir());
    crate::logging::info("orchestrator", "Glass Slipper starting", None);

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
    let active_model = manifest.select_initial_model(hw.total_ram_gb as u32)
        .context("No model fits this machine's available RAM")?;

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
            cfg.max_tool_failures,
            cfg.skip_permissions,
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

    // Wrap server in Arc<Mutex> for shared access between health handler and main
    let api_url_str = server.api_url();
    let server = Arc::new(Mutex::new(server));

    // Create watch channels for memory monitor
    let (health_tx, health_rx) = tokio::sync::watch::channel(None::<crate::memory_monitor::HealthEvent>);
    let (tok_tx, tok_rx) = tokio::sync::watch::channel(None::<f64>);

    // Start the FFI pressure listener
    let pressure_rx = crate::memory_ffi::start_pressure_listener();

    // Determine if we're on a smaller model than the machine can handle
    let best_model = manifest.select_initial_model(hw.total_ram_gb as u32);
    let on_smaller_model = best_model.map(|m| m.tier != active_model.tier).unwrap_or(false);

    // Spawn the memory monitor
    tokio::spawn(crate::memory_monitor::run(health_tx, tok_rx, pressure_rx, on_smaller_model));

    let agent_handle = spawn_agent_loop(
        &api_url_str,
        cfg.project_dir.clone(),
        active_model.ctx_size,
        cfg.model_name.clone(),
        cfg.safety_profile,
        agent_tx.clone(),
        cmd_rx,
        Some(tok_tx),
        cfg.max_tool_failures,
        cfg.skip_permissions,
    );

    // Health event handler — reacts to MemoryMonitor state transitions
    let health_agent_tx = agent_tx.clone();
    let health_server = server.clone();
    let health_manifest = manifest.clone();
    let health_model_id = active_model.id.clone();
    let health_total_ram = hw.total_ram_gb;
    let health_port = cfg.port;

    let health_handle = tokio::spawn(async move {
        let mut health_rx = health_rx;
        let mut current_model_id = health_model_id;
        while health_rx.changed().await.is_ok() {
            let event = health_rx.borrow().clone();
            if let Some(health_event) = event {
                match health_event.health {
                    crate::memory_monitor::SystemHealth::Warning => {
                        let _ = health_agent_tx.send(crate::agent::AgentEvent::MemoryWarning {
                            pageout_rate: health_event.metrics.pageout_delta,
                            swap_used_mb: health_event.metrics.swap_used_mb,
                            tok_per_sec: health_event.metrics.last_tok_per_sec,
                        }).await;
                    }
                    crate::memory_monitor::SystemHealth::Critical => {
                        // Find current model and downgrade target
                        let current = health_manifest.models.iter()
                            .find(|m| m.id == current_model_id);
                        let target = current.and_then(|c| health_manifest.one_tier_down(c));

                        if let (Some(current_m), Some(target_m)) = (current, target) {
                            let target_path = find_model_file(target_m);
                            match target_path {
                                Ok(path) => {
                                    let new_config = config::ServerConfig::from_model_def(
                                        path, health_port, target_m,
                                    );
                                    let mut srv = health_server.lock().await;
                                    match srv.swap_model(new_config).await {
                                        Ok(()) => {
                                            let from = current_m.name.clone();
                                            let to = target_m.name.clone();
                                            current_model_id = target_m.id.clone();
                                            let _ = health_agent_tx.send(
                                                crate::agent::AgentEvent::ModelSwap {
                                                    from_model: from,
                                                    to_model: to,
                                                    reason: format!(
                                                        "System was thrashing (page-outs: {}/5s)",
                                                        health_event.metrics.pageout_delta
                                                    ),
                                                }
                                            ).await;
                                        }
                                        Err(e) => {
                                            let _ = health_agent_tx.send(
                                                crate::agent::AgentEvent::Warning(
                                                    format!("Model downgrade failed: {}", e)
                                                )
                                            ).await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = health_agent_tx.send(
                                        crate::agent::AgentEvent::Warning(
                                            format!("Downgrade model not found: {}", e)
                                        )
                                    ).await;
                                }
                            }
                        } else {
                            let _ = health_agent_tx.send(
                                crate::agent::AgentEvent::Warning(
                                    "Already on smallest model — cannot downgrade".to_string()
                                )
                            ).await;
                        }
                    }
                    crate::memory_monitor::SystemHealth::PromotionAvailable => {
                        if let Some(best) = health_manifest.select_initial_model(health_total_ram as u32) {
                            if best.id != current_model_id {
                                let _ = health_agent_tx.send(
                                    crate::agent::AgentEvent::PromotionAvailable {
                                        to_model: best.name.clone(),
                                    }
                                ).await;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    tui::run(agent_rx, cmd_tx, &project_name, initial_status).await?;

    agent_handle.abort();
    health_handle.abort();
    server.lock().await.stop().await;

    Ok(())
}

/// Run with a remote API — no local server.
async fn run_remote(api_url: &str, cfg: &OrchestratorConfig) -> Result<()> {
    // Load manifest to get ctx_size even in remote mode; fall back to 8192 if missing
    let ctx_size = crate::model_manifest::find_manifest()
        .ok()
        .and_then(|m| m.default_model().ok().map(|d| d.ctx_size))
        .unwrap_or(8192);

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
            cfg.max_tool_failures,
            cfg.skip_permissions,
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
        None,
        cfg.max_tool_failures,
        cfg.skip_permissions,
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
    max_tool_failures: Option<u32>,
    skip_permissions: bool,
) -> Result<()> {
    let mut agent = Agent::new(api_url, project_dir, ctx_size, model_name, safety_profile, format_json, None, max_tool_failures, skip_permissions);

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
    tok_per_sec_tx: Option<tokio::sync::watch::Sender<Option<f64>>>,
    max_tool_failures: Option<u32>,
    skip_permissions: bool,
) -> tokio::task::JoinHandle<()> {
    let api_url = api_url.to_string();
    tokio::spawn(async move {
        let mut agent = Agent::new(&api_url, project_dir, ctx_size, &model_name, safety_profile, false, tok_per_sec_tx, max_tool_failures, skip_permissions);

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
/// Development mode additionally checks ~/.glass-slipper/bin and PATH.
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

/// Development-only fallbacks: ~/.glass-slipper/bin/ and PATH.
fn find_dev_llama_server() -> Result<PathBuf> {
    let home_bin = config::glass_slipper_home().join("bin").join("llama-server");
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
            // exe = Foo.app/Contents/MacOS/glass-slipper-agent
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
