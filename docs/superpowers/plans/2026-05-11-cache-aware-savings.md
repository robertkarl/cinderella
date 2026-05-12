# Cache-Aware Savings Accounting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix savings accounting to use cache-aware turn-level costing so the companion window shows realistic savings numbers.

**Architecture:** Add `context_tokens` as a required parameter to all MCP tool schemas (except `local_status`). Track timestamps between tool calls in `ActivityLogger` to determine cache hit/miss. Price input at cached ($1.875/M) or uncached ($15/M) rate based on 5-minute TTL. Price output at $75/M always.

**Tech Stack:** Rust (MCP server), Swift (companion window UI)

---

### File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `src/mcp/logger.rs` | Modify | Add cache-aware pricing, `last_log_time` tracking, new fields on `ActivityEntry` |
| `src/mcp/tools.rs` | Modify | Add `context_tokens` to all tool schemas (required), extract in dispatch |
| `src/mcp/handlers.rs` | Modify | Thread `context_tokens` through all handlers and `complete_and_log` |
| `glass-slipper/MCPActivityLog.swift` | Modify | Parse new `cache_hit` and `context_tokens` fields |

---

### Task 1: Logger — add cache-aware pricing and new fields

**Files:**
- Modify: `src/mcp/logger.rs`

- [ ] **Step 1: Write failing tests for cache-aware pricing**

Add these tests to the existing `mod tests` block in `src/mcp/logger.rs`. They will fail because `ActivityLogger` doesn't have `last_log_time`, `log()` doesn't accept `context_tokens`, and `ActivityEntry` doesn't have `cache_hit`/`context_tokens` fields.

```rust
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
    // Second call immediately — well within 5 min TTL
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
    // 10000 context tokens at $15/M = $0.15
    // 50 output tokens at $75/M = $0.00375
    // Total = $0.154 (rounded to 3 decimal places)
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
    // First call: uncached
    // Second call: 10000 context tokens at $1.875/M = $0.01875
    //              50 output tokens at $75/M = $0.00375
    //              Total = $0.023 (rounded to 3 decimal places)
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
    // 0 context tokens, 1000 output tokens at $75/M = $0.075
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
    assert!(!entry.cache_hit); // first call is always uncached

    // Verify the JSON string contains the fields
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

    // Simulate cache expiry by directly setting last_log_time to 6 minutes ago
    {
        let mut last = logger.last_log_time.lock().unwrap();
        *last = Some(Instant::now() - std::time::Duration::from_secs(360));
    }

    logger.log("local_summarize", "after-expiry", 100, 50, start, "qwen", 10000);

    let contents = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    let entry: ActivityEntry = serde_json::from_str(lines[1]).unwrap();
    assert!(!entry.cache_hit); // cache expired — should be uncached
    // Should be priced at full rate: 10000 * $15/M + 50 * $75/M = $0.154
    assert!((entry.estimated_cloud_cost_usd - 0.154).abs() < 0.001);
}

#[test]
fn test_large_context_uncached_cost() {
    // 200k context at $15/M = $3.00, 512 output at $75/M = $0.0384
    // Total = $3.038
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
    // 200k context at $1.875/M = $0.375, 512 output at $75/M = $0.0384
    // Total = $0.413
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --bin glass-slipper-mcp -- logger::tests`
Expected: compilation errors — `log()` takes wrong number of args, `ActivityEntry` missing fields

- [ ] **Step 3: Implement cache-aware logger**

Replace the full contents of `src/mcp/logger.rs` with:

```rust
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
```

- [ ] **Step 4: Update existing tests to match new signature**

Replace the two existing tests (`test_log_creates_file_and_appends` and `test_cost_calculation`) with updated versions that use the new 7-arg `log()` signature:

```rust
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
    // Verify pricing is based on context_tokens (the Claude turn size),
    // not input_tokens (the local model's tokens).
    // 50000 context at $15/M = $0.75, 0 output = $0.75
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.jsonl");
    let logger = ActivityLogger::with_path(path.clone());

    let start = Instant::now();
    // input_tokens=100 (local model), context_tokens=50000 (Claude turn)
    logger.log("local_summarize", "test", 100, 0, start, "qwen", 50000);

    let contents = std::fs::read_to_string(&path).unwrap();
    let entry: ActivityEntry = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
    // Should be priced on context_tokens (50000), not input_tokens (100)
    assert!((entry.estimated_cloud_cost_usd - 0.75).abs() < 0.001);
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --bin glass-slipper-mcp -- logger::tests`
Expected: all logger tests pass

- [ ] **Step 6: Commit**

```bash
git add src/mcp/logger.rs
git commit -m "feat(mcp): cache-aware savings with context_tokens and output pricing"
```

---

### Task 2: Tool schemas — add `context_tokens` as required parameter

**Files:**
- Modify: `src/mcp/tools.rs`

- [ ] **Step 1: Write failing tests for context_tokens in schemas**

Add these tests to the existing `mod tests` block in `src/mcp/tools.rs`:

```rust
#[test]
fn test_all_tools_except_status_require_context_tokens() {
    let defs = tool_definitions();
    for tool in defs["tools"].as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        let required = tool["inputSchema"]["required"].as_array().unwrap();
        let required_strs: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        if name == "local_status" {
            assert!(!required_strs.contains(&"context_tokens"),
                "local_status should NOT require context_tokens");
        } else {
            assert!(required_strs.contains(&"context_tokens"),
                "Tool {} should require context_tokens", name);
        }
    }
}

#[test]
fn test_all_tools_except_status_have_context_tokens_property() {
    let defs = tool_definitions();
    for tool in defs["tools"].as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        let props = tool["inputSchema"]["properties"].as_object().unwrap();
        if name == "local_status" {
            assert!(!props.contains_key("context_tokens"),
                "local_status should NOT have context_tokens property");
        } else {
            assert!(props.contains_key("context_tokens"),
                "Tool {} should have context_tokens property", name);
            assert_eq!(props["context_tokens"]["type"], "integer",
                "Tool {} context_tokens should be integer", name);
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --bin glass-slipper-mcp -- tools::tests`
Expected: FAIL — `context_tokens` not in any schema yet

- [ ] **Step 3: Add `context_tokens` to all tool schemas**

Add this property to the `"properties"` object of every tool **except** `local_status`:

```json
"context_tokens": {
    "type": "integer",
    "description": "Current conversation context size in tokens. Used for savings estimation."
}
```

And add `"context_tokens"` to each tool's `"required"` array.

For `local_summarize`, the full `inputSchema` becomes:
```json
"inputSchema": {
    "type": "object",
    "properties": {
        "command": {
            "type": "string",
            "description": "The shell command to run"
        },
        "context_tokens": {
            "type": "integer",
            "description": "Current conversation context size in tokens. Used for savings estimation."
        }
    },
    "required": ["command", "context_tokens"]
}
```

Apply the same pattern to: `local_pass_fail`, `local_explain`, `local_ask`, `local_web_fetch`, `local_review`, `local_draft`.

- [ ] **Step 4: Update dispatch to extract `context_tokens` and pass to handlers**

Replace the `dispatch` function:

```rust
/// Dispatch a tool call to the appropriate handler.
pub async fn dispatch(
    tool_name: &str,
    arguments: &serde_json::Value,
    client: &McpLlmClient,
    logger: &ActivityLogger,
) -> ToolResult {
    let context_tokens = arguments.get("context_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    match tool_name {
        "local_summarize" => {
            let command = arguments.get("command").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_summarize(client, logger, command, context_tokens).await
        }
        "local_pass_fail" => {
            let command = arguments.get("command").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_pass_fail(client, logger, command, context_tokens).await
        }
        "local_explain" => {
            let code = arguments.get("code").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_explain(client, logger, code, context_tokens).await
        }
        "local_ask" => {
            let question = arguments.get("question").and_then(|v| v.as_str()).unwrap_or("");
            let context = arguments.get("context").and_then(|v| v.as_str());
            handlers::handle_ask(client, logger, question, context, context_tokens).await
        }
        "local_web_fetch" => {
            let url = arguments.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let question = arguments.get("question").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_web_fetch(client, logger, url, question, context_tokens).await
        }
        "local_review" => {
            let diff = arguments.get("diff").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_review(client, logger, diff, context_tokens).await
        }
        "local_draft" => {
            let task = arguments.get("task").and_then(|v| v.as_str()).unwrap_or("");
            let context = arguments.get("context").and_then(|v| v.as_str());
            handlers::handle_draft(client, logger, task, context, context_tokens).await
        }
        "local_status" => {
            handlers::handle_status(client).await
        }
        _ => ToolResult::error(format!("Unknown tool: {}", tool_name)),
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --bin glass-slipper-mcp -- tools::tests`
Expected: all tools tests pass (compilation will still fail due to handler signatures — that's Task 3)

- [ ] **Step 6: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "feat(mcp): add context_tokens as required param to all tool schemas"
```

---

### Task 3: Handlers — thread `context_tokens` through all handlers

**Files:**
- Modify: `src/mcp/handlers.rs`

- [ ] **Step 1: Update `complete_and_log` to accept and pass `context_tokens`**

Change the `complete_and_log` signature and body:

```rust
async fn complete_and_log(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    tool_name: &str,
    detail: &str,
    system_prompt: &str,
    user_content: &str,
    max_tokens: u32,
    start: Instant,
    context_tokens: u64,
) -> ToolResult {
    match client.complete(system_prompt, user_content, max_tokens).await {
        Ok(result) => {
            logger.log(tool_name, detail, result.input_tokens, result.output_tokens, start, client.model_name(), context_tokens);
            ToolResult::text(result.text)
        }
        Err(e) => ToolResult::error(format!("LLM call failed: {}", e)),
    }
}
```

- [ ] **Step 2: Update all handler signatures to accept `context_tokens`**

Add `context_tokens: u64` as the last parameter to every handler function: `handle_summarize`, `handle_pass_fail`, `handle_explain`, `handle_ask`, `handle_web_fetch`, `handle_review`, `handle_draft`. Then pass it through to `complete_and_log` and direct `logger.log()` calls.

`handle_summarize`:
```rust
pub async fn handle_summarize(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    command: &str,
    context_tokens: u64,
) -> ToolResult {
    let start = Instant::now();
    let detail = slug_command(command);
    let output = match run_command(command).await {
        Ok(out) => out,
        Err(e) => return ToolResult::error(format!("Failed to run command: {}", e)),
    };
    if output.is_empty() {
        logger.log("local_summarize", &detail, 0, 0, start, client.model_name(), context_tokens);
        return ToolResult::text("Command produced no output.");
    }
    complete_and_log(client, logger, "local_summarize", &detail, prompts::SUMMARIZE, &output, SHORT_MAX_TOKENS, start, context_tokens).await
}
```

`handle_pass_fail`:
```rust
pub async fn handle_pass_fail(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    command: &str,
    context_tokens: u64,
) -> ToolResult {
    let start = Instant::now();
    let detail = slug_command(command);
    let output = match run_command(command).await {
        Ok(out) => out,
        Err(e) => return ToolResult::error(format!("Failed to run command: {}", e)),
    };
    if output.is_empty() {
        logger.log("local_pass_fail", &detail, 0, 0, start, client.model_name(), context_tokens);
        return ToolResult::text("Command produced no output.");
    }
    complete_and_log(client, logger, "local_pass_fail", &detail, prompts::PASS_FAIL, &output, SHORT_MAX_TOKENS, start, context_tokens).await
}
```

`handle_explain`:
```rust
pub async fn handle_explain(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    code: &str,
    context_tokens: u64,
) -> ToolResult {
    let start = Instant::now();
    let detail = slug_first_line(code, 60);
    complete_and_log(client, logger, "local_explain", &detail, prompts::EXPLAIN, code, SHORT_MAX_TOKENS, start, context_tokens).await
}
```

`handle_ask`:
```rust
pub async fn handle_ask(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    question: &str,
    context: Option<&str>,
    context_tokens: u64,
) -> ToolResult {
    let start = Instant::now();
    let detail = truncate(question, 60);
    let user_content = match context {
        Some(ctx) => format!("Context:\n{}\n\nQuestion: {}", ctx, question),
        None => question.to_string(),
    };
    complete_and_log(client, logger, "local_ask", &detail, prompts::ASK, &user_content, SHORT_MAX_TOKENS, start, context_tokens).await
}
```

`handle_web_fetch`:
```rust
pub async fn handle_web_fetch(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    url: &str,
    question: &str,
    context_tokens: u64,
) -> ToolResult {
    let start = Instant::now();
    let detail = slug_url(url);

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let response = match http_client.get(url).send().await {
        Ok(r) => r,
        Err(e) => return ToolResult::error(format!("Failed to fetch URL: {}", e)),
    };

    let body = match response.text().await {
        Ok(b) => b,
        Err(e) => return ToolResult::error(format!("Failed to read response body: {}", e)),
    };

    let text = strip_html_tags(&body);
    let truncated_text = if text.len() > 16000 { &text[..16000] } else { &text };
    let user_content = format!("Page content:\n{}\n\nQuestion: {}", truncated_text, question);
    complete_and_log(client, logger, "local_web_fetch", &detail, prompts::WEB_FETCH, &user_content, SHORT_MAX_TOKENS, start, context_tokens).await
}
```

`handle_review`:
```rust
pub async fn handle_review(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    diff: &str,
    context_tokens: u64,
) -> ToolResult {
    let start = Instant::now();
    let detail = slug_diff(diff);
    complete_and_log(client, logger, "local_review", &detail, prompts::REVIEW, diff, SHORT_MAX_TOKENS, start, context_tokens).await
}
```

`handle_draft`:
```rust
pub async fn handle_draft(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    task: &str,
    context: Option<&str>,
    context_tokens: u64,
) -> ToolResult {
    let start = Instant::now();
    let detail = truncate(task, 60);
    let user_content = match context {
        Some(ctx) => format!("Context:\n{}\n\nTask: {}", ctx, task),
        None => task.to_string(),
    };
    complete_and_log(client, logger, "local_draft", &detail, prompts::DRAFT, &user_content, LONG_MAX_TOKENS, start, context_tokens).await
}
```

`handle_status` is unchanged — it doesn't take `context_tokens`.

- [ ] **Step 3: Run full test suite**

Run: `cargo test --bin glass-slipper-mcp`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add src/mcp/handlers.rs
git commit -m "feat(mcp): thread context_tokens through all handlers"
```

---

### Task 4: Swift UI — parse new JSONL fields

**Files:**
- Modify: `glass-slipper/MCPActivityLog.swift`
- Modify: `glass-slipper/CompanionWindowController.swift`

- [ ] **Step 1: Add new fields to `MCPActivityEntry`**

Add `contextTokens` and `cacheHit` to the struct:

```swift
struct MCPActivityEntry {
    let timestamp: String
    let tool: String
    let detail: String
    let inputTokens: Int
    let outputTokens: Int
    let contextTokens: Int
    let latencyMs: Int
    let estimatedCloudCostUSD: Double
    let cacheHit: Bool
    let model: String
}
```

- [ ] **Step 2: Parse new fields in `readNewEntries()`**

Update the entry construction in `MCPActivityLog.swift`:

```swift
let entry = MCPActivityEntry(
    timestamp: json["ts"] as? String ?? "",
    tool: json["tool"] as? String ?? "",
    detail: json["detail"] as? String ?? "",
    inputTokens: json["input_tokens"] as? Int ?? 0,
    outputTokens: json["output_tokens"] as? Int ?? 0,
    contextTokens: json["context_tokens"] as? Int ?? 0,
    latencyMs: json["latency_ms"] as? Int ?? 0,
    estimatedCloudCostUSD: json["estimated_cloud_cost_usd"] as? Double ?? 0,
    cacheHit: json["cache_hit"] as? Bool ?? false,
    model: json["model"] as? String ?? ""
)
```

- [ ] **Step 3: Update `totalTokensSaved` to use `contextTokens`**

In the accumulation loop, change:
```swift
summary.totalTokensSaved += entry.inputTokens
```
to:
```swift
summary.totalTokensSaved += entry.contextTokens
```

- [ ] **Step 4: Add `local_pass_fail` to display name mapping**

In `CompanionWindowController.swift`, add a case to `toolDisplayName`:

```swift
case "local_pass_fail": verb = "Pass/Fail"
```

- [ ] **Step 5: Build the Xcode project to verify**

Run: `xcodebuild -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper build 2>&1 | tail -5` (or equivalent)
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add glass-slipper/MCPActivityLog.swift glass-slipper/CompanionWindowController.swift
git commit -m "feat(swift): parse cache_hit and context_tokens from activity log"
```

---

### Task 5: Final verification

- [ ] **Step 1: Run full Rust test suite**

Run: `cargo test`
Expected: all tests pass, no warnings related to our changes

- [ ] **Step 2: Verify JSONL format manually**

Run a quick sanity check that the test output contains all expected fields:

```bash
cargo test --bin glass-slipper-mcp -- logger::tests::test_context_tokens_and_cache_hit_roundtrip_through_serde --nocapture 2>&1 | grep -E "cache_hit|context_tokens"
```

- [ ] **Step 3: Review all changes**

Run: `git diff master --stat` and `git log --oneline master..HEAD`
Verify: 4 commits, 4 files changed (logger.rs, tools.rs, handlers.rs, MCPActivityLog.swift + CompanionWindowController.swift)
