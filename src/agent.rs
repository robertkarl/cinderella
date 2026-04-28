/// Agent loop: system prompt -> LLM -> tool parse -> execute -> loop.
/// Context management: truncate tool results, summarize old turns, warn at 80%.

use anyhow::Result;
use std::path::PathBuf;

use crate::config;
use crate::llm::{self, LlmClient, Message, StreamEvent};

const MAX_TOOL_RESULT_LINES: usize = 100;
const MAX_TOOL_RESULT_CHARS: usize = 8000;
const SUMMARIZE_AFTER_TURNS: usize = 10;
const CONTEXT_WARN_PERCENT: f64 = 0.8;
const MAX_RETRY_ON_PARSE_FAILURE: u32 = 2;
const MAX_CONSECUTIVE_TOOL_FAILURES: u32 = 3;

/// Events the agent sends to the TUI.
pub enum AgentEvent {
    /// Streaming text from the LLM.
    TextDelta(String),
    /// Streaming thinking/reasoning from the LLM.
    ThinkingDelta(String),
    /// A tool is about to be executed.
    ToolStart { name: String, args_display: String },
    /// Tool execution completed.
    ToolResult {
        output: String,
        success: bool,
    },
    /// Token rate update.
    TokenRate { tok_per_sec: f64 },
    /// Context utilization update.
    ContextUsage { used: usize, max: usize },
    /// Warning message.
    Warning(String),
    /// Agent turn complete (text response fully received).
    TurnComplete,
    /// Server was restarted.
    #[allow(dead_code)]
    ServerRestarted { attempt: u32 },
}

pub struct Agent {
    client: LlmClient,
    messages: Vec<Message>,
    project_dir: PathBuf,
    ctx_size: u32,
    turn_count: usize,
}

impl Agent {
    pub fn new(api_url: &str, project_dir: PathBuf, ctx_size: u32, model_name: &str) -> Self {
        let client = LlmClient::new(api_url, model_name);

        let messages = vec![Message {
            role: "system".to_string(),
            content: Some(config::SYSTEM_PROMPT.to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];

        Self {
            client,
            messages,
            project_dir,
            ctx_size,
            turn_count: 0,
        }
    }

    /// Process a user message through the agent loop.
    /// Keeps calling the LLM until it produces a text response (no more tool calls).
    pub async fn process_message(
        &mut self,
        user_input: &str,
        mut on_event: impl FnMut(AgentEvent),
    ) -> Result<()> {
        // Add user message
        self.messages.push(Message {
            role: "user".to_string(),
            content: Some(user_input.to_string()),
            tool_calls: None,
            tool_call_id: None,
        });
        self.turn_count += 1;

        // Context management: summarize old turns
        if self.turn_count > SUMMARIZE_AFTER_TURNS {
            self.summarize_old_turns();
        }

        // Check context usage
        let ctx_used = self.estimate_tokens();
        let ctx_max = self.ctx_size as usize;
        on_event(AgentEvent::ContextUsage {
            used: ctx_used,
            max: ctx_max,
        });

        if ctx_used as f64 > ctx_max as f64 * CONTEXT_WARN_PERCENT {
            on_event(AgentEvent::Warning(format!(
                "Context {}% full ({}/{}). Consider /clear.",
                (ctx_used * 100) / ctx_max,
                ctx_used,
                ctx_max
            )));
        }

        // Agent loop: keep going while LLM produces tool calls
        let mut retry_count: u32 = 0;
        let mut consecutive_failures: u32 = 0;
        loop {
            let tools = config::tool_definitions();
            let mut response_text = String::new();
            let mut response_tool_calls: Vec<llm::ToolCall> = Vec::new();

            let result = self
                .client
                .chat_completion(&self.messages, &tools, |event| match event {
                    StreamEvent::TextDelta(text) => {
                        response_text.push_str(&text);
                        on_event(AgentEvent::TextDelta(text));
                    }
                    StreamEvent::ThinkingDelta(text) => {
                        on_event(AgentEvent::ThinkingDelta(text));
                    }
                    StreamEvent::ToolCallComplete(tc) => {
                        response_tool_calls.push(tc);
                    }
                    StreamEvent::TokenTick { tokens, elapsed_ms } => {
                        if elapsed_ms > 0 {
                            let tok_s = tokens as f64 / (elapsed_ms as f64 / 1000.0);
                            on_event(AgentEvent::TokenRate { tok_per_sec: tok_s });
                        }
                    }
                    StreamEvent::Done => {}
                })
                .await;

            match result {
                Ok(msg) => {
                    // Add assistant message to history
                    self.messages.push(msg.clone());

                    // If there are tool calls, execute them
                    if let Some(ref tool_calls) = msg.tool_calls {
                        for tc in tool_calls {
                            // Parse arguments
                            let args = match llm::parse_tool_args(&tc.function.arguments) {
                                Ok(args) => {
                                    retry_count = 0; // reset on successful parse
                                    args
                                }
                                Err(_) if retry_count < MAX_RETRY_ON_PARSE_FAILURE => {
                                    retry_count += 1;
                                    // Add a re-prompt message
                                    self.messages.push(Message {
                                        role: "tool".to_string(),
                                        content: Some(
                                            "Your previous tool call had malformed JSON arguments. \
                                             Please try again with valid JSON."
                                                .to_string(),
                                        ),
                                        tool_calls: None,
                                        tool_call_id: Some(tc.id.clone()),
                                    });
                                    continue;
                                }
                                Err(e) => {
                                    on_event(AgentEvent::Warning(format!(
                                        "Model couldn't format tool call after {} attempts. \
                                         Try rephrasing your request. Error: {}",
                                        MAX_RETRY_ON_PARSE_FAILURE, e
                                    )));
                                    on_event(AgentEvent::TurnComplete);
                                    return Ok(());
                                }
                            };

                            let args_display = format_args_display(&tc.function.name, &args);
                            on_event(AgentEvent::ToolStart {
                                name: tc.function.name.clone(),
                                args_display: args_display.clone(),
                            });

                            // Execute the tool
                            let result = crate::tools::execute(
                                &tc.function.name,
                                &args,
                                &self.project_dir,
                            )
                            .await;

                            // Truncate for display (5 lines)
                            let display_output = truncate_for_display(&result.output, 5);
                            on_event(AgentEvent::ToolResult {
                                output: display_output,
                                success: result.success,
                            });

                            // Track consecutive failures
                            if result.success {
                                consecutive_failures = 0;
                            } else {
                                consecutive_failures += 1;
                            }

                            // Truncate for context (100 lines / 8K chars)
                            let context_output =
                                truncate_for_context(&result.output);

                            // Add tool result to conversation
                            self.messages.push(Message {
                                role: "tool".to_string(),
                                content: Some(context_output),
                                tool_calls: None,
                                tool_call_id: Some(tc.id.clone()),
                            });
                        }

                        // Bail if too many consecutive failures
                        if consecutive_failures >= MAX_CONSECUTIVE_TOOL_FAILURES {
                            let bail_msg = format!(
                                "{} consecutive tool failures. Stopping to avoid an infinite loop. \
                                 Re-read the error messages above and try a different approach, \
                                 or ask the user for help.",
                                consecutive_failures
                            );
                            on_event(AgentEvent::Warning(bail_msg.clone()));
                            self.messages.push(Message {
                                role: "user".to_string(),
                                content: Some(bail_msg),
                                tool_calls: None,
                                tool_call_id: None,
                            });
                            on_event(AgentEvent::TurnComplete);
                            return Ok(());
                        }

                        // Update context usage after tool results
                        let ctx_used = self.estimate_tokens();
                        on_event(AgentEvent::ContextUsage {
                            used: ctx_used,
                            max: ctx_max,
                        });

                        // Continue the loop — LLM needs to process tool results
                        continue;
                    }

                    // No tool calls — text response, turn complete
                    on_event(AgentEvent::TurnComplete);
                    return Ok(());
                }
                Err(e) => {
                    on_event(AgentEvent::Warning(format!("LLM error: {}", e)));
                    on_event(AgentEvent::TurnComplete);
                    return Err(e);
                }
            }
        }
    }

    /// Clear conversation history (keep system prompt).
    pub fn clear(&mut self) {
        self.messages.truncate(1); // keep system prompt
        self.turn_count = 0;
    }

    /// Estimate token count (rough: chars/4).
    fn estimate_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| {
                let content_len = m.content.as_ref().map(|c| c.len()).unwrap_or(0);
                let tool_len = m
                    .tool_calls
                    .as_ref()
                    .map(|tcs| {
                        tcs.iter()
                            .map(|tc| tc.function.name.len() + tc.function.arguments.len())
                            .sum::<usize>()
                    })
                    .unwrap_or(0);
                (content_len + tool_len) / 4
            })
            .sum()
    }

    /// Summarize old tool results to save context.
    fn summarize_old_turns(&mut self) {
        // Keep system prompt (index 0) and last N messages intact
        let keep_recent = 6; // keep last 3 turns (user + assistant + tool each)
        if self.messages.len() <= keep_recent + 1 {
            return;
        }

        let summarize_end = self.messages.len() - keep_recent;
        for i in 1..summarize_end {
            if self.messages[i].role == "tool" {
                if let Some(ref content) = self.messages[i].content {
                    if content.len() > 200 {
                        // Replace with one-line summary
                        let summary = format!(
                            "(summarized: {} chars of tool output)",
                            content.len()
                        );
                        self.messages[i].content = Some(summary);
                    }
                }
            }
        }
    }
}

fn format_args_display(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "read_file" => args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        "write_file" => args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        "edit_file" => args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        "bash" => args
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        "ls" => args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string(),
        _ => format!("{:?}", args),
    }
}

fn truncate_for_display(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_lines {
        return output.to_string();
    }
    let mut result: Vec<&str> = lines[..max_lines].to_vec();
    result.push(&"");
    let remaining = lines.len() - max_lines;
    format!("{}\n...({} more lines, but truncated to 100 lines.)", result.join("\n"), remaining)
}

fn truncate_for_context(output: &str) -> String {
    // Truncate by lines
    let lines: Vec<&str> = output.lines().collect();
    let truncated = if lines.len() > MAX_TOOL_RESULT_LINES {
        let head = 10;
        let tail = 10;
        let omitted = lines.len() - head - tail;
        let mut result = lines[..head].join("\n");
        result.push_str(&format!("\n...({} lines omitted)\n", omitted));
        result.push_str(&lines[lines.len() - tail..].join("\n"));
        result
    } else {
        output.to_string()
    };

    // Truncate by chars (safe for multi-byte UTF-8)
    if truncated.len() > MAX_TOOL_RESULT_CHARS {
        let boundary = truncated
            .char_indices()
            .take_while(|(i, _)| *i < MAX_TOOL_RESULT_CHARS)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        let mut s = truncated[..boundary].to_string();
        s.push_str("\n...(truncated)");
        s
    } else {
        truncated
    }
}
