/// Hardcoded model registry and server defaults.
/// Single model family (Qwen 3.5) for v1. Multi-model is v2.

pub const DEFAULT_PORT: u16 = 8787;
pub const DEFAULT_CTX_SIZE: u32 = 32768;
pub const CINDERELLA_DIR: &str = ".cinderella";
pub const MODELS_DIR: &str = "models";
pub const TEMPLATES_DIR: &str = "templates";

/// RAM tiers for model selection.
/// total_ram_required_gb includes model weights + KV cache + llama-server overhead.
pub struct ModelEntry {
    pub name: &'static str,
    pub filename: &'static str,
    pub size_gb: f64,
    pub total_ram_required_gb: f64,
    pub quant: &'static str,
    pub sha256: &'static str,
    pub ctx_size: u32,
    pub n_gpu_layers: i32,
}

/// The bundled model. v1 ships exactly one.
pub const BUNDLED_MODEL: ModelEntry = ModelEntry {
    name: "Qwen3.5-9B-abliterated",
    filename: "Qwen3.5-9B-abliterated.Q4_K_M.gguf",
    size_gb: 6.1,
    // Model ~5.5GB + KV cache ~2-4GB + server ~0.5GB
    total_ram_required_gb: 10.0,
    quant: "Q4_K_M",
    sha256: "TODO_FILL_AFTER_DOWNLOAD",
    ctx_size: DEFAULT_CTX_SIZE,
    n_gpu_layers: 999, // full offload
};

/// Server startup arguments derived from a model entry.
pub struct ServerConfig {
    pub model_path: std::path::PathBuf,
    pub port: u16,
    pub ctx_size: u32,
    pub n_gpu_layers: i32,
    pub jinja: bool,
}

impl ServerConfig {
    pub fn from_model(model_path: std::path::PathBuf, port: u16, entry: &ModelEntry) -> Self {
        Self {
            model_path,
            port,
            ctx_size: entry.ctx_size,
            n_gpu_layers: entry.n_gpu_layers,
            jinja: true,
        }
    }

    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec![
            "--model".to_string(),
            self.model_path.display().to_string(),
            "--port".to_string(),
            self.port.to_string(),
            "--ctx-size".to_string(),
            self.ctx_size.to_string(),
            "--n-gpu-layers".to_string(),
            self.n_gpu_layers.to_string(),
        ];
        if self.jinja {
            args.push("--jinja".to_string());
        }
        args
    }
}

/// Get the cinderella home directory (~/.cinderella)
pub fn cinderella_home() -> std::path::PathBuf {
    dirs_home().join(CINDERELLA_DIR)
}

/// Get the models directory (~/.cinderella/models)
pub fn models_dir() -> std::path::PathBuf {
    cinderella_home().join(MODELS_DIR)
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
}

/// System prompt for the coding agent.
pub const SYSTEM_PROMPT: &str = r#"You are Cinderella, a local AI coding assistant. You help users read, write, and edit code in their projects.

You have access to these tools:
- read_file: Read file contents (with optional line range)
- write_file: Write content to a file (creates parent directories)
- edit_file: Replace exact string matches in a file
- bash: Execute shell commands (120s timeout)
- ls: List directory contents

Guidelines:
- Read files before modifying them
- Use edit_file for targeted changes, write_file for new files or complete rewrites
- Show your reasoning briefly, then act
- When running bash commands, prefer safe, non-destructive operations
- If a command might be dangerous, explain what it does first
- Keep responses concise and focused on the task
"#;

/// Tool definitions for the OpenAI-compatible API.
pub fn tool_definitions() -> Vec<serde_json::Value> {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read the contents of a file. Returns the file content with line numbers.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to read (relative to project directory)"
                        },
                        "start_line": {
                            "type": "integer",
                            "description": "Optional start line (1-indexed)"
                        },
                        "end_line": {
                            "type": "integer",
                            "description": "Optional end line (1-indexed, inclusive)"
                        }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write content to a file. Creates parent directories if needed. Overwrites existing content.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to write (relative to project directory)"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Replace an exact string match in a file. The old_string must appear exactly once in the file.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to edit (relative to project directory)"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "The exact string to find and replace (must be unique in the file)"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "The string to replace it with"
                        }
                    },
                    "required": ["path", "old_string", "new_string"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "bash",
                "description": "Execute a shell command. Times out after 120 seconds.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        }
                    },
                    "required": ["command"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "ls",
                "description": "List directory contents.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the directory to list (relative to project directory). Defaults to current directory."
                        }
                    },
                    "required": []
                }
            }
        }
    ])
    .as_array()
    .unwrap()
    .clone()
}
