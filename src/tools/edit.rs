use super::{resolve_path, ToolResult};
use serde_json::Value;
use std::fs;
use std::path::Path;

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

    let old_string = match args.get("old_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                output: "Missing required parameter: old_string".to_string(),
                success: false,
            }
        }
    };

    let new_string = match args.get("new_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                output: "Missing required parameter: new_string".to_string(),
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

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                output: format!("Error reading {}: {}", path_str, e),
                success: false,
            }
        }
    };

    // Exact-match guard: old_string must appear exactly once
    let matches: Vec<_> = content.match_indices(old_string).collect();
    match matches.len() {
        0 => ToolResult {
            output: format!(
                "Error: old_string not found in {}. Make sure it matches exactly.",
                path_str
            ),
            success: false,
        },
        1 => {
            let new_content = content.replacen(old_string, new_string, 1);
            match fs::write(&path, &new_content) {
                Ok(()) => {
                    // Find the line number of the replacement
                    let line_num = content[..matches[0].0].lines().count() + 1;
                    let new_lines = new_string.lines().count();
                    ToolResult {
                        output: format!(
                            "Replaced {} lines at line {} in {}",
                            new_lines, line_num, path_str
                        ),
                        success: true,
                    }
                }
                Err(e) => ToolResult {
                    output: format!("Error writing {}: {}", path_str, e),
                    success: false,
                },
            }
        }
        n => ToolResult {
            output: format!(
                "Error: old_string found {} times in {}. It must be unique. Add more context to make it unique.",
                n, path_str
            ),
            success: false,
        },
    }
}
