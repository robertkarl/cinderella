# MCP Companion — Glass Slipper as Claude's Local Sidekick

**Date:** 2026-05-11
**Status:** Approved for implementation
**Premise:** Can a local model save real money on agentic coding by handling grunt work that Claude currently does at cloud prices?

## Motivation

Cloud AI coding agents consume tokens on mechanical tasks — reading build output, explaining code, fetching web pages — at frontier-model prices. A local model (Qwen 3.5 9B on Metal) can handle these tasks for free. Glass Slipper already bundles llama-server and manages model downloads. This spec adds an MCP bridge so Claude Code can delegate to the local model, plus a companion window showing what was delegated and how much was saved.

This is an experiment. Ship wide, observe, narrow.

## Architecture

Three components, one app bundle:

```
┌─────────────────────────────────────────────┐
│  Glass Slipper.app                          │
│  ┌──────────────┐  ┌─────────────────────┐  │
│  │ llama-server  │  │ Companion Window    │  │
│  │ (child proc)  │  │ (AppKit)            │  │
│  │ :8080         │  │ - Setup checklist   │  │
│  └──────┬───────┘  │ - Savings dashboard  │  │
│         │ HTTP      │ - Activity log       │  │
│         │           └──────────┬──────────┘  │
│  ┌──────┴───────┐              │              │
│  │ glass-slipper│  writes      │  reads       │
│  │ -mcp (binary)│──────► JSONL log ──────────►│
│  │ stdio ↔ HTTP │              │              │
│  └──────────────┘              │              │
└─────────────────────────────────────────────┘
         ▲ stdio
         │
   Claude Code (spawns glass-slipper-mcp as MCP server)
```

- **Glass Slipper.app** manages llama-server lifecycle, model downloads, and the companion window.
- **glass-slipper-mcp** is a thin Rust binary bundled at `GlassSlipper.app/Contents/MacOS/glass-slipper-mcp`. It speaks MCP stdio protocol on one side and HTTP to llama-server on the other. Stateless — all state lives in the app.
- **Logging:** Every MCP tool call is logged to `~/Library/Application Support/Glass Slipper/mcp-activity.jsonl`. The companion window reads this file to populate the activity feed and savings stats.

### Why MCP (not raw HTTP or hooks)

Claude Code natively discovers and calls MCP tools. The alternative — teaching Claude to hit localhost via prompt instructions or Bash — is fragile, requires manual configuration, and gives Claude no structured way to report what it delegated. MCP turns "configure an HTTP endpoint and teach Claude to use it" into "add one config line."

PostToolUse hooks were considered for intercepting build output, but they cannot modify tool results — only add context. PreToolUse hooks can rewrite input but require string-munging shell commands. MCP is the clean path.

## MCP Tools

Seven tools, all hitting llama-server at `localhost:8080/v1/chat/completions`. Each tool has a task-specific system prompt tuned for Qwen 3.5 9B.

### local_summarize

- **Input:** `command: string` — a shell command to run
- **Behavior:** MCP binary executes the command, captures stdout/stderr, sends output to Qwen with a summarization system prompt
- **Returns:** 1-5 sentence summary (pass/fail, key details, errors if any)
- **Use case:** Build output triage. `xcodebuild`, `cargo build`, `npm test` — anything that dumps hundreds of lines where Claude only needs "did it work and if not, why"

### local_explain

- **Input:** `code: string` — a code snippet or file contents
- **Behavior:** Sends code to Qwen with an explanation system prompt
- **Returns:** Plain-English explanation of what the code does
- **Use case:** "What does this function do?" — Qwen 9B's sweet spot

### local_ask

- **Input:** `question: string`, `context?: string` — a question with optional context
- **Behavior:** General-purpose Q&A via Qwen
- **Returns:** Answer
- **Use case:** Catch-all for simple questions Claude thinks the local model can handle

### local_web_fetch

- **Input:** `url: string`, `question: string` — a URL and a question about its content
- **Behavior:** MCP binary fetches the URL via HTTP, converts HTML to markdown, sends markdown + question to Qwen
- **Returns:** Extracted answer (2-3 sentences typical)
- **Use case:** Web pages are enormous in token count. Local model extracts the answer and returns a fraction of the tokens.

### local_review

- **Input:** `diff: string` — a git diff or similar
- **Behavior:** Sends diff to Qwen with a review/summarization system prompt
- **Returns:** Summary of what changed
- **Use case:** Experimental — included for data collection. Quality may be marginal.

### local_draft

- **Input:** `task: string`, `context?: string` — what to generate, with optional context
- **Behavior:** Sends to Qwen with a drafting system prompt
- **Returns:** Generated content (code, text, boilerplate)
- **Use case:** Experimental — included for data collection. Qwen 9B struggles with ambiguous code generation, but boilerplate may be fine.

### local_status

- **Input:** (none)
- **Behavior:** Checks llama-server health endpoint, reads model info
- **Returns:** Model name, quantization, running/stopped, tok/s, port
- **Use case:** Diagnostics. No LLM call.

### System Prompts

Each tool gets a task-specific system prompt, not a shared generic one. These prompts tell Qwen:

- It is a local offload model, not the primary agent
- Be concise — no rambling, no follow-up questions, no disclaimers
- What good output looks like for this specific task type
- Don't hallucinate — if unsure, say so in one sentence

Crafting and testing these prompts against real inputs (actual xcodebuild output, real code snippets, real diffs) is an explicit deliverable. The prompts are the highest-leverage quality lever for this experiment.

### Logging

Every tool call logs one JSONL line to `~/Library/Application Support/Glass Slipper/mcp-activity.jsonl`:

```json
{
  "ts": "2026-05-11T11:42:00Z",
  "tool": "local_summarize",
  "input_tokens": 3200,
  "output_tokens": 18,
  "latency_ms": 940,
  "estimated_cloud_cost_usd": 0.048,
  "model": "qwen3.5-9b-q5_k_m"
}
```

`estimated_cloud_cost_usd` is what those input tokens would have cost at Opus rates ($15/M input). This feeds the savings dashboard. It is an estimate — we do not track Claude's actual token usage.

## Companion Window

### Layout: Hybrid (Setup → Dashboard)

**First-run state** — setup checklist with three steps:

1. **Model** — "Qwen 3.5 9B · Q5_K_M · 6.1 GB" — [Download] button if not present, green checkmark if downloaded
2. **Server** — "llama-server · Port 8080" — [Start] if stopped, green "Running" if active
3. **MCP** — "Claude Code integration" — [Install MCP] button that writes config

Once all three steps are complete, the checklist collapses to a single status line. The window transitions to dashboard mode.

**Daily-use state** — dashboard:

- **Status bar** (collapsed): green dot, model name, "MCP Connected", expandable "Setup" link
- **Stats bar**: $ saved today, tasks delegated, tokens saved
- **Activity log**: scrolling feed — timestamp, tool name, token compression ratio (e.g., "3,200→18 tok"), cost saved per call

### Install MCP Button

Writes to `~/.claude.json`:

```json
{
  "mcpServers": {
    "glass-slipper": {
      "command": "/Applications/Glass Slipper.app/Contents/MacOS/glass-slipper-mcp"
    }
  }
}
```

One click. No terminal, no npm, no config files to find.

## Data Flow Example

**Without Glass Slipper (current):**
1. Claude calls `Bash("xcodebuild -project ... build 2>&1")`
2. 200 lines of build output land in context (~3,200 tokens)
3. Claude reads all of it to determine pass/fail
4. Those 3,200 tokens stay in context for every subsequent message
5. Cost: ~$0.048 at Opus input rates, compounding on every future turn

**With Glass Slipper:**
1. Claude calls `local_summarize("xcodebuild -project ... build 2>&1")`
2. `glass-slipper-mcp` runs the command, captures stdout
3. MCP binary sends output to Qwen via llama-server
4. Qwen returns: "BUILD SUCCEEDED" (2 tokens)
5. MCP binary logs the call to JSONL
6. Claude sees only the 2-token summary
7. Token reduction: 99.4% on this call, plus savings on every subsequent turn

## Quality

No explicit quality gating machinery. Claude's natural judgment handles bad responses — if a tool returns garbage, Claude recognizes it and either retries or does the work itself. Claude is the quality gate.

## Not in Scope

- **Adaptive model sizing** — separate spec. This assumes one model (Qwen 3.5 9B) is running.
- **Remote inference** — MCP talks to localhost only.
- **`local_web_search`** — needs search backend infrastructure. Deferred.
- **Automatic delegation via hooks** — Claude chooses when to use the tools. No interception.
- **Multi-model support** — one model at a time.
- **Codex/other agent support** — Claude Code only for now. Config path may differ for other agents.

## Prior Art

[mcp-local-llm](https://github.com/aplaceforallmystuff/mcp-local-llm) (5-6 GitHub stars) implements a similar concept with 7 tools against Ollama. Glass Slipper's differentiators:

- **Zero-config install** — no Ollama, no npm, no clone-and-build. One app download + one click.
- **Bundled inference** — llama-server ships inside the app with Metal shaders.
- **Observability** — savings dashboard and activity log. mcp-local-llm has no tracking.
- **Tuned prompts** — task-specific system prompts for Qwen 3.5, not generic completions.
