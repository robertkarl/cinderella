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

    let lines: Vec<&str> = content.lines().collect();
    let start = args
        .get("start_line")
        .and_then(|v| v.as_u64())
        .map(|v| v.saturating_sub(1) as usize)
        .unwrap_or(0);
    let end = args
        .get("end_line")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(lines.len());

    let end = end.min(lines.len());
    let start = start.min(end);

    let numbered: Vec<String> = lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>4}: {}", start + i + 1, line))
        .collect();

    ToolResult {
        output: numbered.join("\n"),
        success: true,
    }
}
