/// Tool trait, router, and shared types.

pub mod bash;
pub mod edit;
pub mod ls;
pub mod read;
pub mod write;

use crate::config::SafetyProfile;
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
    profile: SafetyProfile,
    skip_permissions: bool,
) -> ToolResult {
    match name {
        "read_file" => read::execute(args, project_dir),
        "write_file" => write::execute(args, project_dir),
        "edit_file" => edit::execute(args, project_dir),
        "bash" => bash::execute(args, project_dir, profile, skip_permissions).await,
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

    let check_path = if resolved.exists() {
        resolved.canonicalize()?
    } else {
        canonicalize_with_ancestors(&resolved)?
    };

    let canonical_project = project_dir.canonicalize()?;
    if !check_path.starts_with(&canonical_project) {
        anyhow::bail!("Path {} is outside project directory", path_str);
    }

    Ok(check_path)
}

/// Canonicalize a non-existent path by walking up to the nearest existing ancestor,
/// canonicalizing it, and reappending the remaining components.
fn canonicalize_with_ancestors(resolved: &Path) -> Result<PathBuf> {
    let (ancestor, suffix_parts) = find_existing_ancestor(resolved);
    if !ancestor.exists() {
        return Ok(resolved.to_path_buf());
    }
    let mut canonical = ancestor.canonicalize()?;
    for part in suffix_parts.iter().rev() {
        canonical = canonical.join(part);
    }
    Ok(canonical)
}

/// Walk up the path tree to find the nearest existing ancestor.
/// Returns (ancestor, suffix_parts) where suffix_parts are the
/// components between ancestor and the original path, in reverse order.
fn find_existing_ancestor(path: &Path) -> (PathBuf, Vec<std::ffi::OsString>) {
    let mut ancestor = path.to_path_buf();
    let mut suffix_parts: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if ancestor.exists() {
            break;
        }
        if let Some(name) = ancestor.file_name() {
            suffix_parts.push(name.to_os_string());
        }
        match ancestor.parent() {
            Some(p) if p != ancestor => ancestor = p.to_path_buf(),
            _ => break,
        }
    }
    (ancestor, suffix_parts)
}

/// Truncate output for context management.
/// Keep first `head` lines and last `tail` lines, omit middle.
#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "line 1\nline 2\nline 3\n").unwrap();
        fs::create_dir_all(dir.path().join("subdir")).unwrap();
        fs::write(dir.path().join("subdir/nested.txt"), "nested content").unwrap();
        dir
    }

    #[test]
    fn test_truncate_output_short() {
        let output = "line1\nline2\nline3";
        assert_eq!(truncate_output(output, 10), output);
    }

    #[test]
    fn test_truncate_output_long() {
        let lines: Vec<String> = (1..=20).map(|i| format!("line {}", i)).collect();
        let output = lines.join("\n");
        let truncated = truncate_output(&output, 6);
        assert!(truncated.contains("line 1"));
        assert!(truncated.contains("line 20"));
        assert!(truncated.contains("omitted"));
    }

    #[test]
    fn test_resolve_path_relative() {
        let dir = setup_test_dir();
        let result = resolve_path("test.txt", dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("test.txt"));
    }

    #[test]
    fn test_resolve_path_traversal_blocked() {
        let dir = setup_test_dir();
        let result = resolve_path("../../etc/passwd", dir.path());
        assert!(result.is_err());
    }

    use crate::config::SafetyProfile;

    #[tokio::test]
    async fn test_read_tool() {
        let dir = setup_test_dir();
        let args = serde_json::json!({"path": "test.txt"});
        let result = execute("read_file", &args, dir.path(), SafetyProfile::Coding, false).await;
        assert!(result.success);
        assert!(result.output.contains("line 1"));
        assert!(result.output.contains("line 2"));
    }

    #[tokio::test]
    async fn test_write_tool() {
        let dir = setup_test_dir();
        let args = serde_json::json!({"path": "new.txt", "content": "hello world"});
        let result = execute("write_file", &args, dir.path(), SafetyProfile::Coding, false).await;
        assert!(result.success);
        assert!(dir.path().join("new.txt").exists());
        assert_eq!(fs::read_to_string(dir.path().join("new.txt")).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_write_tool_creates_dirs() {
        let dir = setup_test_dir();
        let args = serde_json::json!({"path": "a/b/c.txt", "content": "deep"});
        let result = execute("write_file", &args, dir.path(), SafetyProfile::Coding, false).await;
        assert!(result.success);
        assert_eq!(fs::read_to_string(dir.path().join("a/b/c.txt")).unwrap(), "deep");
    }

    #[tokio::test]
    async fn test_edit_tool() {
        let dir = setup_test_dir();
        let args = serde_json::json!({
            "path": "test.txt",
            "old_string": "line 2",
            "new_string": "modified line 2"
        });
        let result = execute("edit_file", &args, dir.path(), SafetyProfile::Coding, false).await;
        assert!(result.success);
        let content = fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert!(content.contains("modified line 2"));
        assert!(!content.contains("\nline 2\n"));
    }

    #[tokio::test]
    async fn test_edit_tool_not_found() {
        let dir = setup_test_dir();
        let args = serde_json::json!({
            "path": "test.txt",
            "old_string": "nonexistent string",
            "new_string": "replacement"
        });
        let result = execute("edit_file", &args, dir.path(), SafetyProfile::Coding, false).await;
        assert!(!result.success);
        assert!(result.output.contains("not found"));
    }

    #[tokio::test]
    async fn test_ls_tool() {
        let dir = setup_test_dir();
        let args = serde_json::json!({"path": "."});
        let result = execute("ls", &args, dir.path(), SafetyProfile::Coding, false).await;
        assert!(result.success);
        assert!(result.output.contains("test.txt"));
        assert!(result.output.contains("subdir/"));
    }

    #[tokio::test]
    async fn test_bash_tool() {
        let dir = setup_test_dir();
        let args = serde_json::json!({"command": "echo hello"});
        let result = execute("bash", &args, dir.path(), SafetyProfile::Coding, false).await;
        assert!(result.success);
        assert!(result.output.contains("hello"));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let dir = setup_test_dir();
        let args = serde_json::json!({});
        let result = execute("nonexistent", &args, dir.path(), SafetyProfile::Coding, false).await;
        assert!(!result.success);
        assert!(result.output.contains("Unknown tool"));
    }
}
