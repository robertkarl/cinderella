use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

/// One line in mcp-activity.jsonl.
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct ActivityEntry {
    pub ts: String,
    pub tool: String,
    pub detail: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub latency_ms: u64,
    pub estimated_cloud_cost_usd: f64,
    pub model: String,
}

/// Opus input pricing: $15 per million tokens.
const OPUS_INPUT_PRICE_PER_TOKEN: f64 = 15.0 / 1_000_000.0;

pub struct ActivityLogger {
    path: PathBuf,
}

impl ActivityLogger {
    pub fn new() -> Self {
        let home = std::env::var("HOME").expect("$HOME must be set");
        let path = PathBuf::from(home)
            .join("Library/Application Support/Glass Slipper/mcp-activity.jsonl");
        Self { path }
    }

    /// Create a logger writing to a custom path (for testing).
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Log a tool call. Creates parent directories if needed.
    pub fn log(&self, tool: &str, detail: &str, input_tokens: u64, output_tokens: u64, start: Instant, model: &str) {
        let latency_ms = start.elapsed().as_millis() as u64;
        let estimated_cost = input_tokens as f64 * OPUS_INPUT_PRICE_PER_TOKEN;

        let entry = ActivityEntry {
            ts: now_unix_secs(),
            tool: tool.to_string(),
            detail: detail.to_string(),
            input_tokens,
            output_tokens,
            latency_ms,
            estimated_cloud_cost_usd: (estimated_cost * 1000.0).round() / 1000.0,
            model: model.to_string(),
        };

        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(line) = serde_json::to_string(&entry) {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.path) {
                let _ = writeln!(file, "{}", line);
            }
        }
    }
}

fn now_unix_secs() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_log_creates_file_and_appends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-activity.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "cargo build", 3200, 18, start, "qwen3.5-9b-q5_k_m");
        logger.log("local_explain", "server.rs:swap_model", 500, 120, start, "qwen3.5-9b-q5_k_m");

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        let entry: ActivityEntry = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry.tool, "local_summarize");
        assert_eq!(entry.input_tokens, 3200);
        assert_eq!(entry.output_tokens, 18);
        assert!(entry.estimated_cloud_cost_usd > 0.0);
    }

    #[test]
    fn test_cost_calculation() {
        let cost = 3200.0 * OPUS_INPUT_PRICE_PER_TOKEN;
        assert!((cost - 0.048).abs() < 0.001);
    }
}
