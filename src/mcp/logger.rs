use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

/// One line in mcp-activity.jsonl.
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct ActivityEntry {
    pub ts: String,
    pub tool: String,
    pub detail: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub context_tokens: u64,
    pub latency_ms: u64,
    pub estimated_cloud_cost_usd: f64,
    pub cache_hit: bool,
    pub model: String,
}

/// Opus uncached input pricing: $15 per million tokens.
const OPUS_UNCACHED_PER_TOKEN: f64 = 15.0 / 1_000_000.0;
/// Opus cached input pricing: $1.875 per million tokens.
const OPUS_CACHED_PER_TOKEN: f64 = 1.875 / 1_000_000.0;
/// Opus output pricing: $75 per million tokens.
const OPUS_OUTPUT_PER_TOKEN: f64 = 75.0 / 1_000_000.0;
/// Anthropic prompt cache TTL: 5 minutes.
const CACHE_TTL_SECS: u64 = 300;

pub struct ActivityLogger {
    path: PathBuf,
    pub last_log_time: Mutex<Option<Instant>>,
}

impl ActivityLogger {
    pub fn new() -> Self {
        let home = std::env::var("HOME").expect("$HOME must be set");
        let path = PathBuf::from(home)
            .join("Library/Application Support/Glass Slipper/mcp-activity.jsonl");
        Self { path, last_log_time: Mutex::new(None) }
    }

    /// Create a logger writing to a custom path (for testing).
    pub fn with_path(path: PathBuf) -> Self {
        Self { path, last_log_time: Mutex::new(None) }
    }

    /// Log a tool call. Creates parent directories if needed.
    pub fn log(&self, tool: &str, detail: &str, input_tokens: u64, output_tokens: u64, start: Instant, model: &str, context_tokens: u64) {
        let latency_ms = start.elapsed().as_millis() as u64;

        let cache_hit = {
            let mut last = self.last_log_time.lock().unwrap();
            let hit = match *last {
                Some(prev) => prev.elapsed().as_secs() < CACHE_TTL_SECS,
                None => false,
            };
            *last = Some(Instant::now());
            hit
        };

        let input_rate = if cache_hit { OPUS_CACHED_PER_TOKEN } else { OPUS_UNCACHED_PER_TOKEN };
        let estimated_cost = context_tokens as f64 * input_rate
            + output_tokens as f64 * OPUS_OUTPUT_PER_TOKEN;

        let entry = ActivityEntry {
            ts: now_unix_secs(),
            tool: tool.to_string(),
            detail: detail.to_string(),
            input_tokens,
            output_tokens,
            context_tokens,
            latency_ms,
            estimated_cloud_cost_usd: (estimated_cost * 1000.0).round() / 1000.0,
            cache_hit,
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
        logger.log("local_summarize", "cargo build", 3200, 18, start, "qwen3.5-9b-q5_k_m", 30000);
        logger.log("local_explain", "server.rs:swap_model", 500, 120, start, "qwen3.5-9b-q5_k_m", 30000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        let entry: ActivityEntry = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry.tool, "local_summarize");
        assert_eq!(entry.input_tokens, 3200);
        assert_eq!(entry.output_tokens, 18);
        assert_eq!(entry.context_tokens, 30000);
        assert!(entry.estimated_cloud_cost_usd > 0.0);
    }

    #[test]
    fn test_cost_uses_context_tokens_not_input_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "test", 100, 0, start, "qwen", 50000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let entry: ActivityEntry = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert!((entry.estimated_cloud_cost_usd - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_first_call_is_always_uncached() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "test", 100, 50, start, "qwen", 10000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let entry: ActivityEntry = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert!(!entry.cache_hit);
        assert_eq!(entry.context_tokens, 10000);
    }

    #[test]
    fn test_second_call_within_ttl_is_cached() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "first", 100, 50, start, "qwen", 10000);
        logger.log("local_summarize", "second", 100, 50, start, "qwen", 10000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        let entry1: ActivityEntry = serde_json::from_str(lines[0]).unwrap();
        let entry2: ActivityEntry = serde_json::from_str(lines[1]).unwrap();
        assert!(!entry1.cache_hit);
        assert!(entry2.cache_hit);
    }

    #[test]
    fn test_uncached_cost_uses_full_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "test", 100, 50, start, "qwen", 10000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let entry: ActivityEntry = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert!((entry.estimated_cloud_cost_usd - 0.154).abs() < 0.001);
    }

    #[test]
    fn test_cached_cost_uses_discounted_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "first", 100, 50, start, "qwen", 10000);
        logger.log("local_summarize", "second", 100, 50, start, "qwen", 10000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        let entry: ActivityEntry = serde_json::from_str(lines[1]).unwrap();
        assert!((entry.estimated_cloud_cost_usd - 0.023).abs() < 0.001);
    }

    #[test]
    fn test_output_tokens_always_priced_at_75_per_mil() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "test", 100, 1000, start, "qwen", 0);

        let contents = std::fs::read_to_string(&path).unwrap();
        let entry: ActivityEntry = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert!((entry.estimated_cloud_cost_usd - 0.075).abs() < 0.001);
    }

    #[test]
    fn test_context_tokens_and_cache_hit_roundtrip_through_serde() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "test", 3200, 18, start, "qwen", 45000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let entry: ActivityEntry = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert_eq!(entry.context_tokens, 45000);
        assert!(!entry.cache_hit);

        let line = contents.lines().next().unwrap();
        assert!(line.contains("\"cache_hit\""));
        assert!(line.contains("\"context_tokens\""));
    }

    #[test]
    fn test_cache_expires_after_ttl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "first", 100, 50, start, "qwen", 10000);

        {
            let mut last = logger.last_log_time.lock().unwrap();
            *last = Some(Instant::now() - std::time::Duration::from_secs(360));
        }

        logger.log("local_summarize", "after-expiry", 100, 50, start, "qwen", 10000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        let entry: ActivityEntry = serde_json::from_str(lines[1]).unwrap();
        assert!(!entry.cache_hit);
        assert!((entry.estimated_cloud_cost_usd - 0.154).abs() < 0.001);
    }

    #[test]
    fn test_large_context_uncached_cost() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "big", 100, 512, start, "qwen", 200_000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let entry: ActivityEntry = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert!((entry.estimated_cloud_cost_usd - 3.038).abs() < 0.001);
    }

    #[test]
    fn test_large_context_cached_cost() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let logger = ActivityLogger::with_path(path.clone());

        let start = Instant::now();
        logger.log("local_summarize", "first", 100, 50, start, "qwen", 200_000);
        logger.log("local_summarize", "second", 100, 512, start, "qwen", 200_000);

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        let entry: ActivityEntry = serde_json::from_str(lines[1]).unwrap();
        assert!((entry.estimated_cloud_cost_usd - 0.413).abs() < 0.001);
    }
}
