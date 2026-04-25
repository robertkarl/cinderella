/// Ratatui TUI: chat display, input box, status bar, keybindings.
///
/// Keybindings:
///   Enter       → Send message
///   Shift-Enter → Newline in input
///   Esc         → Cancel running tool (or clear input)
///   Ctrl-C      → Quit
///   Up/Down     → Scroll conversation
///   /help       → Show keybindings
///   /clear      → Clear conversation

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::io::stdout;
use tokio::sync::mpsc;

use crate::agent::AgentEvent;

const MAX_ENTRIES: usize = 10_000;

/// Display entries in the conversation view.
#[derive(Clone)]
pub enum ChatEntry {
    UserMessage(String),
    AssistantText(String),
    ToolStart { name: String, args: String },
    ToolResult { name: String, output: String, success: bool },
    Warning(String),
    Info(String),
}

/// Status bar state.
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

impl StatusBar {
    fn render_text(&self) -> String {
        let tok_s = self
            .tok_per_sec
            .map(|t| format!("{:.1} t/s", t))
            .unwrap_or_else(|| "\u{2014} t/s".to_string());

        format!(
            "{} {} \u{2502} {} \u{2502} {:.1}/{:.0}G \u{2502} {}/{} \u{2502} {}",
            self.model_name,
            self.quant,
            tok_s,
            self.ram_used_gb,
            self.ram_total_gb,
            format_tokens(self.ctx_used),
            format_tokens(self.ctx_max),
            self.gpu_layers,
        )
    }
}

fn format_tokens(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}K", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

/// Commands from TUI to the main loop.
pub enum TuiCommand {
    SendMessage(String),
    Clear,
    Quit,
    Cancel,
}

/// Run the TUI. Returns when the user quits.
pub async fn run(
    mut agent_events: mpsc::Receiver<AgentEvent>,
    command_tx: mpsc::Sender<TuiCommand>,
    project_name: &str,
    initial_status: StatusBar,
) -> anyhow::Result<()> {
    // Install panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut state = TuiState {
        entries: vec![ChatEntry::Info(format!(
            "Working in {}. Ask me to read, write, or edit code.",
            project_name
        ))],
        input: String::new(),
        input_cursor: 0,
        scroll_offset: 0,
        status: initial_status,
        is_processing: false,
        streaming_text: String::new(),
        project_name: project_name.to_string(),
    };

    loop {
        terminal.draw(|frame| render(frame, &state))?;

        // Poll for events with a short timeout so we can check agent events
        let timeout = std::time::Duration::from_millis(50);

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    match handle_key(key, &mut state) {
                        KeyAction::Send => {
                            let msg = state.input.trim().to_string();
                            state.input.clear();
                            state.input_cursor = 0;

                            if msg.is_empty() {
                                continue;
                            }

                            if msg == "/clear" {
                                let _ = command_tx.send(TuiCommand::Clear).await;
                                state.entries.clear();
                                state.push_entry(ChatEntry::Info(
                                    "Conversation cleared.".to_string(),
                                ));
                                state.streaming_text.clear();
                                continue;
                            }

                            if msg == "/help" {
                                state.push_entry(ChatEntry::Info(HELP_TEXT.to_string()));
                                continue;
                            }

                            state.push_entry(ChatEntry::UserMessage(msg.clone()));
                            state.is_processing = true;
                            state.streaming_text.clear();
                            state.scroll_offset = 0;
                            let _ = command_tx.send(TuiCommand::SendMessage(msg)).await;
                        }
                        KeyAction::Quit => {
                            let _ = command_tx.send(TuiCommand::Quit).await;
                            break;
                        }
                        KeyAction::Cancel => {
                            if state.is_processing {
                                let _ = command_tx.send(TuiCommand::Cancel).await;
                            } else {
                                state.input.clear();
                                state.input_cursor = 0;
                            }
                        }
                        KeyAction::ScrollUp => {
                            state.scroll_offset = state.scroll_offset.saturating_add(3);
                        }
                        KeyAction::ScrollDown => {
                            state.scroll_offset = state.scroll_offset.saturating_sub(3);
                        }
                        KeyAction::None => {}
                    }
                }
                Event::Resize(_, _) => {} // terminal.draw handles resize
                _ => {}
            }
        }

        // Process agent events (non-blocking)
        while let Ok(event) = agent_events.try_recv() {
            match event {
                AgentEvent::TextDelta(text) => {
                    state.streaming_text.push_str(&text);
                    state.scroll_offset = 0; // auto-scroll on new content
                }
                AgentEvent::ToolStart { name, args_display } => {
                    // Flush any streaming text
                    if !state.streaming_text.is_empty() {
                        let text = state.streaming_text.clone();
                        state.streaming_text.clear();
                        state.push_entry(ChatEntry::AssistantText(text));
                    }
                    state.push_entry(ChatEntry::ToolStart {
                        name,
                        args: args_display,
                    });
                    state.scroll_offset = 0;
                }
                AgentEvent::ToolResult {
                    name,
                    output,
                    success,
                } => {
                    state.push_entry(ChatEntry::ToolResult {
                        name,
                        output,
                        success,
                    });
                    state.scroll_offset = 0;
                }
                AgentEvent::TokenRate { tok_per_sec } => {
                    state.status.tok_per_sec = Some(tok_per_sec);
                }
                AgentEvent::ContextUsage { used, max } => {
                    state.status.ctx_used = used;
                    state.status.ctx_max = max;
                }
                AgentEvent::Warning(msg) => {
                    state.push_entry(ChatEntry::Warning(msg));
                }
                AgentEvent::TurnComplete => {
                    if !state.streaming_text.is_empty() {
                        let text = state.streaming_text.clone();
                        state.streaming_text.clear();
                        state.push_entry(ChatEntry::AssistantText(text));
                    }
                    state.is_processing = false;
                    state.scroll_offset = 0;
                }
                AgentEvent::ServerRestarted { attempt } => {
                    state.push_entry(ChatEntry::Warning(format!(
                        "\u{26a0} llama-server restarted ({}/3). Retrying...",
                        attempt
                    )));
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

struct TuiState {
    entries: Vec<ChatEntry>,
    input: String,
    input_cursor: usize,
    scroll_offset: usize,
    status: StatusBar,
    is_processing: bool,
    streaming_text: String,
    project_name: String,
}

impl TuiState {
    /// Push an entry, evicting old entries if we exceed the cap.
    fn push_entry(&mut self, entry: ChatEntry) {
        self.entries.push(entry);
        if self.entries.len() > MAX_ENTRIES {
            // Remove oldest quarter to avoid doing this every push
            let remove = MAX_ENTRIES / 4;
            self.entries.drain(..remove);
        }
    }
}

enum KeyAction {
    Send,
    Quit,
    Cancel,
    ScrollUp,
    ScrollDown,
    None,
}

/// Get the byte offset of the character before the given byte offset.
fn prev_char_boundary(s: &str, byte_pos: usize) -> usize {
    s[..byte_pos]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Get the byte offset of the character after the given byte offset.
fn next_char_boundary(s: &str, byte_pos: usize) -> usize {
    s[byte_pos..]
        .char_indices()
        .nth(1)
        .map(|(i, _)| byte_pos + i)
        .unwrap_or(s.len())
}

fn handle_key(key: KeyEvent, state: &mut TuiState) -> KeyAction {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => KeyAction::Quit,
        (KeyCode::Esc, _) => KeyAction::Cancel,
        (KeyCode::Enter, KeyModifiers::SHIFT) => {
            state.input.insert(state.input_cursor, '\n');
            state.input_cursor += '\n'.len_utf8();
            KeyAction::None
        }
        (KeyCode::Enter, _) => KeyAction::Send,
        (KeyCode::Up, _) if state.input.is_empty() => KeyAction::ScrollUp,
        (KeyCode::Down, _) if state.input.is_empty() => KeyAction::ScrollDown,
        (KeyCode::Backspace, _) => {
            if state.input_cursor > 0 {
                let prev = prev_char_boundary(&state.input, state.input_cursor);
                state.input.drain(prev..state.input_cursor);
                state.input_cursor = prev;
            }
            KeyAction::None
        }
        (KeyCode::Delete, _) => {
            if state.input_cursor < state.input.len() {
                let next = next_char_boundary(&state.input, state.input_cursor);
                state.input.drain(state.input_cursor..next);
            }
            KeyAction::None
        }
        (KeyCode::Left, _) => {
            if state.input_cursor > 0 {
                state.input_cursor = prev_char_boundary(&state.input, state.input_cursor);
            }
            KeyAction::None
        }
        (KeyCode::Right, _) => {
            if state.input_cursor < state.input.len() {
                state.input_cursor = next_char_boundary(&state.input, state.input_cursor);
            }
            KeyAction::None
        }
        (KeyCode::Home, _) => {
            state.input_cursor = 0;
            KeyAction::None
        }
        (KeyCode::End, _) => {
            state.input_cursor = state.input.len();
            KeyAction::None
        }
        (KeyCode::Char(c), _) => {
            state.input.insert(state.input_cursor, c);
            state.input_cursor += c.len_utf8();
            KeyAction::None
        }
        _ => KeyAction::None,
    }
}

fn render(frame: &mut Frame, state: &TuiState) {
    let area = frame.area();

    // Layout: title bar (1), chat area (flexible), input (3), status bar (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // title
            Constraint::Min(5),    // chat
            Constraint::Length(3), // input
            Constraint::Length(1), // status bar
        ])
        .split(area);

    // Title bar
    let title = format!(" cinderella v0.1.0 \u{00b7} {} ", state.project_name);
    let title_widget = Paragraph::new(title)
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));
    frame.render_widget(title_widget, chunks[0]);

    // Chat area
    let chat_text = render_entries(&state.entries, &state.streaming_text, state.is_processing);
    let chat_lines: Vec<Line> = chat_text
        .lines()
        .map(|l| Line::from(l.to_string()))
        .collect();

    let visible_height = chunks[1].height as usize;
    let total_lines = chat_lines.len();
    let scroll = if total_lines > visible_height {
        (total_lines - visible_height).saturating_sub(state.scroll_offset)
    } else {
        0
    };

    let chat = Paragraph::new(chat_lines)
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(chat, chunks[1]);

    // Input box
    let input_title = if state.is_processing {
        " thinking... "
    } else {
        " > "
    };
    let input = Paragraph::new(state.input.as_str())
        .block(Block::default().borders(Borders::TOP | Borders::BOTTOM).title(input_title))
        .wrap(Wrap { trim: false });
    frame.render_widget(input, chunks[2]);

    // Cursor position in input (count characters, not bytes)
    if !state.is_processing {
        let char_offset = state.input[..state.input_cursor].chars().count();
        let cursor_x = chunks[2].x + char_offset as u16;
        let cursor_y = chunks[2].y + 1; // +1 for top border
        if cursor_x < chunks[2].right() {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    // Status bar
    let status_text = state.status.render_text();
    let status_style = if std::env::var("NO_COLOR").is_ok() {
        Style::default()
    } else {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    };
    let status = Paragraph::new(status_text).style(status_style);
    frame.render_widget(status, chunks[3]);
}

fn render_entries(entries: &[ChatEntry], streaming: &str, is_processing: bool) -> String {
    let mut lines = Vec::new();

    for entry in entries {
        match entry {
            ChatEntry::UserMessage(msg) => {
                lines.push(String::new());
                lines.push(format!("  You: {}", msg));
            }
            ChatEntry::AssistantText(text) => {
                lines.push(String::new());
                for line in text.lines() {
                    lines.push(format!("  {}", line));
                }
            }
            ChatEntry::ToolStart { name, args } => {
                lines.push(String::new());
                lines.push(format!("  \u{25cf} {} {}", name, args));
            }
            ChatEntry::ToolResult {
                name: _,
                output,
                success,
            } => {
                let indicator = if *success { "\u{2502}" } else { "\u{2502} \u{2717}" };
                for line in output.lines() {
                    lines.push(format!("  {} {}", indicator, line));
                }
            }
            ChatEntry::Warning(msg) => {
                lines.push(format!("  \u{26a0} {}", msg));
            }
            ChatEntry::Info(msg) => {
                lines.push(String::new());
                for line in msg.lines() {
                    lines.push(format!("  {}", line));
                }
            }
        }
    }

    // Append streaming text
    if !streaming.is_empty() {
        lines.push(String::new());
        for line in streaming.lines() {
            lines.push(format!("  {}", line));
        }
        if is_processing {
            lines.push("  \u{2588}".to_string()); // block cursor
        }
    } else if is_processing {
        lines.push(String::new());
        lines.push("  thinking...".to_string());
    }

    lines.join("\n")
}

const HELP_TEXT: &str = "\
Keybindings:
  Enter       \u{2192} Send message
  Shift-Enter \u{2192} Newline in input
  Esc         \u{2192} Cancel running tool / clear input
  Ctrl-C      \u{2192} Quit
  Up/Down     \u{2192} Scroll conversation
  /help       \u{2192} Show this help
  /clear      \u{2192} Clear conversation";
