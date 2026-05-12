# Glass Slipper Testing Strategy

## Summary

Three-tier testing: Rust unit tests (fast, no server), Rust integration tests (spawns llama-server, real inference), and Swift XCTests (AppDelegate wiring, state machine, path logic).

## Tier 1: Rust Unit Tests (`cargo test`)

Existing tests plus new additions. No server, no model file required.

### New unit tests

**Port conflict detection** (`src/server.rs`):
- Test that `ServerManager::start()` returns an appropriate error when the port is already bound.
- Bind a TCP listener on a random port, try to start the server on that port, assert error message mentions "already in use".

**MCP tool dispatch** (`src/mcp/tools.rs`):
- Test that all 7 tool names are recognized by `dispatch()` (currently only tests definitions, not routing).

**MCP handler argument extraction** (`src/mcp/handlers.rs`):
- Test that each handler correctly extracts its expected arguments from the JSON params.
- Test that missing required arguments produce an error, not a panic.

## Tier 2: Rust Integration Tests (`cargo test --features integration`)

**Location:** `tests/mcp_integration.rs`

**Feature gate:** `#[cfg(feature = "integration")]` in Cargo.toml.

### Test harness

A shared setup that:
1. Checks for model file at `~/Library/Application Support/Glass Slipper/Models/Qwen3.5-9B-Q5_K_M.gguf` — panics with a clear message if missing.
2. Finds `llama-server` via the same search paths as the app (`/opt/homebrew/bin/llama-server`, `/usr/local/bin/llama-server`).
3. Binds port 0 to get a random available port from the OS, then drops the listener.
4. Spawns `llama-server --model <path> --port <random> --ctx-size 4096 --n-gpu-layers 99 --jinja`.
5. Polls `http://127.0.0.1:<port>/health` every 1s for up to 120s.
6. Provides a helper to send JSON-RPC to `glass-slipper-mcp` via stdin/stdout with `GLASS_SLIPPER_URL` set to `http://127.0.0.1:<port>`.
7. Kills the llama-server process on drop.

All integration tests share one server instance (use `std::sync::LazyLock` or similar).

### Test cases

**`test_summarize_not_garbage`**
- Creates a temp text file with a paragraph about Pikachu's electric abilities.
- Sends `local_summarize` with command `cat <tempfile>`.
- Asserts: response is >20 chars, contains "Pikachu" or "electric" (case-insensitive).
- This catches the "Pass" QA bug.

**`test_ask_simple_question`**
- Sends `local_ask` with question "What is the capital of France?"
- Asserts: response contains "Paris".

**`test_explain_code_snippet`**
- Sends `local_explain` with a simple Rust function (`fn add(a: i32, b: i32) -> i32 { a + b }`).
- Asserts: response is >30 chars, not just the code echoed back.

**`test_status_reports_healthy`**
- Sends `local_status`.
- Asserts: response contains "running" or "ok" or "healthy" (case-insensitive).

**`test_draft_generates_code`**
- Sends `local_draft` with prompt "Write a Python hello world function".
- Asserts: response contains "def" or "print".

**`test_review_diff`**
- Sends `local_review` with a simple unified diff.
- Asserts: response is >20 chars.

**`test_web_fetch`** (optional, requires network)
- Sends `local_web_fetch` with a stable URL (e.g., `https://example.com`) and question "What is this page about?"
- Asserts: response mentions "example" or "domain".

## Tier 3: Swift XCTests (`xcodebuild test`)

**New test target:** `glass-slipperTests` in the Xcode project.

### AppDelegateTests

**`testStartLlamaServerBuildsCorrectArguments`**
- Extract the server argument construction into a testable function (e.g., `buildLlamaServerArguments(modelPath:port:ctxSize:gpuLayers:) -> [String]`).
- Assert it produces `["--model", path, "--port", "8787", "--ctx-size", "4096", "--n-gpu-layers", "99", "--jinja"]`.

**`testFindLlamaServerReturnsPath`**
- Call `findLlamaServer()` and assert it returns a non-nil path that exists on disk.

**`testModelFilePathResolvesCorrectly`**
- Call `modelFilePath()` and assert it ends with the expected filename.

### CompanionWindowControllerTests

**`testSetupStateAllIncomplete`**
- Create a CompanionWindowController with no model, no server, no MCP.
- Assert the setup steps are shown (not the dashboard).

**`testSetupStateAllComplete`**
- Mock the state to have model downloaded, server running, MCP installed.
- Assert the dashboard is shown (not setup steps).

**`testServerHealthCheckFailsGracefully`**
- With no server running, call `isServerRunning`.
- Assert it returns false without crashing.

### ModelDownloadManagerTests

**`testIsModelPresentWhenFileExists`**
- Create a temp file at the expected path with the right size.
- Assert `isModelPresent` returns true.

**`testIsModelPresentWhenFileMissing`**
- Assert `isModelPresent` returns false when file doesn't exist.

**`testManifestLoadsSuccessfully`**
- Call `loadManifest()` and assert it returns a valid manifest with at least one model.

## Running

```bash
# Fast unit tests (no server needed)
cargo test

# Integration tests (needs model + llama-server on the machine)
cargo test --features integration

# Swift tests
xcodebuild test -scheme glass-slipper -destination 'platform=macOS'
```

## Cargo.toml changes

```toml
[features]
integration = []
```

No new dependencies needed. Integration tests use `reqwest` (already a dep) for health checks and `std::process` for spawning.
