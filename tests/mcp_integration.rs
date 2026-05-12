//! Integration tests for glass-slipper-mcp against a real llama-server.
//!
//! Run with: cargo test --features integration -- --test-threads=1
//! Requires a GGUF model and llama-server installed.
//! Startup takes ~2 minutes for the model to load.

#![cfg(feature = "integration")]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Test server (shared across all tests via LazyLock)
// ---------------------------------------------------------------------------

struct TestServer {
    port: u16,
    child: Option<Child>,
}

impl TestServer {
    fn start() -> Self {
        let model_path = Self::find_model();
        let llama_bin = Self::find_llama_server();
        let port = Self::pick_free_port();

        eprintln!(
            "[test-harness] starting llama-server on port {} with model {}",
            port,
            model_path.display()
        );

        let child = Command::new(&llama_bin)
            .args([
                "--model",
                model_path.to_str().unwrap(),
                "--port",
                &port.to_string(),
                "--ctx-size",
                "4096",
                "--n-gpu-layers",
                "-1",
                "--jinja",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| {
                panic!(
                    "failed to spawn llama-server at {}: {}",
                    llama_bin.display(),
                    e
                )
            });

        let server = TestServer {
            port,
            child: Some(child),
        };
        server.wait_for_health();
        server
    }

    fn find_model() -> PathBuf {
        let candidates = [
            dirs::home_dir()
                .unwrap()
                .join("Library/Application Support/Glass Slipper/Models/Qwen3.5-9B-Q5_K_M.gguf"),
            dirs::home_dir()
                .unwrap()
                .join("models/Qwen3.5-9B-Q5_K_M.gguf"),
        ];
        for p in &candidates {
            if p.exists() {
                return p.clone();
            }
        }
        panic!(
            "model not found; looked in: {:?}",
            candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()
        );
    }

    fn find_llama_server() -> PathBuf {
        let candidates = [
            PathBuf::from("/opt/homebrew/bin/llama-server"),
            PathBuf::from("/usr/local/bin/llama-server"),
        ];
        for p in &candidates {
            if p.exists() {
                return p.clone();
            }
        }
        panic!(
            "llama-server not found; looked in: {:?}",
            candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()
        );
    }

    fn pick_free_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind ephemeral port");
        listener.local_addr().unwrap().port()
    }

    fn wait_for_health(&self) {
        let url = format!("http://127.0.0.1:{}/health", self.port);
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    eprintln!("[test-harness] llama-server healthy on port {}", self.port);
                    return;
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        panic!(
            "llama-server did not become healthy within 120s on port {}",
            self.port
        );
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            eprintln!("[test-harness] llama-server killed");
        }
    }
}

// A dummy home-dir helper since we can't add crate deps from tests easily.
// We inline the logic rather than pulling in the `dirs` crate.
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

static SERVER: LazyLock<TestServer> = LazyLock::new(TestServer::start);

// ---------------------------------------------------------------------------
// MCP binary helpers
// ---------------------------------------------------------------------------

fn find_mcp_binary() -> PathBuf {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        project_root.join("target/debug/glass-slipper-mcp"),
        project_root.join("target/release/glass-slipper-mcp"),
    ];
    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }
    panic!(
        "glass-slipper-mcp binary not found; run `cargo build` first. Looked in: {:?}",
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
    );
}

/// Send a single JSON-RPC request to the MCP binary and return the parsed response.
fn mcp_call(method: &str, params: serde_json::Value) -> serde_json::Value {
    let server = &*SERVER; // ensure server is running
    let mcp_bin = find_mcp_binary();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let mut child = Command::new(&mcp_bin)
        .env("GLASS_SLIPPER_URL", server.base_url())
        .env("GLASS_SLIPPER_MODEL", "local")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn glass-slipper-mcp: {}", e));

    // Write the JSON-RPC request then close stdin.
    {
        let mut stdin = child.stdin.take().expect("failed to open stdin");
        writeln!(stdin, "{}", serde_json::to_string(&request).unwrap()).unwrap();
        // stdin drops here, closing the pipe
    }

    // Read the first response line from stdout.
    let stdout = child.stdout.take().expect("failed to open stdout");
    let reader = BufReader::new(stdout);
    let response_line = reader
        .lines()
        .next()
        .expect("no response from MCP binary")
        .expect("failed to read response line");

    let _ = child.kill();
    let _ = child.wait();

    serde_json::from_str(&response_line)
        .unwrap_or_else(|e| panic!("failed to parse MCP response JSON: {}\nraw: {}", e, response_line))
}

/// Convenience wrapper: call a tool and extract the text from the first content block.
fn tool_call(tool_name: &str, arguments: serde_json::Value) -> String {
    let resp = mcp_call(
        "tools/call",
        serde_json::json!({
            "name": tool_name,
            "arguments": arguments,
        }),
    );

    resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "tool_call({}) did not return text in result.content[0].text; full response: {}",
                tool_name,
                serde_json::to_string_pretty(&resp).unwrap()
            )
        })
        .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let resp = mcp_call(
        "initialize",
        serde_json::json!({"protocolVersion": "2024-11-05"}),
    );
    let result = &resp["result"];
    assert_eq!(
        result["protocolVersion"].as_str().unwrap(),
        "2024-11-05",
        "unexpected protocolVersion"
    );
    assert!(
        result["serverInfo"]["name"].as_str().is_some(),
        "serverInfo.name missing"
    );
    assert_eq!(
        result["serverInfo"]["name"].as_str().unwrap(),
        "glass-slipper-mcp"
    );
}

#[test]
fn test_tools_list_returns_seven_tools() {
    let resp = mcp_call("tools/list", serde_json::json!({}));
    let tools = resp["result"]["tools"]
        .as_array()
        .expect("tools/list did not return an array");
    assert_eq!(
        tools.len(),
        7,
        "expected 7 tools, got {}: {:?}",
        tools.len(),
        tools
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_status_reports_healthy() {
    let text = tool_call("local_status", serde_json::json!({}));
    let lower = text.to_lowercase();
    assert!(
        lower.contains("running") || lower.contains("healthy") || lower.contains("ok"),
        "local_status should report healthy/running/ok, got: {}",
        text
    );
}

#[test]
fn test_ask_simple_question() {
    let text = tool_call(
        "local_ask",
        serde_json::json!({"question": "What is the capital of France? Answer with just the city name."}),
    );
    let lower = text.to_lowercase();
    assert!(
        lower.contains("paris"),
        "expected 'Paris' in response, got: {}",
        text
    );
}

/// KEY QA TEST: catches the "Pass" bug where local_summarize returns garbage
/// instead of a real summary of the command output.
#[test]
fn test_summarize_not_garbage() {
    // Create a temp file with known content about Pikachu.
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let file_path = dir.path().join("pikachu.txt");
    std::fs::write(
        &file_path,
        "Pikachu is a yellow Electric-type Pokemon known for its lightning bolt shaped tail \
         and its signature move Thunderbolt. It is the mascot of the Pokemon franchise and \
         evolves from Pichu when leveled up with high friendship. Pikachu can further evolve \
         into Raichu when exposed to a Thunder Stone.",
    )
    .expect("failed to write temp file");

    let text = tool_call(
        "local_summarize",
        serde_json::json!({"command": format!("cat {}", file_path.display())}),
    );

    assert!(
        text.len() > 20,
        "summarize response too short ({} chars), likely garbage: {}",
        text.len(),
        text
    );

    let lower = text.to_lowercase();
    assert!(
        lower.contains("pikachu") || lower.contains("pokemon") || lower.contains("electric"),
        "summarize should mention pikachu/pokemon/electric, got: {}",
        text
    );
}

#[test]
fn test_explain_code_snippet() {
    let fib_code = r#"
def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)
"#;
    let text = tool_call("local_explain", serde_json::json!({"code": fib_code}));
    assert!(
        text.len() > 20,
        "explain response too short ({} chars): {}",
        text.len(),
        text
    );
    let lower = text.to_lowercase();
    assert!(
        lower.contains("fibonacci") || lower.contains("recursive") || lower.contains("sequence"),
        "explain should mention fibonacci/recursive/sequence, got: {}",
        text
    );
}

#[test]
fn test_draft_generates_code() {
    let text = tool_call(
        "local_draft",
        serde_json::json!({"task": "Write a Python function called greet that takes a name parameter and returns a greeting string"}),
    );
    let lower = text.to_lowercase();
    assert!(
        lower.contains("def") || lower.contains("greet"),
        "draft should contain 'def' or 'greet', got: {}",
        text
    );
}

#[test]
fn test_review_diff() {
    let sample_diff = r#"
diff --git a/main.py b/main.py
--- a/main.py
+++ b/main.py
@@ -1,5 +1,7 @@
 import os
+import sys

 def main():
-    print("hello")
+    name = sys.argv[1] if len(sys.argv) > 1 else "world"
+    print(f"hello, {name}")
"#;
    let text = tool_call("local_review", serde_json::json!({"diff": sample_diff}));
    assert!(
        text.len() > 20,
        "review response too short ({} chars): {}",
        text.len(),
        text
    );
}

#[test]
fn test_unknown_method_returns_error() {
    let resp = mcp_call("bogus/method", serde_json::json!({}));
    assert!(
        resp.get("error").is_some(),
        "expected error field for unknown method, got: {}",
        serde_json::to_string_pretty(&resp).unwrap()
    );
}

#[test]
fn test_unknown_tool_returns_error() {
    let text = tool_call("nonexistent_tool", serde_json::json!({}));
    let lower = text.to_lowercase();
    assert!(
        lower.contains("unknown"),
        "expected 'unknown' in error for nonexistent tool, got: {}",
        text
    );
}
