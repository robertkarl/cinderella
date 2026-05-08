/// Agent loop: system prompt -> LLM -> tool parse -> execute -> loop.
/// Context management: truncate tool results, summarize old turns, warn at 80%.

use anyhow::Result;
use std::path::PathBuf;

use crate::config::{self, SafetyProfile};
use crate::llm::{self, LlmClient, Message, StreamEvent};

const MAX_TOOL_RESULT_LINES: usize = 100;
const MAX_TOOL_RESULT_CHARS: usize = 8000;
const SUMMARIZE_AFTER_TURNS: usize = 10;
const CONTEXT_WARN_PERCENT: f64 = 0.8;
const MAX_RETRY_ON_PARSE_FAILURE: u32 = 2;
const MAX_CONSECUTIVE_TOOL_FAILURES: u32 = 3;
const MAX_AGENT_ITERATIONS: u32 = 25;

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
    /// Echo of the user's input prompt (for Glass Slipper display).
    UserPrompt(String),
    /// The fixed runbook plan steps (emitted once at start).
    Plan(Vec<String>),
    /// Final diagnosis text (from synthesis step).
    Diagnosis(String),
    /// A diagnostic step has started.
    StepStart { step: String, title: String },
    /// A diagnostic step has completed.
    StepComplete {
        step: String,
        status: StepStatus,
        summary: String,
        detail: String,
    },
    /// Memory pressure warning — suggest downgrade.
    MemoryWarning {
        pageout_rate: u64,
        swap_used_mb: f64,
        tok_per_sec: Option<f64>,
    },
    /// Model swap completed (post-facto notification).
    ModelSwap {
        from_model: String,
        to_model: String,
        reason: String,
    },
    /// Promotion available — running smaller model but pressure has eased.
    PromotionAvailable {
        to_model: String,
    },
}

/// Status of a completed diagnostic step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Pass,
    Warn,
    Fail,
}

impl StepStatus {
    fn severity(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Warn => 1,
            Self::Fail => 2,
        }
    }

    fn worse(self, other: Self) -> Self {
        if other.severity() > self.severity() { other } else { self }
    }
}

/// Tracks diagnostic step transitions by scanning TextDelta for STEP: markers.
/// Uses line buffering because SSE TextDelta events are token-sized fragments.
pub struct StepTracker {
    /// Line buffer — accumulates text until newline.
    line_buf: String,
    /// Currently active step ID (e.g. "dns").
    current_step: Option<String>,
    /// Accumulated text for the current step.
    step_text: String,
    /// Tool exit codes seen during the current step.
    tool_exit_codes: Vec<bool>,
    /// Whether step tracking is enabled (only for --format json).
    enabled: bool,
    /// Re-prompt retry counter (avoid infinite loops).
    retries: u32,
    /// Worst status seen across all completed steps (for diagnosis coloring).
    worst_status: StepStatus,
}

impl StepTracker {
    pub fn new(enabled: bool) -> Self {
        Self {
            line_buf: String::new(),
            current_step: None,
            step_text: String::new(),
            tool_exit_codes: Vec::new(),
            enabled,
            worst_status: StepStatus::Pass,
            retries: 0,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Feed text from a TextDelta event. Returns any step events to emit.
    pub fn feed_text(&mut self, text: &str) -> Vec<AgentEvent> {
        if !self.enabled {
            return Vec::new();
        }
        let mut events = Vec::new();
        self.line_buf.push_str(text);
        self.process_lines(&mut events);
        events
    }

    /// Check the line buffer for a pending STEP: marker without a trailing newline.
    /// Called before tool events, because the model often emits "STEP: dns" then
    /// immediately calls a tool without a newline in between.
    pub fn flush_pending_marker(&mut self) -> Vec<AgentEvent> {
        if !self.enabled {
            return Vec::new();
        }
        let mut events = Vec::new();
        let trimmed = self.line_buf.trim().to_string();
        if let Some(step_name) = trimmed.strip_prefix("STEP: ").map(|s| s.trim().to_string()) {
            if !step_name.is_empty() {
                self.line_buf.clear();
                if let Some(prev) = self.current_step.take() {
                    events.extend(self.close_step(&prev));
                }
                let title = step_display_title(&step_name);
                events.push(AgentEvent::StepStart {
                    step: step_name.clone(),
                    title,
                });
                self.current_step = Some(step_name);
                self.step_text.clear();
                self.tool_exit_codes.clear();
                self.retries = 0;
            }
        }
        events
    }

    fn process_lines(&mut self, events: &mut Vec<AgentEvent>) {
        while let Some(nl_pos) = self.line_buf.find('\n') {
            let line = self.line_buf[..nl_pos].to_string();
            self.line_buf = self.line_buf[nl_pos + 1..].to_string();
            self.handle_step_line(&line, events);
        }
    }

    fn handle_step_line(&mut self, line: &str, events: &mut Vec<AgentEvent>) {
        if let Some(step_name) = extract_step_marker(line) {
            if let Some(prev) = self.current_step.take() {
                events.extend(self.close_step(&prev));
            }
            let title = step_display_title(&step_name);
            events.push(AgentEvent::StepStart {
                step: step_name.clone(),
                title,
            });
            self.current_step = Some(step_name);
            self.step_text.clear();
            self.tool_exit_codes.clear();
            self.retries = 0;
        } else if !line.trim().is_empty() {
            if !self.step_text.is_empty() {
                self.step_text.push('\n');
            }
            self.step_text.push_str(line);
        }
    }

    /// Record a tool result for status derivation and capture output for detail.
    pub fn record_tool_result(&mut self, success: bool, command: &str, output: &str) {
        if self.enabled {
            self.tool_exit_codes.push(success);
            // Capture tool command + output as step detail
            if !self.step_text.is_empty() {
                self.step_text.push('\n');
            }
            self.step_text.push_str(&format!("$ {}", command));
            if !output.is_empty() {
                self.step_text.push('\n');
                self.step_text.push_str(output);
            }
        }
    }

    /// Check if the model should be re-prompted to use tools.
    /// Returns (should_reprompt, pending_events). Caller MUST emit the events
    /// regardless of the bool — they include StepComplete/StepStart from any
    /// pending marker that was flushed.
    pub fn should_force_tool_use(&mut self) -> (bool, Vec<AgentEvent>) {
        if !self.enabled {
            return (false, Vec::new());
        }
        // Check for pending marker first — this may close the previous step
        // and start a new one. The events must be emitted by the caller.
        let marker_events = self.flush_pending_marker();

        let should_reprompt = if let Some(ref step) = self.current_step {
            let needs_tools = !matches!(step.as_str(), "parse_target" | "synthesis");
            let no_tools_called = self.tool_exit_codes.is_empty();
            if needs_tools && no_tools_called {
                // Bump a retry counter to avoid infinite re-prompts
                self.retries += 1;
                self.retries <= 2
            } else {
                false
            }
        } else {
            false
        };
        (should_reprompt, marker_events)
    }

    /// Flush on TurnComplete — close the final step.
    pub fn flush(&mut self) -> Vec<AgentEvent> {
        if !self.enabled {
            return Vec::new();
        }
        let mut events = Vec::new();
        // Accumulate any remaining partial line as step text
        if !self.line_buf.is_empty() {
            let remaining = std::mem::take(&mut self.line_buf);
            if !self.step_text.is_empty() {
                self.step_text.push('\n');
            }
            self.step_text.push_str(&remaining);
        }
        if let Some(prev) = self.current_step.take() {
            events.extend(self.close_step(&prev));
        }
        events
    }

    /// Close a step, returning StepComplete and (if synthesis) a Diagnosis event.
    fn close_step(&mut self, step: &str) -> Vec<AgentEvent> {
        let status = derive_step_status(&self.tool_exit_codes);

        if step != "synthesis" {
            self.worst_status = self.worst_status.worse(status);
        }

        let summary = extract_step_summary(&self.step_text);
        let effective_status = if step == "synthesis" { self.worst_status } else { status };

        let mut events = vec![AgentEvent::StepComplete {
            step: step.to_string(),
            status: effective_status,
            summary,
            detail: self.step_text.clone(),
        }];

        if step == "synthesis" && !self.step_text.is_empty() {
            events.push(AgentEvent::Diagnosis(self.step_text.clone()));
        }

        events
    }
}

/// Map step IDs to display titles.
fn step_display_title(step: &str) -> String {
    match step {
        "parse_target" => "Parse Target".to_string(),
        "dns" => "DNS Resolution".to_string(),
        "connectivity" => "Connectivity Check".to_string(),
        "route_analysis" => "Route Analysis".to_string(),
        "port_check" => "Port Check".to_string(),
        "service_check" => "Service Check".to_string(),
        "synthesis" => "Diagnosis".to_string(),
        other => other.to_string(),
    }
}

pub struct Agent {
    client: LlmClient,
    messages: Vec<Message>,
    project_dir: PathBuf,
    ctx_size: u32,
    turn_count: usize,
    safety_profile: SafetyProfile,
    step_tracker: StepTracker,
}

impl Agent {
    pub fn new(
        api_url: &str,
        project_dir: PathBuf,
        ctx_size: u32,
        model_name: &str,
        safety_profile: SafetyProfile,
        step_tracking: bool,
    ) -> Self {
        let client = LlmClient::new(api_url, model_name);

        let messages = vec![Message {
            role: "system".to_string(),
            content: Some(config::system_prompt_for(safety_profile).to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];

        Self {
            client,
            messages,
            project_dir,
            ctx_size,
            turn_count: 0,
            safety_profile,
            step_tracker: StepTracker::new(step_tracking),
        }
    }

    /// Process a user message through the agent loop.
    /// Keeps calling the LLM until it produces a text response (no more tool calls).
    pub async fn process_message(
        &mut self,
        user_input: &str,
        mut on_event: impl FnMut(AgentEvent),
    ) -> Result<()> {
        self.prepare_turn(user_input, &mut on_event);

        let ctx_max = self.ctx_size as usize;
        let mut retry_count: u32 = 0;
        let mut consecutive_failures: u32 = 0;

        for _ in 0..MAX_AGENT_ITERATIONS {
            let msg = match self.call_llm(&mut on_event).await {
                Ok(msg) => msg,
                Err(e) => {
                    on_event(AgentEvent::Warning(format!("LLM error: {}", e)));
                    self.flush_and_complete(&mut on_event);
                    return Err(e);
                }
            };
            self.messages.push(msg.clone());

            if let Some(ref tool_calls) = msg.tool_calls {
                if self.handle_tool_calls(tool_calls, &mut retry_count, &mut consecutive_failures, ctx_max, &mut on_event).await {
                    return Ok(());
                }
                continue;
            }

            if self.handle_text_response(&mut on_event) {
                continue;
            }
            self.flush_and_complete(&mut on_event);
            return Ok(());
        }

        on_event(AgentEvent::Warning(format!(
            "Agent reached {} iterations without completing. Stopping.",
            MAX_AGENT_ITERATIONS
        )));
        self.flush_and_complete(&mut on_event);
        Ok(())
    }

    /// Set up a new turn: add user message, emit plan events, summarize/check context.
    fn prepare_turn(&mut self, user_input: &str, on_event: &mut impl FnMut(AgentEvent)) {
        self.messages.push(Message {
            role: "user".to_string(),
            content: Some(user_input.to_string()),
            tool_calls: None,
            tool_call_id: None,
        });
        self.turn_count += 1;

        if self.step_tracker.is_enabled() {
            on_event(AgentEvent::UserPrompt(user_input.to_string()));
            on_event(AgentEvent::Plan(vec![
                "Parse Target".to_string(),
                "DNS Resolution".to_string(),
                "Connectivity Check".to_string(),
                "Route Analysis".to_string(),
                "Port Check".to_string(),
                "Service Check".to_string(),
                "Diagnosis".to_string(),
            ]));
        }

        if self.turn_count > SUMMARIZE_AFTER_TURNS {
            self.summarize_old_turns();
        }

        self.emit_context_check(self.ctx_size as usize, on_event);
    }

    /// Call LLM with streaming, routing events to on_event. Returns the response message.
    async fn call_llm(&mut self, on_event: &mut impl FnMut(AgentEvent)) -> Result<Message> {
        let tools = config::tool_definitions();
        self.client
            .chat_completion(&self.messages, &tools, |event| match event {
                StreamEvent::TextDelta(text) => {
                    for se in self.step_tracker.feed_text(&text) {
                        on_event(se);
                    }
                    on_event(AgentEvent::TextDelta(text));
                }
                StreamEvent::ThinkingDelta(text) => {
                    on_event(AgentEvent::ThinkingDelta(text));
                }
                StreamEvent::ToolCallComplete(_) => {} // handled via msg.tool_calls
                StreamEvent::TokenTick { tokens, elapsed_ms } => {
                    if elapsed_ms > 0 {
                        let tok_s = tokens as f64 / (elapsed_ms as f64 / 1000.0);
                        on_event(AgentEvent::TokenRate { tok_per_sec: tok_s });
                    }
                }
                StreamEvent::Done => {}
            })
            .await
    }

    /// Execute all tool calls in a response. Returns true if the turn should end.
    async fn handle_tool_calls(
        &mut self,
        tool_calls: &[llm::ToolCall],
        retry_count: &mut u32,
        consecutive_failures: &mut u32,
        ctx_max: usize,
        on_event: &mut impl FnMut(AgentEvent),
    ) -> bool {
        for tc in tool_calls {
            if self.execute_single_tool_call(tc, retry_count, consecutive_failures, on_event).await
            {
                return true;
            }
        }

        if *consecutive_failures >= MAX_CONSECUTIVE_TOOL_FAILURES {
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
            self.flush_and_complete(on_event);
            return true;
        }

        self.emit_context_check(ctx_max, on_event);
        false
    }

    /// Execute a single tool call. Returns true if the turn should end immediately.
    async fn execute_single_tool_call(
        &mut self,
        tc: &llm::ToolCall,
        retry_count: &mut u32,
        consecutive_failures: &mut u32,
        on_event: &mut impl FnMut(AgentEvent),
    ) -> bool {
        let args = match llm::parse_tool_args(&tc.function.arguments) {
            Ok(args) => {
                *retry_count = 0;
                args
            }
            Err(e) => {
                return self.handle_parse_failure(tc, retry_count, e, on_event);
            }
        };

        let args_display = format_args_display(&tc.function.name, &args);

        for se in self.step_tracker.flush_pending_marker() {
            on_event(se);
        }
        on_event(AgentEvent::ToolStart {
            name: tc.function.name.clone(),
            args_display: args_display.clone(),
        });

        let result =
            crate::tools::execute(&tc.function.name, &args, &self.project_dir, self.safety_profile)
                .await;

        let display_output = truncate_for_display(&result.output, 5);
        on_event(AgentEvent::ToolResult {
            output: display_output.clone(),
            success: result.success,
        });
        self.step_tracker
            .record_tool_result(result.success, &args_display, &display_output);

        if result.success {
            *consecutive_failures = 0;
        } else {
            *consecutive_failures += 1;
        }

        self.messages.push(Message {
            role: "tool".to_string(),
            content: Some(truncate_for_context(&result.output)),
            tool_calls: None,
            tool_call_id: Some(tc.id.clone()),
        });
        false
    }

    /// Handle a tool argument parse failure. Returns true if the turn should end.
    fn handle_parse_failure(
        &mut self,
        tc: &llm::ToolCall,
        retry_count: &mut u32,
        error: anyhow::Error,
        on_event: &mut impl FnMut(AgentEvent),
    ) -> bool {
        if *retry_count < MAX_RETRY_ON_PARSE_FAILURE {
            *retry_count += 1;
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
            return false; // continue to next tool call
        }
        on_event(AgentEvent::Warning(format!(
            "Model couldn't format tool call after {} attempts. \
             Try rephrasing your request. Error: {}",
            MAX_RETRY_ON_PARSE_FAILURE, error
        )));
        self.flush_and_complete(on_event);
        true
    }

    /// Handle a text-only response (no tool calls). Returns true if LLM should be re-prompted.
    fn handle_text_response(&mut self, on_event: &mut impl FnMut(AgentEvent)) -> bool {
        let (should_reprompt, pending_events) = self.step_tracker.should_force_tool_use();
        for se in pending_events {
            on_event(se);
        }
        if !should_reprompt {
            return false;
        }
        self.messages.push(Message {
            role: "user".to_string(),
            content: Some(
                "You wrote a command as text instead of calling the bash tool. \
                 You MUST use the bash tool to execute commands. \
                 Call the bash tool now with the command you just described."
                    .to_string(),
            ),
            tool_calls: None,
            tool_call_id: None,
        });
        on_event(AgentEvent::Warning(
            "Model narrated instead of calling tool — re-prompting.".to_string(),
        ));
        true
    }

    /// Emit context usage and warn if near capacity.
    fn emit_context_check(&self, ctx_max: usize, on_event: &mut impl FnMut(AgentEvent)) {
        let ctx_used = self.estimate_tokens();
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
    }

    /// Flush step tracker and emit TurnComplete.
    fn flush_and_complete(&mut self, on_event: &mut impl FnMut(AgentEvent)) {
        for se in self.step_tracker.flush() {
            on_event(se);
        }
        on_event(AgentEvent::TurnComplete);
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
        let keep_recent = 6; // keep last 3 turns (user + assistant + tool each)
        if self.messages.len() <= keep_recent + 1 {
            return;
        }

        let summarize_end = self.messages.len() - keep_recent;
        for i in 1..summarize_end {
            if should_summarize_message(&self.messages[i]) {
                let len = self.messages[i].content.as_ref().map(|c| c.len()).unwrap_or(0);
                self.messages[i].content =
                    Some(format!("(summarized: {} chars of tool output)", len));
            }
        }
    }
}

/// Extract a step marker name from a line, if present.
fn extract_step_marker(line: &str) -> Option<String> {
    let name = line.strip_prefix("STEP: ")?.trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}

/// Derive step status from a set of tool exit codes.
fn derive_step_status(exit_codes: &[bool]) -> StepStatus {
    if exit_codes.is_empty() || exit_codes.iter().all(|&s| s) {
        StepStatus::Pass
    } else if exit_codes.iter().all(|&s| !s) {
        StepStatus::Fail
    } else {
        StepStatus::Warn
    }
}

/// Extract the first meaningful summary line from step text.
fn extract_step_summary(text: &str) -> String {
    text.lines()
        .find(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('$')
        })
        .or_else(|| text.lines().find(|l| !l.trim().is_empty()))
        .unwrap_or("")
        .to_string()
}

/// Check if a message should be summarized (old tool result > 200 chars).
fn should_summarize_message(msg: &Message) -> bool {
    msg.role == "tool" && msg.content.as_ref().map(|c| c.len() > 200).unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_tracker_detects_marker() {
        let mut tracker = StepTracker::new(true);
        let events = tracker.feed_text("STEP: dns\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::StepStart { step, title } => {
                assert_eq!(step, "dns");
                assert_eq!(title, "DNS Resolution");
            }
            _ => panic!("Expected StepStart"),
        }
    }

    #[test]
    fn test_step_tracker_partial_line_buffering() {
        let mut tracker = StepTracker::new(true);
        // Simulate token-sized fragments
        let events1 = tracker.feed_text("STE");
        assert!(events1.is_empty());
        let events2 = tracker.feed_text("P: dns");
        assert!(events2.is_empty());
        let events3 = tracker.feed_text("\n");
        assert_eq!(events3.len(), 1);
        match &events3[0] {
            AgentEvent::StepStart { step, .. } => assert_eq!(step, "dns"),
            _ => panic!("Expected StepStart"),
        }
    }

    #[test]
    fn test_step_tracker_step_transition() {
        let mut tracker = StepTracker::new(true);
        tracker.feed_text("STEP: parse_target\n");
        tracker.feed_text("Investigating http://localhost\n");
        let events = tracker.feed_text("STEP: dns\n");
        // Should get StepComplete for parse_target, then StepStart for dns
        assert_eq!(events.len(), 2);
        match &events[0] {
            AgentEvent::StepComplete { step, status, summary, .. } => {
                assert_eq!(step, "parse_target");
                assert_eq!(*status, StepStatus::Pass); // no tool calls
                assert_eq!(summary, "Investigating http://localhost");
            }
            _ => panic!("Expected StepComplete"),
        }
        match &events[1] {
            AgentEvent::StepStart { step, .. } => assert_eq!(step, "dns"),
            _ => panic!("Expected StepStart"),
        }
    }

    #[test]
    fn test_step_tracker_flush_closes_final_step() {
        let mut tracker = StepTracker::new(true);
        tracker.feed_text("STEP: synthesis\n");
        tracker.feed_text("Everything looks good.\n");
        let events = tracker.flush();
        assert_eq!(events.len(), 2); // StepComplete + Diagnosis
        match &events[0] {
            AgentEvent::StepComplete { step, status, summary, .. } => {
                assert_eq!(step, "synthesis");
                assert_eq!(*status, StepStatus::Pass);
                assert_eq!(summary, "Everything looks good.");
            }
            _ => panic!("Expected StepComplete"),
        }
        match &events[1] {
            AgentEvent::Diagnosis(text) => {
                assert_eq!(text, "Everything looks good.");
            }
            _ => panic!("Expected Diagnosis"),
        }
    }

    #[test]
    fn test_step_tracker_status_from_tool_results() {
        let mut tracker = StepTracker::new(true);
        tracker.feed_text("STEP: dns\n");
        tracker.feed_text("Running dig\n");
        tracker.record_tool_result(true, "dig localhost", "127.0.0.1");
        tracker.record_tool_result(false, "dig fail.test", "NXDOMAIN");
        let events = tracker.flush();
        match &events[0] {
            AgentEvent::StepComplete { status, .. } => {
                assert_eq!(*status, StepStatus::Warn); // mixed results
            }
            _ => panic!("Expected StepComplete"),
        }
    }

    #[test]
    fn test_step_tracker_all_fail() {
        let mut tracker = StepTracker::new(true);
        tracker.feed_text("STEP: port_check\n");
        tracker.feed_text("Checking port\n");
        tracker.record_tool_result(false, "curl localhost:9999", "Connection refused");
        tracker.record_tool_result(false, "nc -z localhost 9999", "failed");
        let events = tracker.flush();
        match &events[0] {
            AgentEvent::StepComplete { status, .. } => {
                assert_eq!(*status, StepStatus::Fail);
            }
            _ => panic!("Expected StepComplete"),
        }
    }

    #[test]
    fn test_step_tracker_disabled() {
        let mut tracker = StepTracker::new(false);
        let events = tracker.feed_text("STEP: dns\n");
        assert!(events.is_empty());
        let events = tracker.flush();
        assert!(events.is_empty());
    }

    #[test]
    fn test_step_tracker_summary_first_line() {
        let mut tracker = StepTracker::new(true);
        tracker.feed_text("STEP: dns\n");
        tracker.feed_text("DNS resolution succeeds.\ndig returned 127.0.0.1\nMore details here.\n");
        let events = tracker.flush();
        match &events[0] {
            AgentEvent::StepComplete { summary, detail, .. } => {
                assert_eq!(summary, "DNS resolution succeeds.");
                assert!(detail.contains("127.0.0.1"));
            }
            _ => panic!("Expected StepComplete"),
        }
    }

    #[test]
    fn test_step_display_titles() {
        assert_eq!(step_display_title("parse_target"), "Parse Target");
        assert_eq!(step_display_title("dns"), "DNS Resolution");
        assert_eq!(step_display_title("connectivity"), "Connectivity Check");
        assert_eq!(step_display_title("route_analysis"), "Route Analysis");
        assert_eq!(step_display_title("port_check"), "Port Check");
        assert_eq!(step_display_title("service_check"), "Service Check");
        assert_eq!(step_display_title("synthesis"), "Diagnosis");
        assert_eq!(step_display_title("unknown_step"), "unknown_step");
    }

    #[test]
    fn test_step_tracker_flush_with_partial_line() {
        let mut tracker = StepTracker::new(true);
        tracker.feed_text("STEP: synthesis\n");
        tracker.feed_text("Partial text without newline");
        let events = tracker.flush();
        assert_eq!(events.len(), 2); // StepComplete + Diagnosis
        match &events[0] {
            AgentEvent::StepComplete { summary, .. } => {
                assert_eq!(summary, "Partial text without newline");
            }
            _ => panic!("Expected StepComplete"),
        }
        match &events[1] {
            AgentEvent::Diagnosis(text) => {
                assert_eq!(text, "Partial text without newline");
            }
            _ => panic!("Expected Diagnosis"),
        }
    }

    #[test]
    fn test_diagnosis_emitted_when_synthesis_closed_by_step_marker() {
        // Regression: Diagnosis must be emitted even when synthesis is closed
        // by a subsequent STEP: marker (not just by flush).
        let mut tracker = StepTracker::new(true);
        tracker.feed_text("STEP: synthesis\n");
        tracker.feed_text("The server is down.\n");
        let events = tracker.feed_text("STEP: extra_step\n");
        // Should produce: StepComplete(synthesis), Diagnosis, StepStart(extra_step)
        assert!(events.len() >= 3, "Expected at least 3 events, got {}", events.len());
        match &events[0] {
            AgentEvent::StepComplete { step, .. } => assert_eq!(step, "synthesis"),
            _ => panic!("Expected StepComplete for synthesis"),
        }
        match &events[1] {
            AgentEvent::Diagnosis(text) => assert_eq!(text, "The server is down."),
            _ => panic!("Expected Diagnosis"),
        }
        match &events[2] {
            AgentEvent::StepStart { step, .. } => assert_eq!(step, "extra_step"),
            _ => panic!("Expected StepStart for extra_step"),
        }
    }
}
