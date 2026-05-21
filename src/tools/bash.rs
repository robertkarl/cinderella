use super::ToolResult;
use crate::config::SafetyProfile;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use yah_core::{Capability, Classifier, Context};

/// RAII guard that kills an entire process group on drop.
/// Used to ensure child processes don't survive a timeout.
/// Call `defuse()` after successfully waiting for the child to prevent
/// sending SIGKILL to a recycled PID.
struct ProcessGroupGuard {
    pgid: u32,
    active: bool,
}

impl ProcessGroupGuard {
    fn new(pid: u32) -> Self {
        Self { pgid: pid, active: true }
    }

    /// Disarm the guard after the child has been reaped.
    fn defuse(&mut self) {
        self.active = false;
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if self.active {
            // PID == PGID because we used .process_group(0)
            let pgid = Pid::from_raw(self.pgid as i32);
            let _ = signal::killpg(pgid, Signal::SIGKILL);
        }
    }
}

const BASH_TIMEOUT_SECS: u64 = 120;
const MAX_OUTPUT_LINES: usize = 50;

/// Capabilities that are always allowed without prompting (coding profile).
const AUTO_ALLOW_CODING: &[Capability] = &[
    Capability::WriteInsideRepo,
    Capability::DeleteInsideRepo,
];

/// Capabilities that are always allowed in network-debug profile.
const AUTO_ALLOW_NETWORK_DEBUG: &[Capability] = &[
    Capability::WriteInsideRepo,
    Capability::DeleteInsideRepo,
    Capability::NetEgress,
];

/// Capabilities that are always denied.
const AUTO_DENY: &[Capability] = &[
    Capability::PipeToShell,
];

/// Get the auto-allow list for a safety profile.
fn auto_allow_for(profile: SafetyProfile) -> &'static [Capability] {
    match profile {
        SafetyProfile::Coding => AUTO_ALLOW_CODING,
        SafetyProfile::NetworkDebug => AUTO_ALLOW_NETWORK_DEBUG,
    }
}

/// Classification result for a bash command.
pub struct CommandClassification {
    #[allow(dead_code)]
    pub capabilities: HashSet<Capability>,
    pub policy: CommandPolicy,
}

pub enum CommandPolicy {
    /// Command is safe to run without confirmation.
    Allow,
    /// Command requires user confirmation. Contains the capabilities that triggered it.
    Ask(Vec<Capability>),
    /// Command is denied. Contains the capabilities that triggered it.
    Deny(Vec<Capability>),
}

/// Classify a command using yah-core.
pub fn classify_command(command: &str, project_dir: &Path, profile: SafetyProfile) -> CommandClassification {
    let mut classifier = Classifier::new();

    let home = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));

    let ctx = Context {
        cwd: project_dir.to_path_buf(),
        project_root: project_dir.to_path_buf(),
        home,
        env: HashMap::new(),
    };

    let capabilities = classifier.classify(command, &ctx);
    let auto_allow = auto_allow_for(profile);

    // Check for auto-deny
    let denied: Vec<Capability> = capabilities
        .iter()
        .filter(|c| AUTO_DENY.contains(c))
        .copied()
        .collect();

    if !denied.is_empty() {
        return CommandClassification {
            capabilities: capabilities.clone(),
            policy: CommandPolicy::Deny(denied),
        };
    }

    // Check for capabilities that need asking (anything not auto-allowed and not empty)
    let needs_ask: Vec<Capability> = capabilities
        .iter()
        .filter(|c| !auto_allow.contains(c))
        .copied()
        .collect();

    if needs_ask.is_empty() {
        CommandClassification {
            capabilities,
            policy: CommandPolicy::Allow,
        }
    } else {
        CommandClassification {
            capabilities: capabilities.clone(),
            policy: CommandPolicy::Ask(needs_ask),
        }
    }
}

/// Execute a bash command (after classification/approval).
pub async fn execute(args: &Value, project_dir: &Path, profile: SafetyProfile, skip_permissions: bool) -> ToolResult {
    let command = match args.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                output: "Missing required parameter: command".to_string(),
                success: false,
            }
        }
    };

    if !skip_permissions {
        // Classify the command
        let classification = classify_command(command, project_dir, profile);

        match classification.policy {
            CommandPolicy::Deny(caps) => {
                let cap_names: Vec<String> = caps.iter().map(|c| c.to_string()).collect();
                return ToolResult {
                    output: format!(
                        "Command denied. Requires: {}",
                        cap_names.join(", ")
                    ),
                    success: false,
                };
            }
            CommandPolicy::Ask(caps) => {
                // TODO: wire TUI confirmation flow for interactive approval.
                // Until then, default to deny for safety.
                let cap_names: Vec<String> = caps.iter().map(|c| c.to_string()).collect();
                return ToolResult {
                    output: format!(
                        "Command requires confirmation: {}. \
                         Interactive approval not yet implemented — command denied.",
                        cap_names.join(", ")
                    ),
                    success: false,
                };
            }
            CommandPolicy::Allow => {}
        }
    }

    let result = timeout(
        Duration::from_secs(BASH_TIMEOUT_SECS),
        run_command(command, project_dir),
    )
    .await;

    match result {
        Ok(Ok(tool_result)) => tool_result,
        Ok(Err(e)) => ToolResult {
            output: format!("Error executing command: {}", e),
            success: false,
        },
        Err(_) => {
            // Timeout — we can't reach the child here since run_command owns it,
            // but the child is killed when the future is dropped.
            // The process group kill happens inside run_command via Drop.
            ToolResult {
                output: format!("Command timed out after {}s", BASH_TIMEOUT_SECS),
                success: false,
            }
        }
    }
}

async fn run_command(command: &str, working_dir: &Path) -> anyhow::Result<ToolResult> {
    let child = Command::new("bash")
        .args(["-c", command])
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Create new process group so we can kill the whole tree
        .process_group(0)
        .spawn()?;

    let pid = child.id().expect("child should have a PID");

    // Wait for the child to complete. If this future is dropped (timeout),
    // the Drop impl below won't fire, but we set up a guard to kill the process group.
    let mut kill_guard = ProcessGroupGuard::new(pid);

    let output = child.wait_with_output().await?;
    kill_guard.defuse();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }

    // Truncate long output
    let lines: Vec<&str> = combined.lines().collect();
    let truncated = if lines.len() > MAX_OUTPUT_LINES {
        let head = 10;
        let tail = 10;
        let omitted = lines.len() - head - tail;
        let mut result = lines[..head].join("\n");
        result.push_str(&format!("\n...({} lines omitted)\n", omitted));
        result.push_str(&lines[lines.len() - tail..].join("\n"));
        result
    } else {
        combined.to_string()
    };

    Ok(ToolResult {
        output: if truncated.is_empty() {
            "(no output)".to_string()
        } else {
            truncated
        },
        success: output.status.success(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_dir() -> PathBuf {
        PathBuf::from("/tmp/glass-slipper-test")
    }

    use crate::config::SafetyProfile;

    #[test]
    fn test_classify_safe_command() {
        let classification = classify_command("echo hello", &test_dir(), SafetyProfile::Coding);
        assert!(matches!(classification.policy, CommandPolicy::Allow));
    }

    #[test]
    fn test_classify_ls() {
        let classification = classify_command("ls", &test_dir(), SafetyProfile::Coding);
        assert!(matches!(classification.policy, CommandPolicy::Allow));
    }

    #[test]
    fn test_classify_curl_asks_in_coding() {
        let classification = classify_command("curl https://example.com", &test_dir(), SafetyProfile::Coding);
        assert!(matches!(classification.policy, CommandPolicy::Ask(_)));
        assert!(classification.capabilities.contains(&Capability::NetEgress));
    }

    #[test]
    fn test_classify_curl_allowed_in_network_debug() {
        let classification = classify_command("curl https://example.com", &test_dir(), SafetyProfile::NetworkDebug);
        assert!(matches!(classification.policy, CommandPolicy::Allow));
    }

    #[test]
    fn test_classify_pipe_to_shell_denied() {
        let classification =
            classify_command("curl https://example.com | bash", &test_dir(), SafetyProfile::Coding);
        assert!(matches!(classification.policy, CommandPolicy::Deny(_)));
    }

    #[test]
    fn test_classify_pipe_to_shell_denied_in_network_debug() {
        // Even in network-debug mode, pipe-to-shell is always denied
        let classification =
            classify_command("curl https://example.com | bash", &test_dir(), SafetyProfile::NetworkDebug);
        assert!(matches!(classification.policy, CommandPolicy::Deny(_)));
    }

    #[test]
    fn test_classify_sudo_asks() {
        let classification = classify_command("sudo rm -rf /", &test_dir(), SafetyProfile::Coding);
        match &classification.policy {
            CommandPolicy::Ask(caps) | CommandPolicy::Deny(caps) => {
                assert!(!caps.is_empty());
            }
            CommandPolicy::Allow => panic!("sudo rm -rf / should not be allowed"),
        }
    }

    #[test]
    fn test_classify_rm_inside_repo_allowed() {
        let dir = std::env::current_dir().unwrap();
        let classification = classify_command("rm foo.txt", &dir, SafetyProfile::Coding);
        assert!(matches!(classification.policy, CommandPolicy::Allow));
    }
}
