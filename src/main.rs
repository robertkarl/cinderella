mod agent;
mod config;
mod hw;
mod llm;
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

    /// Path to a custom GGUF model file (BYOM).
    #[arg(long)]
    model: Option<PathBuf>,

    /// Port for llama-server (default: 8787).
    #[arg(long, default_value_t = config::DEFAULT_PORT)]
    port: u16,

    /// Path to llama-server binary.
    #[arg(long)]
    llama_server: Option<PathBuf>,
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
    };

    orchestrator::run(cfg).await
}
