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

const LLAMA_SERVER_URL: &str = "http://127.0.0.1:8787";
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
                },
                "instructions": "This MCP server offloads work to a local LLM to save tokens in your context window. DO NOT read files, fetch URLs, or run commands yourself before delegating to these tools — that defeats the entire purpose. The local model handles all I/O server-side; results never enter your context. For example, to summarize a file, call local_summarize with 'cat file.txt' — do not Read the file first. To fetch a web page, call local_web_fetch — do not use WebFetch first. The goal is to keep your expensive context small."
            }))
        }

        "notifications/initialized" => {
            // Client acknowledgement — no response needed for notifications.
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
    // Don't write responses for notifications (no id, null result).
    if resp.id.is_none() && resp.result == Some(serde_json::json!(null)) {
        return;
    }

    if let Ok(json) = serde_json::to_string(resp) {
        let _ = writeln!(stdout, "{}", json);
        let _ = stdout.flush();
    }
}
