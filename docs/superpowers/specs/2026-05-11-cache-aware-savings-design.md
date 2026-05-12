# Cache-Aware Savings Accounting

## Problem

Glass Slipper claims to show how much money users save by delegating tasks to a local model instead of Claude. The current accounting is wrong in two ways:

1. **Overcounts on cached turns.** Every call is priced at the full uncached Opus input rate ($15/M). In reality, Anthropic's prompt cache means most turns within a 5-minute window hit cache at $1.875/M — 8x cheaper. We're inflating savings by ~8x for active sessions.

2. **Undercounts the real savings.** We only price the tokens the local model processed (the subtask), not the full conversation turn Claude would have paid for. The real savings is the entire turn cost Claude avoided.

3. **Ignores output tokens.** Opus output is $75/M. Currently not included in the cost estimate.

## Design

### Approach: Cache-aware turn-level costing

Each tool call estimates what the equivalent Claude turn would have cost, using:
- `context_tokens`: the full conversation context size, passed by Claude Code as a required parameter in every tool call
- Timestamp tracking: if <5 minutes since the last tool call, input tokens are priced at the cached rate; otherwise at the full uncached rate
- Output token pricing: always at $75/M (output is never cached)

### Pricing Constants

| Tier | Rate |
|------|------|
| Opus uncached input | $15.00 / M tokens |
| Opus cached input | $1.875 / M tokens |
| Opus output | $75.00 / M tokens |
| Cache TTL | 300 seconds |

### Tool Schema Changes (`tools.rs`)

Add `context_tokens` as a **required** integer parameter to every tool except `local_status`:

```json
"context_tokens": {
    "type": "integer",
    "description": "Current conversation context size in tokens. Used for savings estimation."
}
```

No backward compatibility. If Claude Code doesn't pass it, the tool call fails.

### Logger Changes (`logger.rs`)

- Add `last_log_time: Mutex<Option<Instant>>` to `ActivityLogger`
- On each `log()` call:
  - If `last_log_time` is `Some` and elapsed < 300s → `cache_hit = true`, use $1.875/M for input
  - Otherwise → `cache_hit = false`, use $15/M for input
  - Output always priced at $75/M
  - `estimated_cloud_cost_usd = context_tokens * input_rate + output_tokens * $75/M`
  - Update `last_log_time` to `Instant::now()`
- `ActivityEntry` gains: `cache_hit: bool`, `context_tokens: u64`
- Remove `OPUS_INPUT_PRICE_PER_TOKEN`, add `OPUS_UNCACHED_PER_TOKEN`, `OPUS_CACHED_PER_TOKEN`, `OPUS_OUTPUT_PER_TOKEN`

### Handler/Dispatch Changes

- `dispatch()` extracts `context_tokens` from arguments as required `u64`
- All handlers receive `context_tokens` and pass it through to `logger.log()`
- `complete_and_log()` gains a `context_tokens` parameter

### Swift UI Changes (`MCPActivityLog.swift`)

- Parse `cache_hit` (Bool) and `context_tokens` (Int) from JSONL entries
- Add both fields to `MCPActivityEntry`
- No display changes — `estimated_cloud_cost_usd` already flows to the dashboard

### JSONL Format

Before:
```json
{"ts":"1715400000","tool":"local_summarize","detail":"cargo build","input_tokens":3200,"output_tokens":18,"latency_ms":500,"estimated_cloud_cost_usd":0.048,"model":"qwen3.5"}
```

After:
```json
{"ts":"1715400000","tool":"local_summarize","detail":"cargo build","input_tokens":3200,"output_tokens":18,"context_tokens":45000,"latency_ms":500,"estimated_cloud_cost_usd":0.676,"cache_hit":false,"model":"qwen3.5"}
```

Cost breakdown: 45000 x $15/M + 18 x $75/M = $0.675 + $0.00135 = $0.676

### Tests

**`logger.rs` tests:**
- First call is always uncached (`cache_hit: false`)
- Two calls <5 min apart → second has `cache_hit: true`, priced at $1.875/M
- Two calls >5 min apart → second has `cache_hit: false`, priced at $15/M
- Output tokens priced at $75/M in all cases
- Cost math spot-checks: known `context_tokens` + `output_tokens` → expected USD
- `context_tokens` and `cache_hit` written to JSONL and round-trip through serde

**`tools.rs` tests:**
- `context_tokens` appears in every tool's `required` array (except `local_status`)
- `context_tokens` appears in every tool's `properties` with type `integer`

**`handlers.rs` tests:**
- `context_tokens` extracted from arguments and passed through to logger
