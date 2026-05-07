use super::{resolve_path, ToolResult};
use serde_json::Value;
use std::fs;
use std::path::Path;

fn ensure_parent_dirs(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

pub fn execute(args: &Value, project_dir: &Path) -> ToolResult {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing required parameter: path".to_string(),
                success: false,
            }
        }
    };

    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                output: "Missing required parameter: content".to_string(),
                success: false,
            }
        }
    };

    let path = match resolve_path(path_str, project_dir) {
        Ok(p) => p,
        Err(e) => {
            return ToolResult {
                output: format!("Error: {}", e),
                success: false,
            }
        }
    };

    // Create parent directories if needed
    if let Err(e) = ensure_parent_dirs(&path) {
        return ToolResult {
            output: format!("Error creating directories: {}", e),
            success: false,
        };
    }

    match fs::write(&path, content) {
        Ok(()) => {
            let line_count = content.lines().count();
            ToolResult {
                output: format!("Wrote {} lines to {}", line_count, path_str),
                success: true,
            }
        }
        Err(e) => ToolResult {
            output: format!("Error writing {}: {}", path_str, e),
            success: false,
        },
    }
}
