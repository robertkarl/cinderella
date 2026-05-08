# Plan: Add `-p` non-interactive prompt mode

## Context
Glass Slipper currently only runs interactively. You want `glass-slipper -p "fix the bug"` — send one prompt, print output, exit. Like `claude -p`.

## Approach: Bypass channels, call agent directly

The agent's `process_message()` already takes an `FnMut(AgentEvent)` callback. In `-p` mode we skip the TUI/channel architecture entirely and call the agent directly on the main thread with a print callback.

## Changes

### 1. `src/main.rs` — add `-p` flag
- Add `#[arg(short = 'p', long = "prompt")] prompt: Option<String>` to `Cli`
- Pass it through to `OrchestratorConfig`

### 2. `src/tui.rs` — extract event printing
- Add `pub struct OutputState { in_thinking, in_text }` 
- Extract the event match block into `pub fn print_event(event: AgentEvent, state: &mut OutputState) -> bool`
- Refactor `run()` to call `print_event()` internally

### 3. `src/orchestrator.rs` — branch on mode
- Add `pub prompt: Option<String>` to `OrchestratorConfig`
- Add `run_prompt()` (~15 lines): creates Agent, calls `process_message` with print callback
- After server startup, if `cfg.prompt.is_some()`, call `run_prompt()` then stop server and return

### 4. `src/agent.rs` — no changes

## Files
- `/Users/robertkarl/Code/glass-slipper/src/main.rs`
- `/Users/robertkarl/Code/glass-slipper/src/orchestrator.rs`
- `/Users/robertkarl/Code/glass-slipper/src/tui.rs`

## Verification
- `cargo build`
- `glass-slipper /some/project -p "list the files in this project"` — should print output and exit
- Interactive mode (`glass-slipper /some/project`) still works as before
