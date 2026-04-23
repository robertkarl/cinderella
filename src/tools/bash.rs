use super::ToolResult;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const BASH_TIMEOUT_SECS: u64 = 120;
const MAX_OUTPUT_LINES: usize = 50;

pub async fn execute(args: &Value, project_dir: &Path) -> ToolResult {
    let command = match args.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                output: "Missing required parameter: command".to_string(),
                success: false,
            }
        }
    };

    // TODO: yah-core integration for command classification
    // For now, execute directly

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
            // Timeout — process group kill handled by tokio's drop
            ToolResult {
                output: format!("Command timed out after {}s", BASH_TIMEOUT_SECS),
                success: false,
            }
        }
    }
}

async fn run_command(command: &str, working_dir: &Path) -> anyhow::Result<ToolResult> {
    let output = Command::new("bash")
        .args(["-c", command])
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Create new process group so we can kill the whole tree
        .process_group(0)
        .output()
        .await?;

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
