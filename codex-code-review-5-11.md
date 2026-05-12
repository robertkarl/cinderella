# Codex Code Review - 2026-05-11

Scope: read-only review of Rust and Swift source in this repository, prioritizing security and maintainability. The review included `src/`, `tests/`, and `glass-slipper/`.

Repository instruction considered: Glass Slipper builds, packages, and distributes its own `llama-server` binary inside the app bundle. Do not use Homebrew or system `llama-server`.

Tests were not run for this review.

## Summary

The highest-risk issues are:

1. MCP shell tools run arbitrary commands without the main app's safety layer.
2. `llama-server` discovery still permits binaries outside the app bundle.
3. Existing model files are trusted without startup SHA verification.
4. MCP web fetch can reach arbitrary hosts and can exhaust memory or panic.

## Findings

### High: MCP command tools bypass safety controls

Evidence:

- `src/mcp/tools.rs:13` and `src/mcp/tools.rs:31` advertise `local_summarize` and `local_pass_fail` as shell command runners.
- `src/mcp/tools.rs:179` dispatches command strings directly to handlers.
- `src/mcp/handlers.rs:150` executes caller-provided strings through `sh -c`.

Why it matters:

This bypasses the main app's bash classifier, project cwd boundary, timeout, process-group cleanup, and output caps. An MCP caller can run commands like `cat ~/.ssh/id_ed25519`, `rm -rf ~/Documents`, or `curl ... | sh`. Commands like `yes` or `cat /dev/zero` can also hang or exhaust memory because output is captured without an enforced size limit.

Recommended fix:

Route MCP command execution through the existing `src/tools/bash.rs` safety path or a shared command-execution module. Require an explicit working directory, classify commands, deny dangerous capabilities, enforce a timeout, cap stdout/stderr bytes, and kill the process group on timeout.

### High: `llama-server` can still come from outside the app bundle

Evidence:

- `src/main.rs:41` exposes `--llama-server`.
- `src/orchestrator.rs:478` falls back to `~/.glass-slipper/bin/llama-server`.
- `src/orchestrator.rs:484` falls back to `PATH` via `which`.
- `src/orchestrator.rs:489` recommends `brew install llama.cpp`.
- `glass-slipper/AppDelegate.swift:443` has development Homebrew fallbacks.
- `glass-slipper/GlassSlipperTests/GlassSlipperTests.swift:14` tests Homebrew/system paths.
- `tests/mcp_integration.rs:88` also hard-codes Homebrew paths.

Why it matters:

This violates the repo instruction and weakens the packaging guarantee. A malicious or mismatched `llama-server` in `PATH`, Homebrew, or a user-writable fallback location could receive local prompt/model traffic or behave differently from the bundled binary.

Recommended fix:

Make bundled `Contents/MacOS/llama-server` the only accepted binary for normal app and release flows. If a developer override remains, gate it behind an explicit dev-only flag or build setting and keep tests from passing because Homebrew happens to be installed. Update error text and tests so they enforce bundled-binary completeness.

### Medium: Managed model files are trusted without startup SHA verification

Evidence:

- `glass-slipper/ModelDownloadManager.swift:73` accepts an existing model by file size only.
- `glass-slipper/AppDelegate.swift:81` enables diagnosis when that size check passes.
- `glass-slipper/CompanionWindowController.swift:30` marks the model downloaded by path existence only.
- `src/orchestrator.rs:55` accepts user-provided model paths by existence.
- `src/orchestrator.rs:417` finds managed model paths by existence.
- `src/model_manifest.rs:149` implements `verify_sha256()`, but startup does not use it.
- `model-manifest.json:10` and `model-manifest.json:42` still contain placeholder hashes for non-default models.

Why it matters:

Downloads are verified after completion, but later launches trust files already on disk. A corrupted or replaced GGUF with the expected size can be passed into `llama-server`, causing crashes or worse if the native model parser has a file-format vulnerability.

Recommended fix:

Use manifest SHA verification before launching managed models, and store verification state carefully if full hashing on every launch is too expensive. At minimum, verify when size, mtime, inode, manifest version, or app version changes. Do not allow placeholder hashes in release manifests.

### Medium: `local_web_fetch` is an SSRF and DoS surface

Evidence:

- `src/mcp/handlers.rs:92` fetches arbitrary URLs.
- `src/mcp/handlers.rs:97` reads the entire response body before limiting.
- `src/mcp/handlers.rs:103` slices the string at byte offset `16000`, which can panic on a UTF-8 boundary.

Why it matters:

An MCP caller can fetch localhost or private-network services, retrieve very large responses into memory, or crash the MCP server with multibyte UTF-8 near the truncation boundary.

Recommended fix:

Restrict allowed schemes and optionally block loopback, link-local, and private network targets unless explicitly enabled. Stream with a hard byte cap, enforce content-type and size limits, and truncate on character boundaries.

### Medium: MCP install status can report stale or malicious config as installed

Evidence:

- `glass-slipper/MCPInstaller.swift:24` only checks whether `mcpServers["glass-slipper"]` exists.
- `glass-slipper/MCPInstaller.swift:36` writes the expected command path, but `isInstalled` does not validate it.

Why it matters:

If `~/.claude.json` contains a `glass-slipper` entry pointing at `/tmp/evil` or an old deleted app path, the companion window can report MCP as configured even though Claude Code will run the wrong binary or fail.

Recommended fix:

Validate that `command` equals the current bundled `glass-slipper-mcp` path and that the file is executable. Surface stale or mismatched configs as repairable, not installed.

### Medium: `~/.claude.json` is rewritten non-atomically

Evidence:

- `glass-slipper/MCPInstaller.swift:65` rewrites the entire config through `data.write(to:)`.

Why it matters:

A crash, disk-full condition, or concurrent Claude Code edit can corrupt or lose config. The installer also normalizes the whole file with pretty-printed sorted JSON, which increases churn and makes unrelated config changes harder to review.

Recommended fix:

Write to a temp file in the same directory, fsync if practical, then atomically replace. Consider creating a backup before first write. Re-read before replace or use a compare-and-swap style guard to avoid silently clobbering concurrent edits.

### Medium: Swift model selection is hardcoded while Rust is manifest-driven

Evidence:

- `glass-slipper/AppDelegate.swift:470` hardcodes the 9B model filename and Application Support path.
- Rust selects a model from `model-manifest.json` by RAM in `src/orchestrator.rs:49`.

Why it matters:

The app has an adaptive model design, but Swift launch logic can still force the 9B model path. This makes small/large model support fragile and creates multiple sources of truth for model identity.

Recommended fix:

Make Swift use the manifest for model path, display name, size, and selected tier. Avoid filename parsing for friendly names; carry the selected `ModelDefinition` through launch.

### Low: Server health is not tied to the spawned child process

Evidence:

- `src/server.rs:43` checks whether the port is free.
- `src/server.rs:55` spawns `llama-server` after that listener is dropped.
- `src/server.rs:80` accepts any successful `127.0.0.1:<port>/health` response.
- Later chat traffic goes through `src/llm.rs:114`.

Why it matters:

A local process could win the port race or an existing compatible service could answer health checks, causing prompt traffic to route to the wrong process.

Recommended fix:

Prefer binding in a way that avoids the time-of-check/time-of-use gap if possible, or verify the spawned process is still running and that the health endpoint exposes an expected server identity. Use a random free port when the caller did not specify one.

### Low: Process, UI, and log buffers are unbounded

Evidence:

- `glass-slipper/AppDelegate.swift:497` appends stdout data until a newline.
- `glass-slipper/CinderellaScaffold.swift:921` accumulates event rows without pruning.
- `glass-slipper/MCPActivityLog.swift:76` parses and retains activity entries indefinitely.

Why it matters:

A helper bug emitting a huge unterminated line, a long diagnosis session, or a long-lived MCP activity log can stall the UI or exhaust memory.

Recommended fix:

Cap line-buffer size, cap row count or virtualize rows, and retain only a bounded activity window plus persisted summary totals.

### Low: Final stdout can be dropped at termination

Evidence:

- `glass-slipper/AppDelegate.swift:698` removes file-handle observers in `taskDidTerminate`.
- The nearby comment acknowledges a race with final stdout delivery.

Why it matters:

If the helper writes the final `diagnosis` or `done` JSON and exits quickly, the UI can show an incomplete run or an error despite successful output.

Recommended fix:

Drain stdout/stderr in the termination path before removing observers and finalizing UI state.

## Test And Verification Gaps

- No release-bundle test verifies that `glass-slipper-agent`, `glass-slipper-mcp`, `llama-server`, and `model-manifest.json` are all present and executable/readable in the app bundle.
- No test enforces bundled-only `llama-server` behavior.
- No startup test verifies managed model SHA checking.
- MCP command tests do not exercise safety policy, timeout behavior, process cleanup, output caps, or working directory restrictions.
- MCP web fetch tests do not cover private-network blocking, large bodies, invalid UTF-8 boundaries, or streaming limits.
- MCP installer tests do not verify stale command detection, atomic writes, backups, or concurrent-edit behavior.
- Swift tests still encode Homebrew/system `llama-server` expectations.
- Rust MCP tests appear stale: unit tests expect eight tools, while integration tests reportedly expect seven and omit required `context_tokens` in some calls.

## Suggested Remediation Order

1. Disable or safety-wrap MCP shell execution.
2. Remove non-bundled `llama-server` paths from release/default flows and update tests.
3. Enforce model SHA verification for managed models and eliminate placeholder hashes from release manifests.
4. Harden `local_web_fetch`.
5. Repair MCP installer validation and atomic writes.
6. Unify Swift model selection with the manifest.
7. Add buffer caps and final stdout draining.
