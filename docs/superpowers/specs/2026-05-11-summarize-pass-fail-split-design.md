# Split local_summarize into general summarizer + pass/fail checker

**Date:** 2026-05-11

## Problem

`local_summarize` has a system prompt hardcoded to report "Pass or fail (one word)" for all command output. When used on non-build output (e.g., a PDF, a log file, arbitrary text), the LLM sees no errors and returns "Pass" — useless.

The tool name says "summarize" but the behavior says "check build status."

## Design

Split into two tools with distinct semantics:

### `local_summarize` (existing, new behavior)

- **Purpose:** Run a shell command and return a general-purpose summary of its output.
- **Input schema:** `{ command: string }` (unchanged)
- **Max tokens:** 512 (unchanged)
- **System prompt:** Summarize the command output in 2-5 sentences. Describe what the output contains and any key information. Note errors or failures if present, but do not assume a pass/fail framing. Do not reproduce the raw output.
- **Tool description (for Claude):** "Run a shell command and return a concise summary of its output. Use this to keep verbose command output out of context. Saves tokens by having a local model read the output and report the key points."

### `local_pass_fail` (new tool)

- **Purpose:** Run a shell command and report pass or fail with relevant details.
- **Input schema:** `{ command: string }`
- **Max tokens:** 512
- **System prompt:** The current `SUMMARIZE` prompt verbatim — "Pass or fail (one word), errors with file/line if failed, warnings if passed."
- **Tool description (for Claude):** "Run a shell command and report pass or fail. Use this for builds, test suites, and linters where you only need to know if it succeeded and what went wrong if it didn't."

## Files to change

- `src/mcp/prompts.rs` — Rewrite `SUMMARIZE` to be general-purpose. Add `PASS_FAIL` constant with the old pass/fail prompt.
- `src/mcp/tools.rs` — Update `local_summarize` tool description. Add `local_pass_fail` tool definition. Add dispatch match arm.
- `src/mcp/handlers.rs` — Add `handle_pass_fail` handler (identical to `handle_summarize` but uses `prompts::PASS_FAIL`).
- Tests — Update tool count assertion (7 → 8), add prompt assertion for `PASS_FAIL`, update `SUMMARIZE` prompt assertion.

## What does not change

- All other tools (`local_explain`, `local_ask`, `local_web_fetch`, `local_review`, `local_draft`, `local_status`)
- `llm_client.rs`, `logger.rs`, `protocol.rs`, `main.rs`
- The MCP protocol handshake or JSON-RPC handling
