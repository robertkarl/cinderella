/// Plain stdout chat interface. No alternate screen, no viewport — just prints
/// to the terminal and lets normal scrollback work.
///
/// Commands:
///   /help  → Show help
///   /clear → Clear conversation
///   Ctrl-C → Quit

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::io::{self, Write};
use tokio::sync::mpsc;

use crate::agent::AgentEvent;

/// Status bar state.
#[allow(dead_code)]
#[derive(Default, Clone)]
pub struct StatusBar {
    pub model_name: String,
    pub quant: String,
    pub tok_per_sec: Option<f64>,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub ctx_used: usize,
    pub ctx_max: usize,
    pub gpu_layers: String,
}

/// Commands from TUI to the main loop.
pub enum TuiCommand {
    SendMessage(String),
    Clear,
    Quit,
    #[allow(dead_code)]
    Cancel,
}

/// Print a styled line (dimmed).
#[allow(dead_code)]
fn print_dim(text: &str) {
    if std::env::var("NO_COLOR").is_ok() {
        println!("{}", text);
    } else {
        println!("\x1b[2m{}\x1b[0m", text);
    }
}

/// Print a styled line (yellow, for warnings).
fn print_warn(text: &str) {
    if std::env::var("NO_COLOR").is_ok() {
        println!("⚠ {}", text);
    } else {
        println!("\x1b[33m⚠ {}\x1b[0m", text);
    }
}

/// Print the prompt and flush.
fn print_prompt() {
    if std::env::var("NO_COLOR").is_ok() {
        print!("\n> ");
    } else {
        print!("\n\x1b[1m$\x1b[0m ");
    }
    let _ = io::stdout().flush();
}

/// Read a line of input using raw mode for key handling.
/// Returns None on Ctrl-C (quit).
fn read_input() -> Option<String> {
    enable_raw_mode().ok()?;
    let mut input = String::new();
    let mut cursor = 0usize;

    loop {
        if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        let _ = disable_raw_mode();
                        println!();
                        return None;
                    }
                    (KeyCode::Char('z'), KeyModifiers::CONTROL) => {
                        // Restore terminal, send SIGTSTP to self, re-enable raw mode on resume
                        let _ = disable_raw_mode();
                        println!("\nCinderella is receiving SIGTSTP now. Depending on your shell, use bg to continue in the background or fg to resume.");
                        let _ = signal::kill(Pid::this(), Signal::SIGTSTP);
                        // When we resume (fg), re-enter raw mode and redraw
                        let _ = enable_raw_mode();
                        print!("\r\x1b[2K\x1b[1m>\x1b[0m {}", input);
                        let chars_after = input[cursor..].chars().count();
                        if chars_after > 0 {
                            print!("\x1b[{}D", chars_after);
                        }
                        let _ = io::stdout().flush();
                    }
                    (KeyCode::Enter, _) => {
                        let _ = disable_raw_mode();
                        println!();
                        return Some(input);
                    }
                    (KeyCode::Backspace, _) => {
                        if cursor > 0 {
                            let prev = input[..cursor]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            input.drain(prev..cursor);
                            cursor = prev;
                            // Redraw the line
                            print!("\r\x1b[2K\x1b[1m>\x1b[0m {}", input);
                            // Position cursor
                            let chars_after = input[cursor..].chars().count();
                            if chars_after > 0 {
                                print!("\x1b[{}D", chars_after);
                            }
                            let _ = io::stdout().flush();
                        }
                    }
                    (KeyCode::Left, _) => {
                        if cursor > 0 {
                            cursor = input[..cursor]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            print!("\x1b[1D");
                            let _ = io::stdout().flush();
                        }
                    }
                    (KeyCode::Right, _) => {
                        if cursor < input.len() {
                            cursor = input[cursor..]
                                .char_indices()
                                .nth(1)
                                .map(|(i, _)| cursor + i)
                                .unwrap_or(input.len());
                            print!("\x1b[1C");
                            let _ = io::stdout().flush();
                        }
                    }
                    (KeyCode::Char(c), _) => {
                        input.insert(cursor, c);
                        cursor += c.len_utf8();
                        // Redraw the line
                        print!("\r\x1b[2K\x1b[1m>\x1b[0m {}", input);
                        let chars_after = input[cursor..].chars().count();
                        if chars_after > 0 {
                            print!("\x1b[{}D", chars_after);
                        }
                        let _ = io::stdout().flush();
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Run the plain stdout interface. Returns when the user quits.
pub async fn run(
    mut agent_events: mpsc::Receiver<AgentEvent>,
    command_tx: mpsc::Sender<TuiCommand>,
    project_name: &str,
    _initial_status: StatusBar,
) -> anyhow::Result<()> {
    println!(
        "Working in {}. Ask me to read, write, or edit code.",
        project_name
    );

    loop {
        print_prompt();

        // Read input in a blocking task so we can't miss agent events
        // (but for simplicity, we read synchronously here — agent events
        // are processed after each turn completes)
        let input = tokio::task::spawn_blocking(read_input).await?;

        let msg = match input {
            None => {
                let _ = command_tx.send(TuiCommand::Quit).await;
                break;
            }
            Some(s) => s.trim().to_string(),
        };

        if msg.is_empty() {
            continue;
        }

        if msg == "/clear" {
            let _ = command_tx.send(TuiCommand::Clear).await;
            println!("Conversation cleared.");
            continue;
        }

        if msg == "/help" {
            println!("{}", HELP_TEXT);
            continue;
        }

        println!("\n  You: {}", msg);
        let _ = command_tx.send(TuiCommand::SendMessage(msg)).await;

        // Process agent events until turn completes
        let mut state = OutputState::default();

        while let Some(event) = agent_events.recv().await {
            if print_event(event, &mut state) {
                break;
            }
        }
    }

    Ok(())
}

/// State for event printing (shared between TUI and -p mode).
pub struct OutputState {
    pub in_thinking: bool,
    pub in_text: bool,
}

impl Default for OutputState {
    fn default() -> Self {
        Self {
            in_thinking: false,
            in_text: false,
        }
    }
}

/// Print a single agent event to stdout. Returns true if the turn is complete.
pub fn print_event(event: AgentEvent, state: &mut OutputState) -> bool {
    match event {
        AgentEvent::ThinkingDelta(text) => {
            if !state.in_thinking {
                state.in_thinking = true;
                state.in_text = false;
                print!("\n");
            }
            if std::env::var("NO_COLOR").is_ok() {
                print!("{}", text);
            } else {
                print!("\x1b[2m{}\x1b[0m", text);
            }
            let _ = io::stdout().flush();
        }
        AgentEvent::TextDelta(text) => {
            if state.in_thinking {
                state.in_thinking = false;
                print!("\n");
            }
            if !state.in_text {
                state.in_text = true;
                print!("\n  ");
            }
            print!("{}", text);
            let _ = io::stdout().flush();
        }
        AgentEvent::ToolStart { name, args_display } => {
            if state.in_thinking {
                state.in_thinking = false;
                print!("\n");
            }
            if state.in_text {
                state.in_text = false;
                println!();
            }
            println!("\n  {} {}", name, args_display);
        }
        AgentEvent::ToolResult { output, success } => {
            let indicator = if success { "|" } else { "| x" };
            for line in output.lines() {
                println!("  {} {}", indicator, line);
            }
        }
        AgentEvent::TokenRate { .. } => {}
        AgentEvent::ContextUsage { .. } => {}
        AgentEvent::Warning(msg) => {
            print_warn(&msg);
        }
        AgentEvent::TurnComplete => {
            if state.in_text || state.in_thinking {
                println!();
            }
            return true;
        }
        AgentEvent::ServerRestarted { attempt } => {
            print_warn(&format!("llama-server restarted ({}/3). Retrying...", attempt));
        }
        AgentEvent::StepStart { .. } => {}
        AgentEvent::StepComplete { .. } => {}
    }
    false
}

/// Serialize an AgentEvent to a JSON-line on stdout, flushing after each line.
/// Used by --format json mode. Returns true if the turn is complete.
/// TODO: Protocol deviations — text/tool_start/tool_done events lack the "step"
/// field the plan spec promises. ToolResult hardcodes "tool":"bash" instead of
/// carrying the actual tool name. ObjC app doesn't use these fields, so it works.
pub fn json_event(event: AgentEvent) -> bool {
    use crate::agent::StepStatus;

    let json = match event {
        AgentEvent::StepStart { step, title } => {
            serde_json::json!({"event": "step_start", "step": step, "title": title})
        }
        AgentEvent::StepComplete {
            step,
            status,
            summary,
            detail,
        } => {
            let status_str = match status {
                StepStatus::Pass => "pass",
                StepStatus::Fail => "fail",
                StepStatus::Warn => "warn",
            };
            serde_json::json!({
                "event": "step_complete",
                "step": step,
                "status": status_str,
                "summary": summary,
                "detail": detail,
            })
        }
        AgentEvent::TextDelta(content) => {
            // Include step context if step tracker is active — but we don't have step here.
            // The step context is already emitted via StepStart/StepComplete.
            serde_json::json!({"event": "text", "content": content})
        }
        AgentEvent::ThinkingDelta(content) => {
            serde_json::json!({"event": "thinking", "content": content})
        }
        AgentEvent::ToolStart { name, args_display } => {
            serde_json::json!({
                "event": "tool_start",
                "tool": name,
                "command": args_display,
            })
        }
        AgentEvent::ToolResult { output, success } => {
            serde_json::json!({
                "event": "tool_done",
                "tool": "bash",
                "exit_code": if success { 0 } else { 1 },
                "output": output,
            })
        }
        AgentEvent::TokenRate { .. } => return false,
        AgentEvent::ContextUsage { .. } => return false,
        AgentEvent::Warning(msg) => {
            serde_json::json!({"event": "warning", "message": msg})
        }
        AgentEvent::TurnComplete => {
            let line = serde_json::json!({"event": "done", "status": "complete"});
            println!("{}", line);
            let _ = io::stdout().flush();
            return true;
        }
        AgentEvent::ServerRestarted { .. } => return false,
    };

    println!("{}", json);
    let _ = io::stdout().flush();
    false
}

const HELP_TEXT: &str = "\
Commands:
  /help  → Show this help
  /clear → Clear conversation
  Ctrl-C → Quit";
