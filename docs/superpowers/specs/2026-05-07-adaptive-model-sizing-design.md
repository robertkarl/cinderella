# Adaptive Model Sizing

Glass Slipper dynamically switches between model sizes based on system memory pressure, so the user always gets the best model their machine can handle right now.

## Models

| Tier | Model | Quant | Size | Min RAM |
|------|-------|-------|------|---------|
| Small | Qwen 3.5 4B | Q4_K_M | ~2.74GB | Any Mac |
| Default | Qwen 3.5 9B | Q5_K_M | ~6GB | 16GB+ |
| Large | Qwen 3.5 35B MoE | Q5_K_M | ~20GB | 64GB+ |

## First-Run Model Download

- Detect system RAM and download the appropriate default model (9B for 16GB+, 4B for 8GB machines).
- Also download one tier down as emergency fallback. If default is 9B, also download 4B.
- 35B MoE only downloaded on explicit user request.
- Model metadata lives in `model-manifest.json` (existing, expanded to cover all three tiers).

## Monitoring

A new `MemoryMonitor` Rust module (`src/memory_monitor.rs`) runs as a Tokio background task.

### Inputs

- **`DISPATCH_SOURCE_TYPE_MEMORYPRESSURE`** — macOS dispatch source, receives WARN and CRITICAL events via FFI. The authoritative "OS says things are bad" signal. Note: macOS never fires a "back to normal" event on this source.
- **`vm_stat` page-out rate** — polled every ~5 seconds. Delta between consecutive polls gives the current page-out rate. This is the real-time thrash signal. High page-out rate means the system is actively struggling. Near-zero page-out rate (sustained) means conditions have improved.
- **`sysctl vm.swapusage`** — absolute swap used/total/free. High swap alone does NOT indicate current pressure (macOS doesn't eagerly reclaim swap). Only meaningful in combination with page-out rate. High swap + zero page-outs = fine. High swap + rising page-outs = trouble.
- **tok/s from llama-server** — parsed from llama-server's response payload timing data. Published to the monitor by the agent loop via a `tokio::sync::watch` channel after each response completes. The monitor does not sit in the response path.

### Output

A `SystemHealth` enum emitted as events on a channel:

- **`Normal`** — no action needed.
- **`Warning`** — sustained page-out rate above threshold for N seconds. Recommend downgrade.
- **`Critical`** — `MEMORYPRESSURE` CRITICAL event, or page-out rate dramatically escalates. Hard cut.
- **`PromotionAvailable`** — running a smaller model and page-out rate has been near zero for M minutes. Suggest upgrade.

Each event includes a raw metrics snapshot (page-out rate, swap used, last tok/s) so consumers can display or log the numbers.

### Thresholds

Hardcoded conservative defaults for v1. No config file, no adaptive learning. All metrics are logged so real usage data can inform future tuning.

## Health State Transitions

```
Normal ──(sustained page-outs > threshold for N sec)──> Warning
Warning ──(MEMORYPRESSURE CRITICAL or severe escalation)──> Critical
Warning ──(page-outs drop to near-zero, sustained)──> Normal
Critical ──(model swap completes)──> Normal (re-evaluate on next poll)
Normal (on smaller model) ──(near-zero page-outs for M min)──> PromotionAvailable
```

Warning → Normal recovery: if the user dismisses the warning and page-out rate subsequently drops to near-zero for a sustained period, the monitor transitions back to Normal silently. The pressure resolved on its own (e.g., user closed a heavy app).

## Model Swap Execution

Hard cut. No graceful handoff, no parallel loading.

1. `MemoryMonitor` emits `Critical` → agent loop receives it.
2. Current agent step returns an error (the step fails, not the whole session).
3. Agent loop kills the llama-server process.
4. Agent loop starts llama-server with the smaller model.
5. Wait for llama-server `/health` endpoint to report ready.
6. Resume agent loop from conversation history. The conversation context lives in the Rust orchestrator, not in llama-server. Swapping the model is swapping the engine; the chassis stays intact. The only loss is the in-flight generation that got killed.
7. `MemoryMonitor` resets to `Normal`.

Context checkpointing is near-constant in Glass Slipper — local inference, no API billing — so losing a single in-flight step is low-cost.

## UX

Three tiers of visibility, matching three severity levels.

### Normal

Status bar shows: `● Qwen 9B · 23 tok/s` (green dot, model name, last response tok/s).

### Warning

- Status bar dot turns yellow.
- Inline banner appears in the chat area:
  - Shows: page-out rate, tok/s degradation, swap used.
  - Action button: "Switch to 4B".
  - Dismiss button.
- If user dismisses, suppress re-warning for N minutes unless conditions escalate to Critical.

### Critical

- Hard cut happens immediately, no user prompt.
- Status bar dot goes red briefly during swap, then green with new model.
- Post-facto banner: "Switched to Qwen 4B — system was thrashing (page-outs: X/s). Current step was cancelled."
- No undo button. The system was in trouble; don't invite going back immediately.

### Promotion

- After running on a smaller model with sustained low page-out rate, a subtle banner: "System pressure has eased. Switch back to 9B?"
- User confirms or dismisses.
- If dismissed, suppress re-suggestion for a longer interval.

### Swift Integration

New event types on the existing JSON protocol between Rust core and the Glass Slipper Swift app: `memory_warning`, `model_swap`, `promotion_available`.

## Logging

Two log files, same directory, same structured timestamped format.

### `glass-slipper-engine.log` (Rust owns)

Aggregates:
- Agent loop events (steps, errors, context state).
- Memory monitor metrics: every poll cycle logs page-out rate, swap used, current health state.
- tok/s from each completed llama-server response.
- Health state transitions (Normal→Warning, Warning→Critical, etc.).
- Model swap events: which model, why, how long the swap took.
- llama-server stderr, piped through Rust with a `llama` source tag.

### `glass-slipper-app.log` (Swift owns)

Logs:
- UI events and user interactions (dismissed warning, accepted promotion, etc.).
- Workflow orchestration decisions (agent dispatch, multi-file runs, control flow state).
- Any Swift-side errors or diagnostics.

Both files live in the same directory. When the full picture is needed, merge-sort by timestamp. No cross-process write contention.

## Not In Scope

- Remote inference / offloading to homelab.
- Configurable thresholds (config file or UI).
- Adaptive/learned thresholds.
- Graceful mid-generation handoff or parallel model loading.
- Auto-promotion without user confirmation.
