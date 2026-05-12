# MCP Companion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a `glass-slipper-mcp` binary that exposes 7 MCP tools backed by the local llama-server, plus a companion window in the Glass Slipper app for one-click setup and savings tracking.

**Architecture:** A thin Rust MCP binary (stdio ↔ HTTP bridge) ships inside GlassSlipper.app. It translates MCP tool calls into requests against the already-running llama-server. Every call logs to a shared JSONL file that the Swift companion window reads for its savings dashboard and activity feed.

**Tech Stack:** Rust (MCP binary, serde_json for protocol, reqwest for HTTP), Swift/AppKit (companion window), JSONL (activity log)

---

## File Structure

### New Rust files (MCP binary)

| File | Responsibility |
|------|---------------|
| `src/mcp/main.rs` | MCP binary entry point. Reads stdin JSON-RPC, dispatches to handlers, writes stdout. |
| `src/mcp/protocol.rs` | MCP JSON-RPC types: `Request`, `Response`, `ToolDefinition`, `InitializeResult`. Serde structs only. |
| `src/mcp/tools.rs` | Tool dispatch: match tool name → handler function. Returns `ToolResult`. |
| `src/mcp/handlers.rs` | Individual tool handler implementations: `summarize`, `explain`, `ask`, `web_fetch`, `review`, `draft`, `status`. |
| `src/mcp/prompts.rs` | Task-specific system prompts for each tool. One const per tool. |
| `src/mcp/logger.rs` | JSONL activity logger. Appends one line per tool call to `~/Library/Application Support/Glass Slipper/mcp-activity.jsonl`. |
| `src/mcp/llm_client.rs` | Minimal non-streaming OpenAI chat completion client (MCP doesn't need streaming — fire and wait). |

### New Swift files (companion window)

| File | Responsibility |
|------|---------------|
| `glass-slipper/CompanionWindowController.swift` | NSWindowController for the companion window. Setup checklist + dashboard layout. |
| `glass-slipper/MCPActivityLog.swift` | Reads and tails `mcp-activity.jsonl`. Parses entries, notifies the window of new rows. |
| `glass-slipper/MCPInstaller.swift` | Reads/writes `~/.claude.json` to add/remove the MCP server config entry. |

### Modified files

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `[[bin]]` entry for `glass-slipper-mcp`. |
| `glass-slipper/AppDelegate.swift` | Add menu item to open companion window. |
| `glass-slipper/CinderellaScaffold.swift` | Add color/typography tokens for companion window (savings green, MCP blue). |
| `glass-slipper/GlassSlipper.xcodeproj/project.pbxproj` | Add new Swift files to build target + cargo build phase for MCP binary. |

---

### Task 1: MCP Protocol Types

**Files:**
- Create: `src/mcp/protocol.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write the failing test**

Create `src/mcp/protocol.rs` with tests that verify JSON-RPC serialization:

```rust
// src/mcp/protocol.rs

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 request from Claude Code.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 response back to Claude Code.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

/// MCP tool content block.
#[derive(Debug, Serialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// MCP tool result (returned inside JSON-RPC response).
#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }

    pub fn error(id: Option<serde_json::Value>, code: i32, message: String) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: None, error: Some(JsonRpcError { code, message }) }
    }
}

impl ToolResult {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent { content_type: "text".into(), text: s.into() }],
            is_error: None,
        }
    }

    pub fn error(s: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent { content_type: "text".into(), text: s.into() }],
            is_error: Some(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_initialize_request() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, Some(serde_json::json!(1)));
    }

    #[test]
    fn test_parse_tool_call_request() {
        let json = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"local_status","arguments":{}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "tools/call");
        let name = req.params.get("name").and_then(|v| v.as_str());
        assert_eq!(name, Some("local_status"));
    }

    #[test]
    fn test_serialize_success_response() {
        let resp = JsonRpcResponse::success(Some(serde_json::json!(1)), serde_json::json!({"status": "ok"}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""status":"ok""#));
        assert!(!json.contains("error"));
    }

    #[test]
    fn test_serialize_error_response() {
        let resp = JsonRpcResponse::error(Some(serde_json::json!(1)), -32601, "Method not found".into());
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""code":-32601"#));
        assert!(!json.contains("result"));
    }

    #[test]
    fn test_tool_result_text() {
        let r = ToolResult::text("BUILD SUCCEEDED");
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "BUILD SUCCEEDED");
        assert!(json.get("is_error").is_none());
    }

    #[test]
    fn test_tool_result_error() {
        let r = ToolResult::error("llama-server not running");
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["is_error"], true);
    }
}
```

- [ ] **Step 2: Create the mcp module directory and mod.rs**

```bash
mkdir -p src/mcp
```

Create `src/mcp/mod.rs`:

```rust
pub mod protocol;
```

- [ ] **Step 3: Add the MCP binary target to Cargo.toml**

Add a second `[[bin]]` section after the existing one:

```toml
[[bin]]
name = "glass-slipper-mcp"
path = "src/mcp/main.rs"
```

Create a minimal `src/mcp/main.rs` so cargo can find both binaries:

```rust
mod protocol;

fn main() {
    eprintln!("glass-slipper-mcp: not yet implemented");
    std::process::exit(1);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --bin glass-slipper-mcp`
Expected: All 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/mcp/ Cargo.toml
git commit -m "feat(mcp): add protocol types with JSON-RPC serde"
```

---

### Task 2: System Prompts

**Files:**
- Create: `src/mcp/prompts.rs`
- Modify: `src/mcp/mod.rs`

- [ ] **Step 1: Write the prompts module**

Create `src/mcp/prompts.rs` with task-specific system prompts:

```rust
// src/mcp/prompts.rs

/// Returns the system prompt for a given tool. Preamble is prepended to each.
pub fn system_prompt(tool: &str) -> &'static str {
    match tool {
        "local_summarize" => SUMMARIZE,
        "local_explain" => EXPLAIN,
        "local_ask" => ASK,
        "local_web_fetch" => WEB_FETCH,
        "local_review" => REVIEW,
        "local_draft" => DRAFT,
        _ => PREAMBLE,
    }
}

const PREAMBLE: &str = "You are a local offload model running on the user's machine. \
A frontier AI assistant (Claude) has delegated this task to you to save tokens and cost. \
Be concise. No preamble, no disclaimers, no follow-up questions. \
If you are unsure, say so in one sentence — do not hallucinate.";

pub const SUMMARIZE: &str = "You are a local offload model running on the user's machine. \
A frontier AI assistant (Claude) has delegated this task to you to save tokens and cost. \
Be concise. No preamble, no disclaimers, no follow-up questions. \
If you are unsure, say so in one sentence — do not hallucinate.\n\n\
Your task: summarize the output of a shell command. Report:\n\
1. Pass or fail (one word)\n\
2. If failed: the specific error(s), with file and line number if present\n\
3. If passed: any warnings worth noting (skip if zero warnings)\n\
Keep it under 5 sentences. Do not reproduce the raw output.";

pub const EXPLAIN: &str = "You are a local offload model running on the user's machine. \
A frontier AI assistant (Claude) has delegated this task to you to save tokens and cost. \
Be concise. No preamble, no disclaimers, no follow-up questions. \
If you are unsure, say so in one sentence — do not hallucinate.\n\n\
Your task: explain what the given code does in plain English. \
Focus on the purpose and behavior, not line-by-line narration. \
One paragraph unless the code is complex enough to warrant more.";

pub const ASK: &str = "You are a local offload model running on the user's machine. \
A frontier AI assistant (Claude) has delegated this task to you to save tokens and cost. \
Be concise. No preamble, no disclaimers, no follow-up questions. \
If you are unsure, say so in one sentence — do not hallucinate.\n\n\
Your task: answer the question using the provided context (if any). \
Be direct. If the context doesn't contain the answer, say so.";

pub const WEB_FETCH: &str = "You are a local offload model running on the user's machine. \
A frontier AI assistant (Claude) has delegated this task to you to save tokens and cost. \
Be concise. No preamble, no disclaimers, no follow-up questions. \
If you are unsure, say so in one sentence — do not hallucinate.\n\n\
Your task: answer a specific question about a web page. \
You will receive the page content as markdown. \
Extract only the information needed to answer the question. \
Do not summarize the whole page — answer the question and stop.";

pub const REVIEW: &str = "You are a local offload model running on the user's machine. \
A frontier AI assistant (Claude) has delegated this task to you to save tokens and cost. \
Be concise. No preamble, no disclaimers, no follow-up questions. \
If you are unsure, say so in one sentence — do not hallucinate.\n\n\
Your task: summarize a code diff. Report:\n\
1. What changed (files, functions, behavior)\n\
2. Why it likely changed (if obvious from context)\n\
Keep it under 5 sentences. Do not reproduce the diff.";

pub const DRAFT: &str = "You are a local offload model running on the user's machine. \
A frontier AI assistant (Claude) has delegated this task to you to save tokens and cost. \
Be concise. No preamble, no disclaimers, no follow-up questions. \
If you are unsure, say so in one sentence — do not hallucinate.\n\n\
Your task: generate code or text as requested. \
Follow the instructions exactly. Output only the requested content — \
no explanations, no markdown code fences unless the content itself is markdown.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompts_contain_preamble() {
        assert!(SUMMARIZE.starts_with("You are a local offload model"));
        assert!(EXPLAIN.starts_with("You are a local offload model"));
        assert!(ASK.starts_with("You are a local offload model"));
        assert!(WEB_FETCH.starts_with("You are a local offload model"));
        assert!(REVIEW.starts_with("You are a local offload model"));
        assert!(DRAFT.starts_with("You are a local offload model"));
    }

    #[test]
    fn test_prompts_contain_task_instructions() {
        assert!(SUMMARIZE.contains("Pass or fail"));
        assert!(EXPLAIN.contains("plain English"));
        assert!(ASK.contains("answer the question"));
        assert!(WEB_FETCH.contains("web page"));
        assert!(REVIEW.contains("code diff"));
        assert!(DRAFT.contains("generate code"));
    }

    #[test]
    fn test_system_prompt_dispatch() {
        assert_eq!(system_prompt("local_summarize"), SUMMARIZE);
        assert_eq!(system_prompt("local_explain"), EXPLAIN);
        assert_eq!(system_prompt("unknown_tool"), PREAMBLE);
    }
}
```

The preamble is duplicated in each prompt constant to keep them as simple `&'static str` — no macros, no crates, no lifetime issues. The repetition is intentional and deliberate: these are prompt text, not code. DRY doesn't apply to string constants that may diverge during tuning.

- [ ] **Step 2: Add `prompts` to `src/mcp/mod.rs`**

```rust
pub mod protocol;
pub mod prompts;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --bin glass-slipper-mcp`
Expected: All prompt tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/mcp/prompts.rs src/mcp/mod.rs
git commit -m "feat(mcp): add task-specific system prompts for Qwen"
```

---

### Task 3: JSONL Activity Logger

**Files:**
- Create: `src/mcp/logger.rs`
- Modify: `src/mcp/mod.rs`

- [ ] **Step 1: Write the logger with tests**

Create `src/mcp/logger.rs`:

```rust
// src/mcp/logger.rs

use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

/// One line in mcp-activity.jsonl.
#[derive(Debug, Serialize)]
pub struct ActivityEntry {
    pub ts: String,
    pub tool: String,
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
    pub fn log(&self, tool: &str, input_tokens: u64, output_tokens: u64, start: Instant, model: &str) {
        let latency_ms = start.elapsed().as_millis() as u64;
        let estimated_cost = input_tokens as f64 * OPUS_INPUT_PRICE_PER_TOKEN;

        let entry = ActivityEntry {
            ts: now_iso8601(),
            tool: tool.to_string(),
            input_tokens,
            output_tokens,
            latency_ms,
            estimated_cloud_cost_usd: (estimated_cost * 1000.0).round() / 1000.0, // 3 decimal places
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

fn now_iso8601() -> String {
    // Use a simple approach without chrono dependency.
    // Shell out to date is gross; use SystemTime instead.
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    // Format as seconds since epoch — the Swift side can parse this.
    // For human-readable ISO-8601, we'd need chrono. Keep it simple.
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
        logger.log("local_summarize", 3200, 18, start, "qwen3.5-9b-q5_k_m");
        logger.log("local_explain", 500, 120, start, "qwen3.5-9b-q5_k_m");

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
        // 3200 tokens * $15/M = $0.048
        let cost = 3200.0 * OPUS_INPUT_PRICE_PER_TOKEN;
        assert!((cost - 0.048).abs() < 0.001);
    }
}
```

- [ ] **Step 2: Add `logger` to `src/mcp/mod.rs`**

```rust
pub mod protocol;
pub mod prompts;
pub mod logger;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --bin glass-slipper-mcp`
Expected: Logger tests pass. File creation and JSONL format verified.

- [ ] **Step 4: Commit**

```bash
git add src/mcp/logger.rs src/mcp/mod.rs
git commit -m "feat(mcp): add JSONL activity logger with cost estimation"
```

---

### Task 4: Minimal LLM Client for MCP

**Files:**
- Create: `src/mcp/llm_client.rs`
- Modify: `src/mcp/mod.rs`

The existing `src/llm.rs` is a streaming client designed for the agent loop. The MCP binary needs a simpler non-streaming client — fire a request, wait for the full response, return it. No SSE parsing needed.

- [ ] **Step 1: Write the non-streaming client with tests**

Create `src/mcp/llm_client.rs`:

```rust
// src/mcp/llm_client.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

pub struct CompletionResult {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub struct McpLlmClient {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl McpLlmClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            model: model.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Send a non-streaming chat completion. Returns the response text and token counts.
    pub async fn complete(
        &self,
        system_prompt: &str,
        user_content: &str,
        max_tokens: u32,
    ) -> Result<CompletionResult, String> {
        let messages = vec![
            ChatMessage { role: "system".into(), content: system_prompt.into() },
            ChatMessage { role: "user".into(), content: user_content.into() },
        ];

        let body = ChatRequest {
            model: self.model.clone(),
            messages,
            temperature: 0.1,
            max_tokens,
        };

        let response = self.client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to llama-server: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("llama-server returned {}: {}", status, body));
        }

        let resp: ChatResponse = response.json().await
            .map_err(|e| format!("Failed to parse llama-server response: {}", e))?;

        let text = resp.choices.first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let (input_tokens, output_tokens) = resp.usage
            .map(|u| (u.prompt_tokens, u.completion_tokens))
            .unwrap_or((0, 0));

        Ok(CompletionResult { text, input_tokens, output_tokens })
    }

    /// Check if llama-server is reachable.
    pub async fn health_check(&self) -> Result<(), String> {
        self.client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Health check failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("Health check returned error: {}", e))?;
        Ok(())
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_serialization() {
        let body = ChatRequest {
            model: "qwen3.5-9b".into(),
            messages: vec![
                ChatMessage { role: "system".into(), content: "Be concise.".into() },
                ChatMessage { role: "user".into(), content: "Hello".into() },
            ],
            temperature: 0.1,
            max_tokens: 1024,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "qwen3.5-9b");
        assert_eq!(json["messages"].as_array().unwrap().len(), 2);
        assert_eq!(json["temperature"], 0.1);
    }

    #[test]
    fn test_parse_chat_response() {
        let json = r#"{
            "choices": [{"message": {"content": "BUILD SUCCEEDED"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 3200, "completion_tokens": 2, "total_tokens": 3202}
        }"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.choices[0].message.content.as_deref(), Some("BUILD SUCCEEDED"));
        assert_eq!(resp.usage.unwrap().prompt_tokens, 3200);
    }

    #[test]
    fn test_parse_response_without_usage() {
        let json = r#"{"choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}]}"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.usage.is_none());
    }
}
```

- [ ] **Step 2: Add `llm_client` to `src/mcp/mod.rs`**

```rust
pub mod protocol;
pub mod prompts;
pub mod logger;
pub mod llm_client;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --bin glass-slipper-mcp`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/mcp/llm_client.rs src/mcp/mod.rs
git commit -m "feat(mcp): add non-streaming LLM client for MCP tools"
```

---

### Task 5: Tool Handlers

**Files:**
- Create: `src/mcp/handlers.rs`
- Modify: `src/mcp/mod.rs`

- [ ] **Step 1: Write handlers**

Create `src/mcp/handlers.rs`:

```rust
// src/mcp/handlers.rs

use crate::mcp::llm_client::{CompletionResult, McpLlmClient};
use crate::mcp::logger::ActivityLogger;
use crate::mcp::prompts;
use crate::mcp::protocol::ToolResult;
use std::time::Instant;

/// Max tokens for LLM responses. Short for summaries, longer for drafts.
const SHORT_MAX_TOKENS: u32 = 512;
const LONG_MAX_TOKENS: u32 = 2048;

pub async fn handle_summarize(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    command: &str,
) -> ToolResult {
    let start = Instant::now();

    // Run the command
    let output = match run_command(command).await {
        Ok(out) => out,
        Err(e) => return ToolResult::error(format!("Failed to run command: {}", e)),
    };

    if output.is_empty() {
        log_and_return(logger, "local_summarize", 0, 0, start, client.model_name(),
            ToolResult::text("Command produced no output."))
    } else {
        complete_and_log(client, logger, "local_summarize", prompts::SUMMARIZE, &output, SHORT_MAX_TOKENS, start).await
    }
}

pub async fn handle_explain(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    code: &str,
) -> ToolResult {
    let start = Instant::now();
    complete_and_log(client, logger, "local_explain", prompts::EXPLAIN, code, SHORT_MAX_TOKENS, start).await
}

pub async fn handle_ask(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    question: &str,
    context: Option<&str>,
) -> ToolResult {
    let start = Instant::now();
    let user_content = match context {
        Some(ctx) => format!("Context:\n{}\n\nQuestion: {}", ctx, question),
        None => question.to_string(),
    };
    complete_and_log(client, logger, "local_ask", prompts::ASK, &user_content, SHORT_MAX_TOKENS, start).await
}

pub async fn handle_web_fetch(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    url: &str,
    question: &str,
) -> ToolResult {
    let start = Instant::now();

    // Fetch the URL
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

    // Basic HTML to text: strip tags. Not perfect, but serviceable.
    let text = strip_html_tags(&body);

    // Truncate to avoid overwhelming the local model's context.
    let truncated = if text.len() > 16000 { &text[..16000] } else { &text };

    let user_content = format!("Page content:\n{}\n\nQuestion: {}", truncated, question);
    complete_and_log(client, logger, "local_web_fetch", prompts::WEB_FETCH, &user_content, SHORT_MAX_TOKENS, start).await
}

pub async fn handle_review(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    diff: &str,
) -> ToolResult {
    let start = Instant::now();
    complete_and_log(client, logger, "local_review", prompts::REVIEW, diff, SHORT_MAX_TOKENS, start).await
}

pub async fn handle_draft(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    task: &str,
    context: Option<&str>,
) -> ToolResult {
    let start = Instant::now();
    let user_content = match context {
        Some(ctx) => format!("Context:\n{}\n\nTask: {}", ctx, task),
        None => task.to_string(),
    };
    complete_and_log(client, logger, "local_draft", prompts::DRAFT, &user_content, LONG_MAX_TOKENS, start).await
}

pub async fn handle_status(client: &McpLlmClient) -> ToolResult {
    match client.health_check().await {
        Ok(()) => ToolResult::text(format!(
            "Model: {}\nStatus: running\nEndpoint: healthy",
            client.model_name()
        )),
        Err(e) => ToolResult::error(format!(
            "Model: {}\nStatus: unreachable\nError: {}",
            client.model_name(), e
        )),
    }
}

// -- Helpers --

async fn run_command(command: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .await
        .map_err(|e| format!("Failed to execute: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !combined.is_empty() { combined.push('\n'); }
        combined.push_str(&stderr);
    }

    // Include exit code if non-zero
    if !output.status.success() {
        combined.push_str(&format!("\n[exit code: {}]", output.status.code().unwrap_or(-1)));
    }

    Ok(combined)
}

async fn complete_and_log(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    tool_name: &str,
    system_prompt: &str,
    user_content: &str,
    max_tokens: u32,
    start: Instant,
) -> ToolResult {
    match client.complete(system_prompt, user_content, max_tokens).await {
        Ok(result) => {
            logger.log(tool_name, result.input_tokens, result.output_tokens, start, client.model_name());
            ToolResult::text(result.text)
        }
        Err(e) => ToolResult::error(format!("LLM call failed: {}", e)),
    }
}

fn log_and_return(
    logger: &ActivityLogger,
    tool_name: &str,
    input_tokens: u64,
    output_tokens: u64,
    start: Instant,
    model: &str,
    result: ToolResult,
) -> ToolResult {
    logger.log(tool_name, input_tokens, output_tokens, start, model);
    result
}

/// Strip HTML tags. Crude but effective for converting web pages to readable text.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<p>Hello <b>world</b></p>"), "Hello world");
        assert_eq!(strip_html_tags("no tags here"), "no tags here");
        assert_eq!(strip_html_tags("<div><span>nested</span></div>"), "nested");
    }

    #[tokio::test]
    async fn test_run_command_success() {
        let output = run_command("echo hello").await.unwrap();
        assert_eq!(output.trim(), "hello");
    }

    #[tokio::test]
    async fn test_run_command_failure() {
        let output = run_command("false").await.unwrap();
        assert!(output.contains("exit code"));
    }

    #[tokio::test]
    async fn test_run_command_stderr() {
        let output = run_command("echo err >&2").await.unwrap();
        assert!(output.contains("err"));
    }
}
```

- [ ] **Step 2: Add `handlers` to `src/mcp/mod.rs`**

```rust
pub mod protocol;
pub mod prompts;
pub mod logger;
pub mod llm_client;
pub mod handlers;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --bin glass-slipper-mcp`
Expected: HTML stripping and command execution tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/mcp/handlers.rs src/mcp/mod.rs
git commit -m "feat(mcp): add tool handlers for all 7 MCP tools"
```

---

### Task 6: Tool Dispatch and Definitions

**Files:**
- Create: `src/mcp/tools.rs`
- Modify: `src/mcp/mod.rs`

- [ ] **Step 1: Write the tool dispatch module**

Create `src/mcp/tools.rs`:

```rust
// src/mcp/tools.rs

use crate::mcp::handlers;
use crate::mcp::llm_client::McpLlmClient;
use crate::mcp::logger::ActivityLogger;
use crate::mcp::protocol::ToolResult;

/// MCP tool definitions returned by tools/list.
pub fn tool_definitions() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "local_summarize",
                "description": "Run a shell command and return a concise summary of its output. Use this instead of Bash for noisy commands (builds, test suites) where you only need pass/fail and key details. Saves tokens by keeping verbose output out of context.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to run"
                        }
                    },
                    "required": ["command"]
                }
            },
            {
                "name": "local_explain",
                "description": "Explain what a piece of code does in plain English. Use this for 'what does this function do?' questions — cheaper than reading and analyzing the code yourself.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "code": {
                            "type": "string",
                            "description": "The code to explain"
                        }
                    },
                    "required": ["code"]
                }
            },
            {
                "name": "local_ask",
                "description": "Ask the local model a question, optionally with context. Use this for simple questions that don't require frontier reasoning.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The question to ask"
                        },
                        "context": {
                            "type": "string",
                            "description": "Optional context to inform the answer"
                        }
                    },
                    "required": ["question"]
                }
            },
            {
                "name": "local_web_fetch",
                "description": "Fetch a web page and answer a specific question about its content. Use this instead of WebFetch to keep large HTML pages out of your context — the local model reads the page and returns only the answer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to fetch"
                        },
                        "question": {
                            "type": "string",
                            "description": "What to extract from the page"
                        }
                    },
                    "required": ["url", "question"]
                }
            },
            {
                "name": "local_review",
                "description": "Summarize a code diff. Returns a concise description of what changed and why. Experimental — quality may vary.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "diff": {
                            "type": "string",
                            "description": "The diff to summarize"
                        }
                    },
                    "required": ["diff"]
                }
            },
            {
                "name": "local_draft",
                "description": "Generate code or text using the local model. Use for boilerplate, templates, and straightforward generation tasks. Experimental — quality may vary for complex code.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "What to generate"
                        },
                        "context": {
                            "type": "string",
                            "description": "Optional context (existing code, requirements, etc.)"
                        }
                    },
                    "required": ["task"]
                }
            },
            {
                "name": "local_status",
                "description": "Check the status of the local model. Returns model name, health, and endpoint info.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        ]
    })
}

/// Dispatch a tool call to the appropriate handler.
pub async fn dispatch(
    tool_name: &str,
    arguments: &serde_json::Value,
    client: &McpLlmClient,
    logger: &ActivityLogger,
) -> ToolResult {
    match tool_name {
        "local_summarize" => {
            let command = arguments.get("command").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_summarize(client, logger, command).await
        }
        "local_explain" => {
            let code = arguments.get("code").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_explain(client, logger, code).await
        }
        "local_ask" => {
            let question = arguments.get("question").and_then(|v| v.as_str()).unwrap_or("");
            let context = arguments.get("context").and_then(|v| v.as_str());
            handlers::handle_ask(client, logger, question, context).await
        }
        "local_web_fetch" => {
            let url = arguments.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let question = arguments.get("question").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_web_fetch(client, logger, url, question).await
        }
        "local_review" => {
            let diff = arguments.get("diff").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_review(client, logger, diff).await
        }
        "local_draft" => {
            let task = arguments.get("task").and_then(|v| v.as_str()).unwrap_or("");
            let context = arguments.get("context").and_then(|v| v.as_str());
            handlers::handle_draft(client, logger, task, context).await
        }
        "local_status" => {
            handlers::handle_status(client).await
        }
        _ => ToolResult::error(format!("Unknown tool: {}", tool_name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_has_seven_tools() {
        let defs = tool_definitions();
        let tools = defs["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);
    }

    #[test]
    fn test_tool_definitions_names() {
        let defs = tool_definitions();
        let names: Vec<&str> = defs["tools"].as_array().unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(names.contains(&"local_summarize"));
        assert!(names.contains(&"local_explain"));
        assert!(names.contains(&"local_ask"));
        assert!(names.contains(&"local_web_fetch"));
        assert!(names.contains(&"local_review"));
        assert!(names.contains(&"local_draft"));
        assert!(names.contains(&"local_status"));
    }

    #[test]
    fn test_all_tools_have_input_schema() {
        let defs = tool_definitions();
        for tool in defs["tools"].as_array().unwrap() {
            assert!(tool.get("inputSchema").is_some(), "Tool {} missing inputSchema", tool["name"]);
        }
    }
}
```

- [ ] **Step 2: Add `tools` to `src/mcp/mod.rs`**

```rust
pub mod protocol;
pub mod prompts;
pub mod logger;
pub mod llm_client;
pub mod handlers;
pub mod tools;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --bin glass-slipper-mcp`
Expected: All tests pass including 7-tool count verification.

- [ ] **Step 4: Commit**

```bash
git add src/mcp/tools.rs src/mcp/mod.rs
git commit -m "feat(mcp): add tool definitions and dispatch for 7 MCP tools"
```

---

### Task 7: MCP Main Loop (stdio JSON-RPC)

**Files:**
- Modify: `src/mcp/main.rs`

This is where it all comes together. The main loop reads JSON-RPC requests from stdin, dispatches them, and writes responses to stdout.

- [ ] **Step 1: Write the main loop**

Replace `src/mcp/main.rs` with:

```rust
// src/mcp/main.rs
//
// glass-slipper-mcp: MCP server bridging Claude Code to a local llama-server.
// Reads JSON-RPC on stdin, dispatches tool calls to llama-server over HTTP,
// writes JSON-RPC responses to stdout.

mod protocol;
mod prompts;
mod logger;
mod llm_client;
mod handlers;
mod tools;

use protocol::{JsonRpcRequest, JsonRpcResponse};
use logger::ActivityLogger;
use llm_client::McpLlmClient;
use std::io::{self, BufRead, Write};

const LLAMA_SERVER_URL: &str = "http://127.0.0.1:8080";
const MODEL_NAME: &str = "local";

#[tokio::main]
async fn main() {
    let base_url = std::env::var("GLASS_SLIPPER_URL")
        .unwrap_or_else(|_| LLAMA_SERVER_URL.to_string());
    let model = std::env::var("GLASS_SLIPPER_MODEL")
        .unwrap_or_else(|_| MODEL_NAME.to_string());

    let client = McpLlmClient::new(&base_url, &model);
    let logger = ActivityLogger::new();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e));
                write_response(&mut stdout, &resp);
                continue;
            }
        };

        let response = handle_request(&request, &client, &logger).await;
        write_response(&mut stdout, &response);
    }
}

async fn handle_request(
    req: &JsonRpcRequest,
    client: &McpLlmClient,
    logger: &ActivityLogger,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => {
            JsonRpcResponse::success(req.id.clone(), serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "glass-slipper-mcp",
                    "version": "0.1.0"
                }
            }))
        }

        "notifications/initialized" => {
            // Client acknowledgement — no response needed for notifications.
            // But we still return a response since our loop always writes one.
            // Use a null id to signal "no response" and skip writing.
            JsonRpcResponse::success(None, serde_json::json!(null))
        }

        "tools/list" => {
            JsonRpcResponse::success(req.id.clone(), tools::tool_definitions())
        }

        "tools/call" => {
            let tool_name = req.params.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = req.params.get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            let result = tools::dispatch(tool_name, &arguments, client, logger).await;
            JsonRpcResponse::success(req.id.clone(), serde_json::to_value(&result).unwrap())
        }

        _ => {
            JsonRpcResponse::error(req.id.clone(), -32601, format!("Method not found: {}", req.method))
        }
    }
}

fn write_response(stdout: &mut io::Stdout, resp: &JsonRpcResponse) {
    // Don't write responses for notifications (no id).
    if resp.id.is_none() && resp.result == Some(serde_json::json!(null)) {
        return;
    }

    if let Ok(json) = serde_json::to_string(resp) {
        let _ = writeln!(stdout, "{}", json);
        let _ = stdout.flush();
    }
}
```

- [ ] **Step 2: Build the MCP binary**

Run: `cargo build --bin glass-slipper-mcp`
Expected: Compiles successfully.

- [ ] **Step 3: Smoke test with stdin**

Run:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}' | cargo run --bin glass-slipper-mcp 2>/dev/null
```

Expected: JSON response with `protocolVersion`, `capabilities.tools`, and `serverInfo`.

- [ ] **Step 4: Test tools/list**

Run:

```bash
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}\n{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n' | cargo run --bin glass-slipper-mcp 2>/dev/null
```

Expected: Second response contains 7 tool definitions.

- [ ] **Step 5: Commit**

```bash
git add src/mcp/main.rs
git commit -m "feat(mcp): wire up main loop with stdio JSON-RPC dispatch"
```

---

### Task 8: Cargo.toml Binary Target Configuration

**Files:**
- Modify: `Cargo.toml`

The MCP binary should share dependencies with the main binary but not pull in modules it doesn't need (like crossterm, clap, nix, yah-core).

- [ ] **Step 1: Add the second binary target**

In `Cargo.toml`, after the existing `[[bin]]` section, add:

```toml
[[bin]]
name = "glass-slipper-mcp"
path = "src/mcp/main.rs"
```

Note: The MCP binary uses `reqwest`, `serde`, `serde_json`, and `tokio` — all already in `[dependencies]`. It does NOT use `crossterm`, `clap`, `nix`, `sysinfo`, or `yah-core`, but Cargo compiles all dependencies for all binaries in the same workspace. This is fine — unused crates just add to compile time, not binary size (the linker strips them). If compile time becomes a problem later, split into a workspace with separate crates.

- [ ] **Step 2: Verify both binaries build**

Run: `cargo build`
Expected: Both `glass-slipper` and `glass-slipper-mcp` binaries compile.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat(mcp): add glass-slipper-mcp binary target to Cargo.toml"
```

---

### Task 9: MCP Installer (Swift)

**Files:**
- Create: `glass-slipper/MCPInstaller.swift`

- [ ] **Step 1: Write MCPInstaller**

Create `glass-slipper/MCPInstaller.swift`:

```swift
//
//  MCPInstaller.swift
//  Glass Slipper — one-click MCP config for Claude Code
//
//  Reads ~/.claude.json, adds/removes the glass-slipper MCP entry,
//  writes it back. Creates the file if it doesn't exist.
//

import Foundation

enum MCPInstaller {

    /// Path to the MCP binary inside the app bundle.
    static var mcpBinaryPath: String {
        Bundle.main.bundlePath + "/Contents/MacOS/glass-slipper-mcp"
    }

    /// Path to Claude Code config.
    static var claudeConfigPath: String {
        NSHomeDirectory() + "/.claude.json"
    }

    /// Check if MCP is already configured.
    static var isInstalled: Bool {
        guard let config = readConfig() else { return false }
        guard let servers = config["mcpServers"] as? [String: Any] else { return false }
        return servers["glass-slipper"] != nil
    }

    /// Install the MCP entry into ~/.claude.json.
    /// Returns nil on success, error message on failure.
    static func install() -> String? {
        var config = readConfig() ?? [String: Any]()

        var servers = config["mcpServers"] as? [String: Any] ?? [:]
        servers["glass-slipper"] = [
            "command": mcpBinaryPath
        ]
        config["mcpServers"] = servers

        return writeConfig(config)
    }

    /// Remove the MCP entry from ~/.claude.json.
    /// Returns nil on success, error message on failure.
    static func uninstall() -> String? {
        guard var config = readConfig() else { return nil } // nothing to remove
        guard var servers = config["mcpServers"] as? [String: Any] else { return nil }
        servers.removeValue(forKey: "glass-slipper")
        config["mcpServers"] = servers
        return writeConfig(config)
    }

    // MARK: - Private

    private static func readConfig() -> [String: Any]? {
        let path = claudeConfigPath
        guard FileManager.default.fileExists(atPath: path),
              let data = FileManager.default.contents(atPath: path),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        return json
    }

    private static func writeConfig(_ config: [String: Any]) -> String? {
        do {
            let data = try JSONSerialization.data(
                withJSONObject: config,
                options: [.prettyPrinted, .sortedKeys]
            )
            let path = claudeConfigPath
            try data.write(to: URL(fileURLWithPath: path))
            return nil
        } catch {
            return "Failed to write \(claudeConfigPath): \(error.localizedDescription)"
        }
    }
}
```

- [ ] **Step 2: Add file to Xcode project**

Add `MCPInstaller.swift` to the GlassSlipper target in Xcode. For a non-interactive approach, manually add the file reference to `project.pbxproj`, or open Xcode and drag the file in.

Simpler: the project likely uses a glob or folder reference. Check the build phase — if it compiles all `.swift` files in the directory, just creating the file is enough.

- [ ] **Step 3: Build to verify**

Run: `xcodebuild -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add glass-slipper/MCPInstaller.swift
git commit -m "feat(swift): add MCPInstaller for one-click Claude Code config"
```

---

### Task 10: JSONL Activity Log Reader (Swift)

**Files:**
- Create: `glass-slipper/MCPActivityLog.swift`

- [ ] **Step 1: Write the log reader**

Create `glass-slipper/MCPActivityLog.swift`:

```swift
//
//  MCPActivityLog.swift
//  Glass Slipper — reads mcp-activity.jsonl for the companion window
//
//  Polls the JSONL file for new lines. Parses entries and notifies
//  the delegate of new activity. Computes running totals.
//

import Foundation

/// One parsed entry from mcp-activity.jsonl.
struct MCPActivityEntry {
    let timestamp: String
    let tool: String
    let inputTokens: Int
    let outputTokens: Int
    let latencyMs: Int
    let estimatedCloudCostUSD: Double
    let model: String
}

/// Running totals for the savings dashboard.
struct MCPSavingsSummary {
    var totalCostSaved: Double = 0
    var totalTasksDelegated: Int = 0
    var totalTokensSaved: Int = 0
}

protocol MCPActivityLogDelegate: AnyObject {
    func activityLogDidUpdate(entries: [MCPActivityEntry], summary: MCPSavingsSummary)
}

final class MCPActivityLog {

    weak var delegate: MCPActivityLogDelegate?

    private var entries: [MCPActivityEntry] = []
    private var summary = MCPSavingsSummary()
    private var lastReadOffset: UInt64 = 0
    private var timer: Timer?

    private var logPath: String {
        NSHomeDirectory() + "/Library/Application Support/Glass Slipper/mcp-activity.jsonl"
    }

    /// Start polling for new entries every 2 seconds.
    func startPolling() {
        // Read existing entries on start
        readNewEntries()

        timer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { [weak self] _ in
            self?.readNewEntries()
        }
    }

    func stopPolling() {
        timer?.invalidate()
        timer = nil
    }

    private func readNewEntries() {
        guard FileManager.default.fileExists(atPath: logPath),
              let handle = FileHandle(forReadingAtPath: logPath) else { return }

        handle.seek(toFileOffset: lastReadOffset)
        let data = handle.readDataToEndOfFile()
        lastReadOffset = handle.offsetInFile
        handle.closeFile()

        guard !data.isEmpty,
              let text = String(data: data, encoding: .utf8) else { return }

        var newEntries = [MCPActivityEntry]()

        for line in text.components(separatedBy: "\n") where !line.isEmpty {
            guard let jsonData = line.data(using: .utf8),
                  let json = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any]
            else { continue }

            let entry = MCPActivityEntry(
                timestamp: json["ts"] as? String ?? "",
                tool: json["tool"] as? String ?? "",
                inputTokens: json["input_tokens"] as? Int ?? 0,
                outputTokens: json["output_tokens"] as? Int ?? 0,
                latencyMs: json["latency_ms"] as? Int ?? 0,
                estimatedCloudCostUSD: json["estimated_cloud_cost_usd"] as? Double ?? 0,
                model: json["model"] as? String ?? ""
            )

            newEntries.append(entry)
            summary.totalCostSaved += entry.estimatedCloudCostUSD
            summary.totalTasksDelegated += 1
            summary.totalTokensSaved += entry.inputTokens
        }

        if !newEntries.isEmpty {
            entries.append(contentsOf: newEntries)
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.delegate?.activityLogDidUpdate(entries: self.entries, summary: self.summary)
            }
        }
    }
}
```

- [ ] **Step 2: Build to verify**

Run: `xcodebuild -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add glass-slipper/MCPActivityLog.swift
git commit -m "feat(swift): add MCPActivityLog reader for companion window"
```

---

### Task 11: Companion Window Color Tokens

**Files:**
- Modify: `glass-slipper/CinderellaScaffold.swift`

- [ ] **Step 1: Add companion window color and typography tokens**

Add these tokens to the existing NSColor extension in `CinderellaScaffold.swift`, after the "Status pill — text" section (after line 100):

```swift
    // MCP Companion — savings
    static let savingsGreen       = NSColor(hex: 0x4ADE80)               // green-400 (big $ number)
    static let savingsGreenMuted  = NSColor(hex: 0xBBF7D0)               // green-200 (savings bg)
    static let companionBlue      = NSColor(hex: 0x60A5FA)               // blue-400 (delegated count)
    static let companionPurple    = NSColor(hex: 0xC084FC)               // purple-400 (tokens count)
    static let setupStepBg        = NSColor(hex: 0xF8FAFC)               // slate-50 (setup row bg)
    static let setupCheckmark     = NSColor(hex: 0x22C55E)               // green-500 (done checkmark)
    static let setupActionBg      = NSColor(hex: 0x3B82F6)               // blue-500 (Install button bg)
    static let setupActionFg      = NSColor(hex: 0xFFFFFF)               // white (Install button text)
```

- [ ] **Step 2: Build to verify**

Run: `xcodebuild -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add glass-slipper/CinderellaScaffold.swift
git commit -m "feat(swift): add companion window color tokens"
```

---

### Task 12: Companion Window Controller

**Files:**
- Create: `glass-slipper/CompanionWindowController.swift`
- Modify: `glass-slipper/AppDelegate.swift`

- [ ] **Step 1: Write CompanionWindowController**

Create `glass-slipper/CompanionWindowController.swift`:

```swift
//
//  CompanionWindowController.swift
//  Glass Slipper — Claude Code companion window
//
//  Setup checklist (first-run) → savings dashboard (daily use).
//  Reads mcp-activity.jsonl via MCPActivityLog for live stats.
//

import AppKit

final class CompanionWindowController: NSWindowController, MCPActivityLogDelegate {

    private let activityLog = MCPActivityLog()

    // Setup views
    private var setupStack: NSStackView!
    private var modelRow: SetupRow!
    private var serverRow: SetupRow!
    private var mcpRow: SetupRow!

    // Dashboard views
    private var dashboardStack: NSStackView!
    private var savedLabel: NSTextField!
    private var delegatedLabel: NSTextField!
    private var tokensSavedLabel: NSTextField!
    private var activityStack: NSStackView!
    private var statusLine: NSTextField!

    // State
    private var isSetupComplete: Bool {
        isModelDownloaded && isServerRunning && MCPInstaller.isInstalled
    }

    private var isModelDownloaded: Bool {
        guard let manifest = ModelDownloadManager.loadManifest(),
              let model = manifest.models.first(where: { $0.id == manifest.default_model })
        else { return false }
        return ModelDownloadManager(model: model).isModelPresent
    }

    private var isServerRunning: Bool {
        // Quick check: can we hit the health endpoint?
        let semaphore = DispatchSemaphore(value: 0)
        var healthy = false
        let url = URL(string: "http://127.0.0.1:8080/health")!
        URLSession.shared.dataTask(with: url) { _, response, _ in
            if let http = response as? HTTPURLResponse, http.statusCode == 200 {
                healthy = true
            }
            semaphore.signal()
        }.resume()
        _ = semaphore.wait(timeout: .now() + 2)
        return healthy
    }

    convenience init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 420, height: 480),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Glass Slipper — Claude Companion"
        window.minSize = NSSize(width: 360, height: 320)
        window.center()

        self.init(window: window)
        buildUI()
        refreshState()
        activityLog.delegate = self
        activityLog.startPolling()
    }

    // MARK: - UI Construction

    private func buildUI() {
        guard let contentView = window?.contentView else { return }
        contentView.wantsLayer = true
        contentView.layer?.backgroundColor = NSColor.surfacePrimary.cgColor

        // Setup stack
        modelRow = SetupRow(
            step: "1",
            title: "Model",
            detail: "Qwen 3.5 9B · Q5_K_M",
            actionTitle: "Download",
            action: { [weak self] in self?.handleModelDownload() }
        )
        serverRow = SetupRow(
            step: "2",
            title: "Server",
            detail: "llama-server · Port 8080",
            actionTitle: "Start",
            action: { [weak self] in self?.handleServerStart() }
        )
        mcpRow = SetupRow(
            step: "3",
            title: "Claude Code MCP",
            detail: "Not configured",
            actionTitle: "Install",
            action: { [weak self] in self?.handleMCPInstall() }
        )

        setupStack = NSStackView(views: [modelRow, serverRow, mcpRow])
        setupStack.orientation = .vertical
        setupStack.spacing = Spacing.md
        setupStack.translatesAutoresizingMaskIntoConstraints = false

        // Dashboard views
        savedLabel = makeStatLabel(value: "$0.00", subtitle: "saved today", color: .savingsGreen)
        delegatedLabel = makeStatLabel(value: "0", subtitle: "delegated", color: .companionBlue)
        tokensSavedLabel = makeStatLabel(value: "0", subtitle: "tokens saved", color: .companionPurple)

        let statsRow = NSStackView(views: [savedLabel, delegatedLabel, tokensSavedLabel])
        statsRow.distribution = .fillEqually
        statsRow.translatesAutoresizingMaskIntoConstraints = false

        // Activity log
        let activityHeader = NSTextField(labelWithString: "ACTIVITY")
        activityHeader.font = .sectionHeader
        activityHeader.textColor = .textQuiet
        activityHeader.translatesAutoresizingMaskIntoConstraints = false

        activityStack = NSStackView()
        activityStack.orientation = .vertical
        activityStack.spacing = 2
        activityStack.translatesAutoresizingMaskIntoConstraints = false

        let scrollView = NSScrollView()
        scrollView.documentView = activityStack
        scrollView.hasVerticalScroller = true
        scrollView.translatesAutoresizingMaskIntoConstraints = false

        // Status line (collapsed setup)
        statusLine = NSTextField(labelWithString: "")
        statusLine.font = .detailText
        statusLine.textColor = .textSecondary
        statusLine.translatesAutoresizingMaskIntoConstraints = false
        statusLine.isHidden = true

        dashboardStack = NSStackView(views: [statusLine, statsRow, activityHeader, scrollView])
        dashboardStack.orientation = .vertical
        dashboardStack.spacing = Spacing.lg
        dashboardStack.translatesAutoresizingMaskIntoConstraints = false

        contentView.addSubview(setupStack)
        contentView.addSubview(dashboardStack)

        NSLayoutConstraint.activate([
            setupStack.topAnchor.constraint(equalTo: contentView.topAnchor, constant: Spacing.xxl),
            setupStack.leadingAnchor.constraint(equalTo: contentView.leadingAnchor, constant: Spacing.xl),
            setupStack.trailingAnchor.constraint(equalTo: contentView.trailingAnchor, constant: -Spacing.xl),

            dashboardStack.topAnchor.constraint(equalTo: contentView.topAnchor, constant: Spacing.xxl),
            dashboardStack.leadingAnchor.constraint(equalTo: contentView.leadingAnchor, constant: Spacing.xl),
            dashboardStack.trailingAnchor.constraint(equalTo: contentView.trailingAnchor, constant: -Spacing.xl),
            dashboardStack.bottomAnchor.constraint(equalTo: contentView.bottomAnchor, constant: -Spacing.xl),

            scrollView.heightAnchor.constraint(greaterThanOrEqualToConstant: 200),
        ])
    }

    // MARK: - State

    private func refreshState() {
        let modelOK = isModelDownloaded
        let serverOK = isServerRunning
        let mcpOK = MCPInstaller.isInstalled

        modelRow.setComplete(modelOK)
        serverRow.setComplete(serverOK)
        mcpRow.setComplete(mcpOK)
        if mcpOK {
            mcpRow.updateDetail("Configured")
        }

        let allDone = modelOK && serverOK && mcpOK
        setupStack.isHidden = allDone
        dashboardStack.isHidden = !allDone

        if allDone {
            statusLine.isHidden = false
            statusLine.stringValue = "● Qwen 3.5 9B · Running · MCP Connected"
            statusLine.textColor = .setupCheckmark
        }
    }

    // MARK: - Actions

    private func handleModelDownload() {
        // Delegate to the existing ModelDownloadManager flow.
        // For now, just refresh state — the main window handles downloads.
        refreshState()
    }

    private func handleServerStart() {
        // The main app manages llama-server. For now, just refresh.
        refreshState()
    }

    private func handleMCPInstall() {
        if let error = MCPInstaller.install() {
            let alert = NSAlert()
            alert.messageText = "MCP Install Failed"
            alert.informativeText = error
            alert.runModal()
        }
        refreshState()
    }

    // MARK: - MCPActivityLogDelegate

    func activityLogDidUpdate(entries: [MCPActivityEntry], summary: MCPSavingsSummary) {
        savedLabel.stringValue = String(format: "$%.2f", summary.totalCostSaved)
        delegatedLabel.stringValue = "\(summary.totalTasksDelegated)"

        let tokenStr: String
        if summary.totalTokensSaved >= 1000 {
            tokenStr = String(format: "%.1fk", Double(summary.totalTokensSaved) / 1000.0)
        } else {
            tokenStr = "\(summary.totalTokensSaved)"
        }
        tokensSavedLabel.stringValue = tokenStr

        // Update activity feed (show last 50 entries, newest first)
        let recentEntries = entries.suffix(50).reversed()
        activityStack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for entry in recentEntries {
            let row = makeActivityRow(entry: entry)
            activityStack.addArrangedSubview(row)
        }
    }

    // MARK: - Helpers

    private func makeStatLabel(value: String, subtitle: String, color: NSColor) -> NSTextField {
        let field = NSTextField(labelWithString: value)
        field.font = .systemFont(ofSize: 24, weight: .bold)
        field.textColor = color
        field.alignment = .center
        field.translatesAutoresizingMaskIntoConstraints = false
        return field
    }

    private func makeActivityRow(entry: MCPActivityEntry) -> NSView {
        let container = NSView()
        container.translatesAutoresizingMaskIntoConstraints = false

        let toolLabel = NSTextField(labelWithString: entry.tool)
        toolLabel.font = .detailText
        toolLabel.textColor = .textPrimary
        toolLabel.translatesAutoresizingMaskIntoConstraints = false

        let costLabel = NSTextField(labelWithString: String(format: "$%.3f", entry.estimatedCloudCostUSD))
        costLabel.font = .detailText
        costLabel.textColor = .savingsGreen
        costLabel.alignment = .right
        costLabel.translatesAutoresizingMaskIntoConstraints = false

        let tokenLabel = NSTextField(labelWithString: "\(entry.inputTokens)→\(entry.outputTokens) tok")
        tokenLabel.font = .systemFont(ofSize: 10)
        tokenLabel.textColor = .textQuiet
        tokenLabel.translatesAutoresizingMaskIntoConstraints = false

        container.addSubview(toolLabel)
        container.addSubview(costLabel)
        container.addSubview(tokenLabel)

        NSLayoutConstraint.activate([
            toolLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            toolLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 2),
            tokenLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            tokenLabel.topAnchor.constraint(equalTo: toolLabel.bottomAnchor),
            tokenLabel.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -2),
            costLabel.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            costLabel.centerYAnchor.constraint(equalTo: container.centerYAnchor),
        ])

        return container
    }
}

// MARK: - SetupRow

final class SetupRow: NSView {
    private let stepLabel = NSTextField(labelWithString: "")
    private let titleLabel = NSTextField(labelWithString: "")
    private let detailLabel = NSTextField(labelWithString: "")
    private let actionButton: NSButton
    private let checkmark = NSTextField(labelWithString: "✓")
    private var onAction: (() -> Void)?

    init(step: String, title: String, detail: String, actionTitle: String, action: @escaping () -> Void) {
        self.actionButton = NSButton(title: actionTitle, target: nil, action: nil)
        self.onAction = action
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.setupStepBg.cgColor
        layer?.cornerRadius = 6
        translatesAutoresizingMaskIntoConstraints = false

        stepLabel.stringValue = step
        stepLabel.font = .sectionHeader
        stepLabel.textColor = .textQuiet
        stepLabel.translatesAutoresizingMaskIntoConstraints = false

        titleLabel.stringValue = title
        titleLabel.font = .cardTitle
        titleLabel.textColor = .textPrimary
        titleLabel.translatesAutoresizingMaskIntoConstraints = false

        detailLabel.stringValue = detail
        detailLabel.font = .detailText
        detailLabel.textColor = .textSecondary
        detailLabel.translatesAutoresizingMaskIntoConstraints = false

        actionButton.bezelStyle = .rounded
        actionButton.target = self
        actionButton.action = #selector(buttonClicked)
        actionButton.translatesAutoresizingMaskIntoConstraints = false

        checkmark.font = .systemFont(ofSize: 16, weight: .bold)
        checkmark.textColor = .setupCheckmark
        checkmark.translatesAutoresizingMaskIntoConstraints = false
        checkmark.isHidden = true

        addSubview(stepLabel)
        addSubview(titleLabel)
        addSubview(detailLabel)
        addSubview(actionButton)
        addSubview(checkmark)

        NSLayoutConstraint.activate([
            heightAnchor.constraint(greaterThanOrEqualToConstant: 48),

            stepLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.lg),
            stepLabel.centerYAnchor.constraint(equalTo: centerYAnchor),

            titleLabel.leadingAnchor.constraint(equalTo: stepLabel.trailingAnchor, constant: Spacing.md),
            titleLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.md),

            detailLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            detailLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 2),
            detailLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.md),

            actionButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.lg),
            actionButton.centerYAnchor.constraint(equalTo: centerYAnchor),

            checkmark.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.lg),
            checkmark.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) { fatalError("not in IB") }

    func setComplete(_ complete: Bool) {
        actionButton.isHidden = complete
        checkmark.isHidden = !complete
    }

    func updateDetail(_ text: String) {
        detailLabel.stringValue = text
    }

    @objc private func buttonClicked() {
        onAction?()
    }
}
```

- [ ] **Step 2: Add companion window to AppDelegate**

In `glass-slipper/AppDelegate.swift`, add a property and menu item. Add this property near the top of the class:

```swift
private var companionWindowController: CompanionWindowController?
```

In `setupMenuBar()`, add a menu item after the Edit menu:

```swift
// Window menu — companion window
let windowMenuItem = NSMenuItem()
menubar.addItem(windowMenuItem)
let windowMenu = NSMenu(title: "Window")
windowMenu.addItem(withTitle: "Claude Companion", action: #selector(showCompanionWindow), keyEquivalent: "2")
windowMenuItem.submenu = windowMenu
```

Add the action method to AppDelegate:

```swift
@objc private func showCompanionWindow() {
    if companionWindowController == nil {
        companionWindowController = CompanionWindowController()
    }
    companionWindowController?.showWindow(nil)
    companionWindowController?.window?.makeKeyAndOrderFront(nil)
}
```

- [ ] **Step 3: Build to verify**

Run: `xcodebuild -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add glass-slipper/CompanionWindowController.swift glass-slipper/AppDelegate.swift
git commit -m "feat(swift): add companion window with setup checklist and savings dashboard"
```

---

### Task 13: Integration Smoke Test

**Files:** (no new files)

End-to-end verification that the MCP binary works with Claude Code.

- [ ] **Step 1: Build the MCP binary in release mode**

Run: `cargo build --release --bin glass-slipper-mcp`
Expected: Compiles. Binary at `target/release/glass-slipper-mcp`.

- [ ] **Step 2: Test initialize + tools/list handshake**

Run:

```bash
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}\n{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n' | ./target/release/glass-slipper-mcp 2>/dev/null | head -2
```

Expected: Two JSON lines — initialize response and tools/list response with 7 tools.

- [ ] **Step 3: Test local_status (no llama-server running)**

Run:

```bash
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"local_status","arguments":{}}}\n' | ./target/release/glass-slipper-mcp 2>/dev/null
```

Expected: Second response contains `"unreachable"` since llama-server isn't running.

- [ ] **Step 4: Test local_summarize with a simple command**

Start llama-server first (or use the Glass Slipper app), then:

```bash
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"local_summarize","arguments":{"command":"echo hello world"}}}\n' | GLASS_SLIPPER_URL=http://127.0.0.1:8787 ./target/release/glass-slipper-mcp 2>/dev/null
```

Expected: Response contains a summary of "hello world" (if llama-server is running).

- [ ] **Step 5: Verify JSONL log was created**

Run: `cat ~/Library/Application\ Support/Glass\ Slipper/mcp-activity.jsonl`
Expected: At least one JSONL line with tool name, token counts, and cost.

- [ ] **Step 6: Commit (if any fixes were needed)**

```bash
git add -u
git commit -m "fix(mcp): integration test fixes"
```

---

### Task 14: Xcode Build Phase for MCP Binary

**Files:**
- Modify: `glass-slipper/GlassSlipper.xcodeproj/project.pbxproj`

The MCP binary needs to be compiled by cargo and copied into the app bundle during xcodebuild.

- [ ] **Step 1: Check existing cargo build phase**

Read the existing Xcode project to see how `glass-slipper` (the main Rust binary) is built. There's likely already a "Run Script" build phase that runs `cargo build`. The MCP binary target just needs to be added to the same cargo command.

Run: Look for "cargo" in `project.pbxproj`:

```bash
grep -n "cargo" glass-slipper/GlassSlipper.xcodeproj/project.pbxproj
```

- [ ] **Step 2: Add glass-slipper-mcp to the build phase**

If there's an existing cargo build script phase, modify it to also build `glass-slipper-mcp`:

Change: `cargo build --release --bin glass-slipper`
To: `cargo build --release --bin glass-slipper --bin glass-slipper-mcp`

And add a copy step:

```bash
cp "${CARGO_TARGET_DIR:-../target}/release/glass-slipper-mcp" "${BUILT_PRODUCTS_DIR}/${PRODUCT_NAME}.app/Contents/MacOS/"
```

If there's no existing cargo build phase, create a new "Run Script" build phase in Xcode with the full cargo build + copy commands.

- [ ] **Step 3: Build to verify**

Run: `xcodebuild -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED. MCP binary exists at `DerivedData/.../GlassSlipper.app/Contents/MacOS/glass-slipper-mcp`.

- [ ] **Step 4: Commit**

```bash
git add glass-slipper/GlassSlipper.xcodeproj/
git commit -m "feat(xcode): add glass-slipper-mcp to app bundle build phase"
```

---

### Task 15: System Prompt Tuning

**Files:**
- Modify: `src/mcp/prompts.rs`

The initial prompts from Task 2 are a starting point. This task iterates on them using real inputs.

- [ ] **Step 1: Test the summarize prompt against real xcodebuild output**

Capture actual xcodebuild output:

```bash
xcodebuild -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 > /tmp/xcodebuild-output.txt
```

Then feed it to the local model with the current summarize prompt. Evaluate the response: is it concise? Does it capture pass/fail? Iterate on the prompt text in `src/mcp/prompts.rs` until the output is good.

- [ ] **Step 2: Test the explain prompt against real code**

Pick a function from the codebase (e.g., `ServerManager::swap_model` from `src/server.rs`), feed it to the local model with the explain prompt, evaluate quality.

- [ ] **Step 3: Test the web_fetch prompt**

Fetch a real web page (e.g., the llama.cpp README on GitHub), ask a specific question, verify the answer is accurate and concise.

- [ ] **Step 4: Update prompts based on findings**

Edit `src/mcp/prompts.rs` with refined prompt text. The specifics depend on what you observe — common fixes include:
- Adding "Do not include markdown code fences in your response"
- Adding "If the build succeeded, just say BUILD SUCCEEDED — do not restate zero errors"
- Adding "Maximum 3 sentences" for summarize
- Adding "Do not start with 'This code...'" for explain

- [ ] **Step 5: Run tests and commit**

Run: `cargo test --bin glass-slipper-mcp`

```bash
git add src/mcp/prompts.rs
git commit -m "refine(mcp): tune system prompts based on real-world testing"
```
