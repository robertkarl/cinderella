# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0.0] - 2026-04-24

### Added

- CLI entry point: `cinderella <project> [--model path] [--port N] [--llama-server path]`
- SSE streaming LLM client for OpenAI-compatible endpoints (llama-server)
- JSON repair pipeline for malformed tool calls from local models
- Agent loop with context management (truncation, summarization, 80% warning)
- 5 tools: read_file, write_file, edit_file, bash, ls
- Bash tool: 120s timeout, process group kill, yah-core safety classification
- Edit tool: exact-match guard (rejects if old_string not unique)
- Path traversal protection on all file tools
- Ratatui TUI with chat display, input box, status bar
- Claude Code parity keybindings (Enter, Shift-Enter, Esc, Ctrl-C, Up/Down, /help, /clear)
- Status bar: model name, quant, tok/s, RAM usage, context utilization, GPU layers
- Hardware detection via sysctl (total RAM, available RAM, chip name)
- llama-server lifecycle management (start, health check, SIGTERM/SIGKILL stop)
- Port collision detection on server start
- Hardcoded model registry: Qwen3.5-9B-abliterated Q4_K_M
- RAM check includes KV cache overhead (requires ~10 GiB for bundled model)
- NO_COLOR support for accessibility
- TUI panic hook to restore terminal state
- 32 unit tests
