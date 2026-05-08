mod agent;
mod config;
mod hw;
mod llm;
mod memory_ffi;
mod memory_monitor;
mod model_manifest;
mod orchestrator;
mod server;
mod tools;
mod tui;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// Cinderella: local AI coding agent. The shoe that fits.
#[derive(Parser)]
#[command(name = "cinderella", version, about)]
struct Cli {
    /// Project directory to work in.
    project: PathBuf,

    /// Non-interactive prompt mode: send one prompt, stream output, exit.
    #[arg(short = 'p', long = "prompt")]
    prompt: Option<String>,

    /// Use the network-debug playbook (allows diagnostic commands like curl, dig, traceroute).
    #[arg(long = "playbook", value_name = "NAME")]
    playbook: Option<String>,

    /// Path to a custom GGUF model file (BYOM).
    #[arg(long)]
    model: Option<PathBuf>,

    /// Port for llama-server (default: 8787).
    #[arg(long, default_value_t = config::DEFAULT_PORT)]
    port: u16,

    /// Path to llama-server binary.
    #[arg(long)]
    llama_server: Option<PathBuf>,

    /// Connect to a remote OpenAI-compatible API instead of launching a local llama-server.
    /// Example: --api-url http://192.168.50.4:11434
    #[arg(long)]
    api_url: Option<String>,

    /// Model name to send in API requests (for llama-swap routing).
    /// Default: "local" for local llama-server.
    #[arg(long, default_value = "local")]
    model_name: String,

    /// Output format. Use "json" for JSON-lines output (for machine consumption).
    #[arg(long, default_value = "text")]
    format: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Validate project directory
    let project_dir = cli.project.canonicalize().unwrap_or_else(|_| {
        eprintln!(
            "Directory {} does not exist.",
            cli.project.display()
        );
        std::process::exit(1);
    });

    if !project_dir.is_dir() {
        eprintln!("{} is not a directory.", project_dir.display());
        std::process::exit(1);
    }

    // Validate format flag
    let format_json = match cli.format.as_str() {
        "json" => true,
        "text" => false,
        other => {
            eprintln!("Unknown format: {}. Available: text, json", other);
            std::process::exit(1);
        }
    };

    // Resolve playbook to safety profile
    let safety_profile = match cli.playbook.as_deref() {
        Some("network-debug") => config::SafetyProfile::NetworkDebug,
        Some(name) => {
            eprintln!("Unknown playbook: {}. Available: network-debug", name);
            std::process::exit(1);
        }
        None => config::SafetyProfile::default(),
    };

    if let Some(api_url) = cli.api_url {
        // Remote mode: skip local server, connect directly
        let cfg = orchestrator::OrchestratorConfig {
            project_dir,
            model_path: cli.model,
            port: cli.port,
            llama_server_path: PathBuf::new(),
            api_url: Some(api_url),
            model_name: cli.model_name,
            safety_profile,
            prompt: cli.prompt,
            format_json,
        };
        return orchestrator::run(cfg).await;
    }

    // Find llama-server
    let llama_server_path =
        orchestrator::find_llama_server(cli.llama_server.as_deref()).unwrap_or_else(|e| {
            eprintln!("{}", e);
            std::process::exit(1);
        });

    let cfg = orchestrator::OrchestratorConfig {
        project_dir,
        model_path: cli.model,
        port: cli.port,
        llama_server_path,
        api_url: None,
        model_name: cli.model_name,
        safety_profile,
        prompt: cli.prompt,
        format_json,
    };

    orchestrator::run(cfg).await
}
