use super::{resolve_path, ToolResult};
use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn execute(args: &Value, project_dir: &Path) -> ToolResult {
    let path_str = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let path = match resolve_path(path_str, project_dir) {
        Ok(p) => p,
        Err(e) => {
            return ToolResult {
                output: format!("Error: {}", e),
                success: false,
            }
        }
    };

    if !path.exists() {
        return ToolResult {
            output: format!("Directory not found: {}", path_str),
            success: false,
        };
    }

    if !path.is_dir() {
        return ToolResult {
            output: format!("{} is not a directory", path_str),
            success: false,
        };
    }

    let mut entries: Vec<String> = Vec::new();

    match fs::read_dir(&path) {
        Ok(dir) => {
            let mut items: Vec<_> = dir
                .filter_map(|e| e.ok())
                .collect();
            items.sort_by_key(|e| e.file_name());

            for entry in items {
                let name = entry.file_name().to_string_lossy().to_string();
                // Use symlink_metadata to detect symlinks (file_type() follows them)
                let suffix = match entry.path().symlink_metadata() {
                    Ok(meta) if meta.file_type().is_symlink() => "@",
                    Ok(meta) if meta.is_dir() => "/",
                    _ => "",
                };
                entries.push(format!("{}{}", name, suffix));
            }
        }
        Err(e) => {
            return ToolResult {
                output: format!("Error reading directory {}: {}", path_str, e),
                success: false,
            }
        }
    }

    ToolResult {
        output: if entries.is_empty() {
            "(empty directory)".to_string()
        } else {
            entries.join("\n")
        },
        success: true,
    }
}
