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
    let detail = slug_command(command);
    let output = match run_command(command).await {
        Ok(out) => out,
        Err(e) => return ToolResult::error(format!("Failed to run command: {}", e)),
    };
    if output.is_empty() {
        logger.log("local_summarize", &detail, 0, 0, start, client.model_name());
        return ToolResult::text("Command produced no output.");
    }
    complete_and_log(client, logger, "local_summarize", &detail, prompts::SUMMARIZE, &output, SHORT_MAX_TOKENS, start).await
}

pub async fn handle_explain(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    code: &str,
) -> ToolResult {
    let start = Instant::now();
    let detail = slug_first_line(code, 60);
    complete_and_log(client, logger, "local_explain", &detail, prompts::EXPLAIN, code, SHORT_MAX_TOKENS, start).await
}

pub async fn handle_ask(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    question: &str,
    context: Option<&str>,
) -> ToolResult {
    let start = Instant::now();
    let detail = truncate(question, 60);
    let user_content = match context {
        Some(ctx) => format!("Context:\n{}\n\nQuestion: {}", ctx, question),
        None => question.to_string(),
    };
    complete_and_log(client, logger, "local_ask", &detail, prompts::ASK, &user_content, SHORT_MAX_TOKENS, start).await
}

pub async fn handle_web_fetch(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    url: &str,
    question: &str,
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
    complete_and_log(client, logger, "local_web_fetch", &detail, prompts::WEB_FETCH, &user_content, SHORT_MAX_TOKENS, start).await
}

pub async fn handle_review(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    diff: &str,
) -> ToolResult {
    let start = Instant::now();
    let detail = slug_diff(diff);
    complete_and_log(client, logger, "local_review", &detail, prompts::REVIEW, diff, SHORT_MAX_TOKENS, start).await
}

pub async fn handle_draft(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    task: &str,
    context: Option<&str>,
) -> ToolResult {
    let start = Instant::now();
    let detail = truncate(task, 60);
    let user_content = match context {
        Some(ctx) => format!("Context:\n{}\n\nTask: {}", ctx, task),
        None => task.to_string(),
    };
    complete_and_log(client, logger, "local_draft", &detail, prompts::DRAFT, &user_content, LONG_MAX_TOKENS, start).await
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
    if !output.status.success() {
        combined.push_str(&format!("\n[exit code: {}]", output.status.code().unwrap_or(-1)));
    }
    Ok(combined)
}

async fn complete_and_log(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    tool_name: &str,
    detail: &str,
    system_prompt: &str,
    user_content: &str,
    max_tokens: u32,
    start: Instant,
) -> ToolResult {
    match client.complete(system_prompt, user_content, max_tokens).await {
        Ok(result) => {
            logger.log(tool_name, detail, result.input_tokens, result.output_tokens, start, client.model_name());
            ToolResult::text(result.text)
        }
        Err(e) => ToolResult::error(format!("LLM call failed: {}", e)),
    }
}

// -- Slug generators --

/// Extract a readable slug from a shell command.
/// "xcodebuild -project foo/Bar.xcodeproj -scheme Bar build 2>&1" → "xcodebuild"
/// "cargo build --release" → "cargo build"
/// "bash /path/to/noisy-build.sh" → "noisy-build.sh"
fn slug_command(cmd: &str) -> String {
    let cmd = cmd.trim();
    // If it starts with "bash" or "sh" + a path, use the filename
    if let Some(rest) = cmd.strip_prefix("bash ").or_else(|| cmd.strip_prefix("sh ")) {
        let path = rest.split_whitespace().next().unwrap_or(rest);
        if let Some(name) = path.rsplit('/').next() {
            return name.to_string();
        }
    }
    // Otherwise take the first two words (e.g. "cargo build", "npm test")
    let words: Vec<&str> = cmd.split_whitespace().collect();
    match words.len() {
        0 => "(empty)".to_string(),
        1 => words[0].to_string(),
        _ => format!("{} {}", words[0], words[1]),
    }
}

/// Extract a readable slug from a URL.
/// "https://docs.rs/serde/latest/serde/" → "docs.rs"
fn slug_url(url: &str) -> String {
    url.split("//")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
        .to_string()
}

/// Extract a slug from a diff — count files changed.
fn slug_diff(diff: &str) -> String {
    let file_count = diff.lines()
        .filter(|l| l.starts_with("diff --git") || l.starts_with("--- a/") || l.starts_with("+++ b/"))
        .filter(|l| l.starts_with("diff --git"))
        .count();
    if file_count == 0 {
        "diff".to_string()
    } else {
        format!("{} file{}", file_count, if file_count == 1 { "" } else { "s" })
    }
}

/// First non-empty line of code, truncated.
fn slug_first_line(text: &str, max: usize) -> String {
    let line = text.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("(code)");
    truncate(line.trim(), max)
}

/// Truncate a string to max chars, adding "..." if needed.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
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

    #[test]
    fn test_slug_command() {
        assert_eq!(slug_command("cargo build --release"), "cargo build");
        assert_eq!(slug_command("xcodebuild -project foo build"), "xcodebuild -project");
        assert_eq!(slug_command("bash /path/to/noisy-build.sh"), "noisy-build.sh");
        assert_eq!(slug_command("npm test"), "npm test");
        assert_eq!(slug_command("echo hello"), "echo hello");
    }

    #[test]
    fn test_slug_url() {
        assert_eq!(slug_url("https://docs.rs/serde/latest/"), "docs.rs");
        assert_eq!(slug_url("http://localhost:8787/health"), "localhost:8787");
    }

    #[test]
    fn test_slug_diff() {
        assert_eq!(slug_diff("diff --git a/foo.rs b/foo.rs\n+hello"), "1 file");
        assert_eq!(slug_diff("diff --git a/a.rs b/a.rs\ndiff --git a/b.rs b/b.rs\n"), "2 files");
        assert_eq!(slug_diff("just some text"), "diff");
    }

    #[test]
    fn test_slug_first_line() {
        assert_eq!(slug_first_line("fn main() {\n    println!(\"hi\");\n}", 20), "fn main() {");
        assert_eq!(slug_first_line("\n\n  pub struct Foo {", 60), "pub struct Foo {");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("this is a long string", 10), "this is...");
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
