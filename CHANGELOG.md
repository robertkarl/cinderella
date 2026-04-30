# Changelog

All notable changes to this project will be documented in this file.

## [0.1.1.0] - 2026-04-30

### Added

- Network-debug diagnostic runbook system prompt with 7-step structured workflow
- SafetyProfile enum (Coding, NetworkDebug) controlling yah-core auto-allow lists
- `--playbook network-debug` flag to activate network-debug profile
- `-p` / `--prompt` flag for non-interactive prompt mode (send one prompt, stream output, exit)
- Docker demo target: flaky Flask service returning 503 every 3rd request
- MAX_AGENT_ITERATIONS=25 guard to prevent infinite loops in -p mode
- 2 new tests for network-debug safety profile (curl allowed, pipe-to-shell denied)

### Changed

- Orchestrator refactored: extracted shared `spawn_agent_loop()` function (was duplicated in run/run_remote)
- Event printing extracted to `print_event()` function shared between TUI and -p mode
- Bundled model updated to Qwen3.5-35B-MoE Q4_K_M
- Tool result indicators changed to ASCII (from Unicode)

### Fixed

- traceroute timeout advice in diagnostic prompt now uses `timeout 15` wrapper
- Network-debug prompt no longer falsely claims "ONE tool: bash"

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
