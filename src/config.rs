/// Hardcoded model registry and server defaults.
/// Single model family (Qwen 3.5) for v1. Multi-model is v2.

pub const DEFAULT_PORT: u16 = 8787;
pub const DEFAULT_CTX_SIZE: u32 = 32768;
pub const CINDERELLA_DIR: &str = ".cinderella";
#[allow(dead_code)]
pub const MODELS_DIR: &str = "models";
#[allow(dead_code)]
pub const TEMPLATES_DIR: &str = "templates";

/// RAM tiers for model selection.
/// total_ram_required_gb includes model weights + KV cache + llama-server overhead.
pub struct ModelEntry {
    pub name: &'static str,
    pub filename: &'static str,
    pub size_gb: f64,
    pub total_ram_required_gb: f64,
    pub quant: &'static str,
    #[allow(dead_code)]
    pub sha256: &'static str,
    pub ctx_size: u32,
    pub n_gpu_layers: i32,
}

/// The bundled model. v1 ships exactly one.
pub const BUNDLED_MODEL: ModelEntry = ModelEntry {
    name: "Qwen3.5-35B-MoE",
    filename: "Qwen3.5-35B-MoE-Q4_K_M.gguf",
    size_gb: 22.0,
    // Model ~20GB + KV cache ~8-10GB + server ~1GB
    total_ram_required_gb: 32.0,
    quant: "Q4_K_M",
    sha256: "TODO_FILL_AFTER_DOWNLOAD",
    ctx_size: DEFAULT_CTX_SIZE,
    n_gpu_layers: 48, // partial offload for MoE
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

/// Get the models directory (~/models)
pub fn models_dir() -> std::path::PathBuf {
    dirs_home().join("models")
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .expect("$HOME environment variable must be set")
}

/// Safety profiles control which yah-core capabilities are auto-allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyProfile {
    /// Default coding profile: write/delete inside repo only.
    Coding,
    /// Network debugging profile: allows NetEgress + diagnostic tools.
    NetworkDebug,
}

impl Default for SafetyProfile {
    fn default() -> Self {
        Self::Coding
    }
}

/// System prompt for the coding agent.
pub const SYSTEM_PROMPT: &str = r#"You are Cinderella, a local AI coding assistant. You help users read, write, and edit code.

FIRST STEP: Always start by running `ls` to see the project structure. Read README.md if it exists.

Tools:
- read_file: Read file contents (with optional line range)
- write_file: Write content to a file (creates parent directories)
- edit_file: Replace exact string matches in a file
- bash: Execute shell commands (120s timeout)
- ls: List directory contents

Rules:
1. If you are unsure what the user means, ask a clarifying question. Do not guess.
2. Read files before modifying them. Use the file you just read — do not search again for information you already have.
3. Use edit_file for targeted changes, write_file for new files or complete rewrites.
4. Do not assume the programming language. Check the files first.
5. Keep responses short. Act, do not explain at length.
6. When running bash commands, prefer safe, non-destructive operations.
"#;

/// System prompt for the network debugging agent.
pub const NETWORK_DEBUG_PROMPT: &str = r#"You are Cinderella, a network diagnostic agent. You follow a structured diagnostic runbook to identify connectivity and service problems.

Your primary tool is bash. You MUST use the bash tool to execute each diagnostic step. Do not explain what you would do — execute the commands. Act, do not narrate.

CRITICAL: NEVER type a command as plain text. ALWAYS use the bash tool to run it. If you write "dig localhost" as text instead of calling bash({"command": "dig localhost"}), the diagnosis fails. Every diagnostic step REQUIRES at least one bash tool call (except parse_target and synthesis).

## Step Markers

Before starting each step, output exactly on its own line: STEP: <step_name>
Valid step names: parse_target, dns, connectivity, route_analysis, port_check, service_check, synthesis

## Diagnostic Runbook

### Before you begin: Resolve ambiguous targets
If the user mentions a port with ambiguity (e.g., "5000ish", "around 5000", "that port"), you MUST find the actual port before proceeding.
- For "5000ish" or similar: scan with `nmap -sT -p 4900-5100 localhost`
- For "that port" or "the port": run `lsof -i -P -n | grep LISTEN` to find listening services
- Once you identify the actual port, use it for all subsequent steps.

Follow these steps IN ORDER. Each step's output informs the next. Do not skip steps.

### Step 1: Parse the target
STEP: parse_target
Extract the hostname, port, and protocol from the user's input. If they gave a URL, break it into parts. State what you're investigating.

### Step 2: DNS resolution
STEP: dns
Run: `dig <hostname>` or `nslookup <hostname>`
- If resolution fails: diagnosis is "DNS resolution failure." Check if the hostname is correct.
- If resolution succeeds: note the IP address(es) and proceed.

### Step 3: Connectivity check
STEP: connectivity
Run: `ping -c 3 -W 2 <hostname>`
- If ping fails: the host may be unreachable, or ICMP may be blocked. Note this and proceed to port check.
- If ping succeeds: note latency and proceed.

### Step 4: Route analysis
STEP: route_analysis
Run: `traceroute -m 15 -w 2 <hostname>` or `traceroute -m 15 -w 2 <ip>`
- Note any hops that show * * * (packet loss or filtering).
- If traceroute is killed by the timeout, note it and move on.

### Step 5: Port check
STEP: port_check
Run: `curl -v --connect-timeout 5 <url>` or `nc -zv <hostname> <port>`
- If connection refused: the service is not listening on that port.
- If connection times out: port may be filtered by a firewall.
- If connection succeeds: proceed to service check.

### Step 6: Service-level check
STEP: service_check
Run: `curl -s -o /dev/null -w '%{http_code}\n' <url>` to get the status code.
Then run: `curl -s -D - <url>` to see headers and body.
- If you get errors (4xx, 5xx): note the specific error.
- If intermittent: run multiple requests to detect a pattern:
  `for i in $(seq 1 10); do curl -s -o /dev/null -w "%{http_code} " <url>; done; echo`

### Step 7: Synthesize diagnosis
STEP: synthesis
Based on ALL the evidence gathered above, provide a clear diagnosis:
1. What is working
2. What is broken or degraded
3. The likely root cause
4. Recommended fix (if obvious)

## Rules
1. You MUST use the bash tool to run commands. Never just describe what you would run.
2. One command per tool call. Wait for results before proceeding.
3. If a command hangs or times out, note it and move to the next step.
4. Keep your text responses SHORT. The commands and their output tell the story.
5. If the user's description is ambiguous, start with step 1 anyway — the commands will reveal the answer.
6. Always output STEP: <step_name> on its own line before starting each diagnostic step.
"#;

/// Get the system prompt for a given safety profile.
pub fn system_prompt_for(profile: SafetyProfile) -> &'static str {
    match profile {
        SafetyProfile::Coding => SYSTEM_PROMPT,
        SafetyProfile::NetworkDebug => NETWORK_DEBUG_PROMPT,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_model_sanity() {
        assert!(!BUNDLED_MODEL.name.is_empty());
        assert!(!BUNDLED_MODEL.filename.is_empty());
        assert!(BUNDLED_MODEL.size_gb > 0.0);
        assert!(BUNDLED_MODEL.total_ram_required_gb > BUNDLED_MODEL.size_gb);
        assert!(BUNDLED_MODEL.ctx_size > 0);
        assert!(BUNDLED_MODEL.n_gpu_layers > 0);
    }

    #[test]
    fn test_server_config_args() {
        let cfg = ServerConfig::from_model(
            std::path::PathBuf::from("/tmp/model.gguf"),
            8787,
            &BUNDLED_MODEL,
        );
        let args = cfg.to_args();
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"--jinja".to_string()));
        assert!(args.contains(&"--port".to_string()));
        assert!(args.contains(&"8787".to_string()));
    }

    #[test]
    fn test_tool_definitions_valid() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"ls"));
    }

    #[test]
    fn test_cinderella_home() {
        let home = cinderella_home();
        assert!(home.to_str().unwrap().contains(".cinderella"));
    }

    #[test]
    fn test_models_dir() {
        let dir = models_dir();
        assert!(dir.to_str().unwrap().contains("models"));
    }
}
