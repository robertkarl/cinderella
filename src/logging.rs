/// Structured JSON-lines logging for the Glass Slipper engine.
///
/// Writes to `~/Library/Logs/Glass Slipper/glass-slipper-engine.log`.

use anyhow::Result;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

// ── LogLevel ─────────────────────────────────────────────────────────

/// Severity level for log entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

// ── LogEntry ─────────────────────────────────────────────────────────

/// A single structured log entry, serialized as one JSON line.
#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: f64,
    pub source: String,
    pub level: LogLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ── EngineLogger ─────────────────────────────────────────────────────

/// Logger that appends JSON lines to a file.
struct EngineLogger {
    file: File,
}

static LOGGER: OnceLock<Mutex<EngineLogger>> = OnceLock::new();

/// Returns the default log directory: `~/Library/Logs/Glass Slipper`.
pub fn log_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Library")
        .join("Logs")
        .join("Glass Slipper")
}

/// Initialize the global logger. Creates the directory if needed and opens
/// the log file in append mode. Safe to call multiple times (only the first
/// call takes effect).
pub fn init(log_dir: &Path) -> Result<()> {
    fs::create_dir_all(log_dir)?;
    let log_path = log_dir.join("glass-slipper-engine.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    // OnceLock::set returns Err if already initialized — that's fine.
    let _ = LOGGER.set(Mutex::new(EngineLogger { file }));
    Ok(())
}

/// Current time as fractional epoch seconds.
fn now_epoch_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// Write one JSON-line log entry. Silently drops the entry on any error
/// (logging must never crash the host).
pub fn log(
    source: &str,
    level: LogLevel,
    message: &str,
    data: Option<serde_json::Value>,
) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    let Ok(mut guard) = logger.lock() else {
        return;
    };

    let entry = LogEntry {
        timestamp: now_epoch_secs(),
        source: source.to_string(),
        level,
        message: message.to_string(),
        data,
    };

    if let Ok(json) = serde_json::to_string(&entry) {
        let _ = writeln!(guard.file, "{}", json);
    }
}

/// Log at Info level.
pub fn info(source: &str, message: &str, data: Option<serde_json::Value>) {
    log(source, LogLevel::Info, message, data);
}

/// Log at Warn level.
pub fn warn(source: &str, message: &str, data: Option<serde_json::Value>) {
    log(source, LogLevel::Warn, message, data);
}

/// Log at Error level.
pub fn error(source: &str, message: &str, data: Option<serde_json::Value>) {
    log(source, LogLevel::Error, message, data);
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_level_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&LogLevel::Info).unwrap(), "\"info\"");
        assert_eq!(serde_json::to_string(&LogLevel::Warn).unwrap(), "\"warn\"");
        assert_eq!(serde_json::to_string(&LogLevel::Error).unwrap(), "\"error\"");
    }

    #[test]
    fn log_entry_serializes_to_json() {
        let entry = LogEntry {
            timestamp: 1715100000.123,
            source: "test".to_string(),
            level: LogLevel::Info,
            message: "hello world".to_string(),
            data: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["timestamp"], 1715100000.123);
        assert_eq!(parsed["source"], "test");
        assert_eq!(parsed["level"], "info");
        assert_eq!(parsed["message"], "hello world");
        // data should be absent (skip_serializing_if)
        assert!(parsed.get("data").is_none());
    }

    #[test]
    fn log_entry_with_data() {
        let entry = LogEntry {
            timestamp: 1715100000.0,
            source: "monitor".to_string(),
            level: LogLevel::Warn,
            message: "memory pressure".to_string(),
            data: Some(serde_json::json!({"swap_mb": 1024})),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["level"], "warn");
        assert_eq!(parsed["data"]["swap_mb"], 1024);
    }

    #[test]
    fn log_dir_ends_with_glass_slipper() {
        let dir = log_dir();
        assert!(dir.ends_with("Glass Slipper"));
    }
}
