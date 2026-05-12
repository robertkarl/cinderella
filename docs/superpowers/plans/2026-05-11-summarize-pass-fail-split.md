# Split local_summarize / local_pass_fail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the current `local_summarize` MCP tool into a general-purpose summarizer (`local_summarize`) and a build/test pass-fail checker (`local_pass_fail`).

**Architecture:** Add a new `PASS_FAIL` prompt constant and `handle_pass_fail` handler that preserves the old pass/fail behavior. Rewrite the `SUMMARIZE` prompt to be general-purpose. Add `local_pass_fail` to tool definitions and dispatch. All other tools untouched.

**Tech Stack:** Rust, tokio, serde_json

---

### File Map

- Modify: `src/mcp/prompts.rs` — rewrite `SUMMARIZE`, add `PASS_FAIL`, update `system_prompt()` match, update tests
- Modify: `src/mcp/tools.rs` — update `local_summarize` description, add `local_pass_fail` definition, add dispatch arm, update tests
- Modify: `src/mcp/handlers.rs` — add `handle_pass_fail`

---

### Task 1: Add PASS_FAIL prompt and rewrite SUMMARIZE prompt

**Files:**
- Modify: `src/mcp/prompts.rs`

- [ ] **Step 1: Write the failing test**

Add a test for the new `PASS_FAIL` prompt in `src/mcp/prompts.rs`:

```rust
#[test]
fn test_pass_fail_prompt() {
    assert!(PASS_FAIL.starts_with("You are a local offload model"));
    assert!(PASS_FAIL.contains("Pass or fail"));
    assert_eq!(system_prompt("local_pass_fail"), PASS_FAIL);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib mcp::prompts::tests::test_pass_fail_prompt`
Expected: FAIL — `PASS_FAIL` not defined

- [ ] **Step 3: Add PASS_FAIL constant, rewrite SUMMARIZE, update system_prompt match**

In `src/mcp/prompts.rs`, add the `PASS_FAIL` constant (the current `SUMMARIZE` text, verbatim):

```rust
pub const PASS_FAIL: &str = "You are a local offload model running on the user's machine. \
A frontier AI assistant (Claude) has delegated this task to you to save tokens and cost. \
Be concise. No preamble, no disclaimers, no follow-up questions. \
If you are unsure, say so in one sentence — do not hallucinate.\n\n\
Your task: summarize the output of a shell command. Report:\n\
1. Pass or fail (one word)\n\
2. If failed: the specific error(s), with file and line number if present\n\
3. If passed: any warnings worth noting (skip if zero warnings)\n\
Keep it under 5 sentences. Do not reproduce the raw output.";
```

Replace the existing `SUMMARIZE` constant with:

```rust
pub const SUMMARIZE: &str = "You are a local offload model running on the user's machine. \
A frontier AI assistant (Claude) has delegated this task to you to save tokens and cost. \
Be concise. No preamble, no disclaimers, no follow-up questions. \
If you are unsure, say so in one sentence — do not hallucinate.\n\n\
Your task: summarize the output of a shell command.\n\
Describe what the output contains and any key information in 2-5 sentences.\n\
Note errors or failures if present, but do not assume a pass/fail framing.\n\
Do not reproduce the raw output.";
```

Add `"local_pass_fail"` to the `system_prompt` match:

```rust
"local_pass_fail" => PASS_FAIL,
```

- [ ] **Step 4: Update existing tests**

The existing test `test_prompts_contain_task_instructions` asserts `SUMMARIZE.contains("Pass or fail")`. Update it:

```rust
assert!(SUMMARIZE.contains("summarize the output"));
```

The existing test `test_system_prompt_dispatch` doesn't test `local_pass_fail` — that's covered by the new test.

- [ ] **Step 5: Run all prompt tests**

Run: `cargo test --lib mcp::prompts`
Expected: all tests pass (including the new `test_pass_fail_prompt`)

- [ ] **Step 6: Commit**

```bash
git add src/mcp/prompts.rs
git commit -m "refactor(mcp): split SUMMARIZE prompt into general-purpose + PASS_FAIL"
```

---

### Task 2: Add local_pass_fail tool definition and dispatch

**Files:**
- Modify: `src/mcp/tools.rs`

- [ ] **Step 1: Write the failing test**

Add a test in `src/mcp/tools.rs`:

```rust
#[test]
fn test_tool_definitions_has_pass_fail() {
    let defs = tool_definitions();
    let names: Vec<&str> = defs["tools"].as_array().unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"local_pass_fail"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib mcp::tools::tests::test_tool_definitions_has_pass_fail`
Expected: FAIL — no tool named `local_pass_fail`

- [ ] **Step 3: Add tool definition**

In `src/mcp/tools.rs`, in the `tool_definitions()` function, update the `local_summarize` description and add `local_pass_fail` after it:

Update `local_summarize` description (line 14) to:

```rust
"description": "Run a shell command and return a concise summary of its output. Use this to keep verbose command output out of context. Saves tokens by having a local model read the output and report the key points.",
```

Add new tool definition after the `local_summarize` block:

```rust
{
    "name": "local_pass_fail",
    "description": "Run a shell command and report pass or fail. Use this for builds, test suites, and linters where you only need to know if it succeeded and what went wrong if it didn't.",
    "inputSchema": {
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "The shell command to run"
            }
        },
        "required": ["command"]
    }
},
```

- [ ] **Step 4: Add dispatch arm**

In the `dispatch()` match block, add after the `"local_summarize"` arm:

```rust
"local_pass_fail" => {
    let command = arguments.get("command").and_then(|v| v.as_str()).unwrap_or("");
    handlers::handle_pass_fail(client, logger, command).await
}
```

- [ ] **Step 5: Update tool count test**

In `test_tool_definitions_has_seven_tools`, update the assertion:

```rust
assert_eq!(tools.len(), 8);
```

And rename the test to `test_tool_definitions_has_eight_tools`.

- [ ] **Step 6: Add `local_pass_fail` to the names test**

In `test_tool_definitions_names`, add:

```rust
assert!(names.contains(&"local_pass_fail"));
```

- [ ] **Step 7: Run all tools tests**

Run: `cargo test --lib mcp::tools`
Expected: FAIL — `handlers::handle_pass_fail` doesn't exist yet. That's expected; Task 3 adds it.

- [ ] **Step 8: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "feat(mcp): add local_pass_fail tool definition and dispatch"
```

---

### Task 3: Add handle_pass_fail handler

**Files:**
- Modify: `src/mcp/handlers.rs`

- [ ] **Step 1: Add the handler**

In `src/mcp/handlers.rs`, add after `handle_summarize`:

```rust
pub async fn handle_pass_fail(
    client: &McpLlmClient,
    logger: &ActivityLogger,
    command: &str,
) -> ToolResult {
    let start = Instant::now();
    let detail = slug_command(command);
    let output = match run_command(command).await {
        Ok(out) => out,
        Err(e) => return ToolResult::error(format!("Failed to run command: {}", e)),
    };
    if output.is_empty() {
        logger.log("local_pass_fail", &detail, 0, 0, start, client.model_name());
        return ToolResult::text("Command produced no output.");
    }
    complete_and_log(client, logger, "local_pass_fail", &detail, prompts::PASS_FAIL, &output, SHORT_MAX_TOKENS, start).await
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test --lib`
Expected: all tests pass (prompts, tools, handlers)

- [ ] **Step 3: Build release binary**

Run: `cargo build --release`
Expected: compiles successfully

- [ ] **Step 4: Commit**

```bash
git add src/mcp/handlers.rs
git commit -m "feat(mcp): add handle_pass_fail handler"
```

---

### Task 4: Integration smoke test

- [ ] **Step 1: Test local_pass_fail via MCP binary**

```bash
printf '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}\n{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"local_pass_fail","arguments":{"command":"echo hello"}}}\n' | ./target/release/glass-slipper-mcp 2>/dev/null
```

Expected: response contains "Pass" (since `echo hello` exits 0, no errors)

- [ ] **Step 2: Test local_summarize via MCP binary**

```bash
printf '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}\n{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"local_summarize","arguments":{"command":"echo Mother Day Fondue Plan: cheese course then chocolate course"}}}\n' | ./target/release/glass-slipper-mcp 2>/dev/null
```

Expected: response contains a general summary, NOT "Pass"

- [ ] **Step 3: Verify tools/list returns 8 tools**

```bash
printf '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}\n{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}\n' | ./target/release/glass-slipper-mcp 2>/dev/null | tail -1 | python3 -c "import sys,json; tools=json.load(sys.stdin)['result']['tools']; print(f'{len(tools)} tools'); [print(f'  - {t[\"name\"]}') for t in tools]"
```

Expected: 8 tools listed, including both `local_summarize` and `local_pass_fail`
