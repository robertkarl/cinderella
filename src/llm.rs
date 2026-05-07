/// OpenAI-compatible SSE streaming client for llama-server.
/// Text tokens stream live. Tool calls buffer until complete.
/// JSON repair pipeline for malformed tool calls.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Streaming response events.
pub enum StreamEvent {
    /// A chunk of text content.
    TextDelta(String),
    /// A chunk of thinking/reasoning content.
    ThinkingDelta(String),
    /// A complete tool call (buffered from stream).
    ToolCallComplete(ToolCall),
    /// Stream finished.
    Done,
    /// Token timing for tok/s calculation.
    TokenTick { tokens: u64, elapsed_ms: u64 },
}

/// SSE streaming chat completion against an OpenAI-compatible endpoint.
pub struct LlmClient {
    base_url: String,
    model_name: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct SseChunk {
    choices: Vec<SseChoice>,
}

#[derive(Deserialize)]
struct SseChoice {
    delta: SseDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct SseDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<SseToolCallDelta>>,
}

#[derive(Deserialize)]
struct SseToolCallDelta {
    index: Option<usize>,
    id: Option<String>,
    function: Option<SseFunctionDelta>,
}

#[derive(Deserialize)]
struct SseFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

impl LlmClient {
    pub fn new(base_url: &str, model_name: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            model_name: model_name.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Send a streaming chat completion request.
    /// Calls `on_event` for each streaming event.
    pub async fn chat_completion(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
        mut on_event: impl FnMut(StreamEvent),
    ) -> Result<Message> {
        let body = serde_json::json!({
            "model": self.model_name,
            "messages": messages,
            "tools": tools,
            "stream": true,
            "temperature": 0.1,
            "tool_choice": "auto",
        });

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await
            .context("Failed to connect to llama-server")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("llama-server returned {}: {}", status, body);
        }

        let mut stream = response.bytes_stream();
        let mut content = String::new();
        let mut tool_calls: Vec<ToolCallBuilder> = Vec::new();
        let mut token_count: u64 = 0;
        let start = Instant::now();
        let mut buffer = String::new();
        let mut done = false;

        while let Some(chunk) = stream.next().await {
            if done {
                break;
            }
            let chunk = chunk.context("Stream error")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            done = process_sse_buffer(
                &mut buffer,
                &mut content,
                &mut tool_calls,
                &mut token_count,
                start,
                &mut on_event,
            );
        }

        Ok(build_response_message(content, tool_calls))
    }
}

/// Classify a single SSE line.
enum SseLine<'a> {
    Skip,
    Done,
    Data(&'a str),
}

fn classify_sse_line(line: &str) -> SseLine<'_> {
    if line.is_empty() || line.starts_with(':') {
        return SseLine::Skip;
    }
    if line == "data: [DONE]" {
        return SseLine::Done;
    }
    if let Some(data) = line.strip_prefix("data: ") {
        return SseLine::Data(data);
    }
    SseLine::Skip
}

/// Process all complete SSE lines in the buffer. Returns true if stream is done.
fn process_sse_buffer(
    buffer: &mut String,
    content: &mut String,
    tool_calls: &mut Vec<ToolCallBuilder>,
    token_count: &mut u64,
    start: Instant,
    on_event: &mut impl FnMut(StreamEvent),
) -> bool {
    while let Some(pos) = buffer.find('\n') {
        let line = buffer[..pos].trim().to_string();
        *buffer = buffer[pos + 1..].to_string();

        match classify_sse_line(&line) {
            SseLine::Skip => continue,
            SseLine::Done => {
                on_event(StreamEvent::Done);
                return true;
            }
            SseLine::Data(data) => {
                if let Ok(chunk) = serde_json::from_str::<SseChunk>(data) {
                    for choice in &chunk.choices {
                        process_choice_delta(choice, content, tool_calls, token_count, start, on_event);
                    }
                }
            }
        }
    }
    false
}

/// Process a single SSE choice delta: text, thinking, tool calls, and finish.
fn process_choice_delta(
    choice: &SseChoice,
    content: &mut String,
    tool_calls: &mut Vec<ToolCallBuilder>,
    token_count: &mut u64,
    start: Instant,
    on_event: &mut impl FnMut(StreamEvent),
) {
    emit_thinking_delta(&choice.delta, token_count, on_event);

    if let Some(ref text) = choice.delta.content {
        emit_text_delta(text, content, token_count, start, on_event);
    }

    if let Some(ref tc_deltas) = choice.delta.tool_calls {
        for delta in tc_deltas {
            accumulate_tool_call_delta(tool_calls, delta);
        }
    }

    if choice.finish_reason.as_deref() == Some("tool_calls") {
        emit_completed_tool_calls(tool_calls, on_event);
    }
}

/// Emit a thinking delta if present and non-empty.
fn emit_thinking_delta(
    delta: &SseDelta,
    token_count: &mut u64,
    on_event: &mut impl FnMut(StreamEvent),
) {
    if let Some(ref text) = delta.reasoning_content {
        if !text.is_empty() {
            *token_count += 1;
            on_event(StreamEvent::ThinkingDelta(text.clone()));
        }
    }
}

/// Emit ToolCallComplete events for all built tool calls.
fn emit_completed_tool_calls(
    tool_calls: &[ToolCallBuilder],
    on_event: &mut impl FnMut(StreamEvent),
) {
    for (i, builder) in tool_calls.iter().enumerate() {
        if let Some(tc) = builder.build(i) {
            on_event(StreamEvent::ToolCallComplete(tc));
        }
    }
}

/// Buffer text content and emit TextDelta + TokenTick events.
fn emit_text_delta(
    text: &str,
    content: &mut String,
    token_count: &mut u64,
    start: Instant,
    on_event: &mut impl FnMut(StreamEvent),
) {
    if text.is_empty() {
        return;
    }
    content.push_str(text);
    *token_count += 1;
    on_event(StreamEvent::TextDelta(text.to_string()));

    let elapsed = start.elapsed().as_millis() as u64;
    if elapsed > 0 {
        on_event(StreamEvent::TokenTick {
            tokens: *token_count,
            elapsed_ms: elapsed,
        });
    }
}

/// Merge a single tool call delta into the builder vector.
fn accumulate_tool_call_delta(tool_calls: &mut Vec<ToolCallBuilder>, delta: &SseToolCallDelta) {
    let idx = delta.index.unwrap_or(0);
    while tool_calls.len() <= idx {
        tool_calls.push(ToolCallBuilder::default());
    }
    let builder = &mut tool_calls[idx];

    if let Some(ref id) = delta.id {
        builder.id = Some(id.clone());
    }
    if let Some(ref func) = delta.function {
        if let Some(ref name) = func.name {
            builder.name = Some(name.clone());
        }
        if let Some(ref args) = func.arguments {
            builder.arguments.push_str(args);
        }
    }
}

/// Assemble the final response Message from accumulated content and tool calls.
fn build_response_message(content: String, tool_calls: Vec<ToolCallBuilder>) -> Message {
    if !tool_calls.is_empty() {
        let built: Vec<ToolCall> = tool_calls
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.build(i))
            .collect();

        if !built.is_empty() {
            return Message {
                role: "assistant".to_string(),
                content: if content.is_empty() { None } else { Some(content) },
                tool_calls: Some(built),
                tool_call_id: None,
            };
        }
    }

    Message {
        role: "assistant".to_string(),
        content: Some(content),
        tool_calls: None,
        tool_call_id: None,
    }
}

#[derive(Default)]
struct ToolCallBuilder {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl ToolCallBuilder {
    fn build(&self, index: usize) -> Option<ToolCall> {
        let name = self.name.as_ref()?;
        let id = self
            .id
            .clone()
            .unwrap_or_else(|| format!("call_{}", index));

        // Attempt JSON repair on arguments
        let arguments = repair_json(&self.arguments);

        Some(ToolCall {
            id,
            call_type: "function".to_string(),
            function: FunctionCall {
                name: name.clone(),
                arguments,
            },
        })
    }
}

/// Attempt to repair common JSON issues from local models.
/// Fix trailing commas, missing quotes around keys, unescaped newlines.
pub fn repair_json(input: &str) -> String {
    let trimmed = input.trim();

    // If it already parses, return as-is
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return trimmed.to_string();
    }

    let mut result = trimmed.to_string();

    // Fix trailing commas before } or ]
    loop {
        let before = result.clone();
        result = result.replace(",}", "}").replace(",]", "]");
        // Also handle whitespace: ", }" etc.
        let re_obj = regex_lite_trailing_comma(&result, '}');
        let re_arr = regex_lite_trailing_comma(&re_obj, ']');
        result = re_arr;
        if result == before {
            break;
        }
    }

    // Fix unescaped newlines inside strings
    result = fix_unescaped_newlines(&result);

    // If still doesn't parse, try wrapping in {}
    if serde_json::from_str::<serde_json::Value>(&result).is_err() && !result.starts_with('{') {
        let wrapped = format!("{{{}}}", result);
        if serde_json::from_str::<serde_json::Value>(&wrapped).is_ok() {
            return wrapped;
        }
    }

    result
}

fn regex_lite_trailing_comma(input: &str, closer: char) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == ',' && is_followed_by_closer(&chars[i + 1..], closer) {
            i += 1;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Check if a char slice starts with optional whitespace then the closer char.
fn is_followed_by_closer(chars: &[char], closer: char) -> bool {
    let next_non_ws = chars.iter().find(|c| !c.is_whitespace());
    next_non_ws == Some(&closer)
}

fn fix_unescaped_newlines(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_string = false;
    let mut prev_was_backslash = false;

    for ch in input.chars() {
        if ch == '"' && !prev_was_backslash {
            in_string = !in_string;
        }

        if in_string && ch == '\n' && !prev_was_backslash {
            result.push_str("\\n");
        } else {
            result.push(ch);
        }

        prev_was_backslash = ch == '\\' && !prev_was_backslash;
    }

    result
}

/// Parse tool call arguments, with repair and error reporting.
pub fn parse_tool_args(arguments: &str) -> Result<serde_json::Value> {
    let repaired = repair_json(arguments);
    serde_json::from_str(&repaired).context("Failed to parse tool call arguments after repair")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repair_json_valid() {
        let input = r#"{"path": "src/main.rs"}"#;
        assert_eq!(repair_json(input), input);
    }

    #[test]
    fn test_repair_json_trailing_comma() {
        let input = r#"{"path": "src/main.rs",}"#;
        let result = repair_json(input);
        assert!(serde_json::from_str::<serde_json::Value>(&result).is_ok());
    }

    #[test]
    fn test_repair_json_trailing_comma_with_whitespace() {
        let input = r#"{"path": "src/main.rs" , }"#;
        let result = repair_json(input);
        assert!(serde_json::from_str::<serde_json::Value>(&result).is_ok());
    }

    #[test]
    fn test_repair_json_newlines_in_string() {
        let input = "{\"content\": \"line1\nline2\"}";
        let result = repair_json(input);
        assert!(serde_json::from_str::<serde_json::Value>(&result).is_ok());
    }

    #[test]
    fn test_parse_tool_args_valid() {
        let args = r#"{"path": "test.txt"}"#;
        let result = parse_tool_args(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_tool_args_with_repair() {
        let args = r#"{"path": "test.txt",}"#;
        let result = parse_tool_args(args);
        assert!(result.is_ok());
    }
}
