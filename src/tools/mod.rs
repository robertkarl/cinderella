/// Tool trait, router, and shared types.

pub mod bash;
pub mod edit;
pub mod ls;
pub mod read;
pub mod write;

use anyhow::Result;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Result of executing a tool.
pub struct ToolResult {
    pub output: String,
    pub success: bool,
}

/// Execute a tool call by name with the given arguments.
pub async fn execute(
    name: &str,
    args: &Value,
    project_dir: &Path,
) -> ToolResult {
    match name {
        "read_file" => read::execute(args, project_dir),
        "write_file" => write::execute(args, project_dir),
        "edit_file" => edit::execute(args, project_dir),
        "bash" => bash::execute(args, project_dir).await,
        "ls" => ls::execute(args, project_dir),
        _ => ToolResult {
            output: format!("Unknown tool: {}", name),
            success: false,
        },
    }
}

/// Resolve a path argument relative to the project directory.
/// Prevents path traversal above the project root.
pub fn resolve_path(path_str: &str, project_dir: &Path) -> Result<PathBuf> {
    let path = Path::new(path_str);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    };

    // Canonicalize what exists, then check prefix
    // For new files, check the parent
    let check_path = if resolved.exists() {
        resolved.canonicalize()?
    } else if let Some(parent) = resolved.parent() {
        if parent.exists() {
            let canonical_parent = parent.canonicalize()?;
            canonical_parent.join(resolved.file_name().unwrap_or_default())
        } else {
            resolved.clone()
        }
    } else {
        resolved.clone()
    };

    let canonical_project = project_dir.canonicalize()?;
    if !check_path.starts_with(&canonical_project) {
        anyhow::bail!(
            "Path {} is outside project directory",
            path_str
        );
    }

    Ok(resolved)
}

/// Truncate output for context management.
/// Keep first `head` lines and last `tail` lines, omit middle.
pub fn truncate_output(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_lines {
        return output.to_string();
    }

    let head = max_lines / 2;
    let tail = max_lines - head;
    let omitted = lines.len() - head - tail;

    let mut result = lines[..head].join("\n");
    result.push_str(&format!("\n...({} lines omitted)\n", omitted));
    result.push_str(&lines[lines.len() - tail..].join("\n"));
    result
}
