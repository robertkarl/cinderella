// src/mcp/handlers.rs

use super::llm_client::McpLlmClient;
use super::logger::ActivityLogger;
use super::prompts;
use super::protocol::ToolResult;
use std::time::Instant;

const SHORT_MAX_TOKENS: u32 = 512;
const LONG_MAX_TOKENS: u32 = 2048;

pub async fn handle_summarize(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    command: &str,
) -> ToolResult {
    let start = Instant::now();
    let output = match run_command(command).await {
        Ok(out) => out,
        Err(e) => return ToolResult::error(format!("Failed to run command: {}", e)),
    };
    if output.is_empty() {
        logger.log("local_summarize", 0, 0, start, client.model_name());
        return ToolResult::text("Command produced no output.");
    }
    // Truncate to avoid exceeding local model's context window.
    // Keep first 6000 chars + last 2000 chars so we see the start and the final result.
    let truncated = if output.len() > 10000 {
        let head = &output[..6000];
        let tail = &output[output.len() - 2000..];
        format!("{}\n\n[... {} chars truncated ...]\n\n{}", head, output.len() - 8000, tail)
    } else {
        output
    };
    complete_and_log(client, logger, "local_summarize", prompts::SUMMARIZE, &truncated, SHORT_MAX_TOKENS, start).await
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
