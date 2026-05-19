# Model Lineup Expansion

**Date:** 2026-05-14
**Status:** Draft

## Problem

The manifest has 3 models (4B, 9B, 35B MoE) with a rigid Small/Default/Large tier system. New Qwen 3.5 variants are available (0.8B, 27B) and need to be incorporated. The KV cache runs at f16 by default, wasting significant memory. The auto-selection logic uses total RAM, but available RAM matters more — a 96 GB machine running Photoshop and Final Cut acts like a 32 GB system.

## Design

### Model Lineup

Five models in the manifest:

| Model | Quant | File Size | Auto-select | Role |
|-------|-------|-----------|-------------|------|
| Qwen 3.5 0.8B | Q5_K_M | 563 MB | Yes | Emergency fallback |
| Qwen 3.5 4B | Q5_K_M | 2.9 GB | Yes | Low-memory fallback |
| Qwen 3.5 9B | Q5_K_M | 6.1 GB | Yes | Default (always tried first) |
| Qwen 3.5 27B | Q5_K_M | 18 GB | No | User opt-in power mode |
| Qwen 3.5 35B MoE | Q5_K_M | 20 GB | No | User opt-in (better parallelism, faster inference on some hardware) |

**Dropped:** Qwen 3.5 122B-A10B. Too large for local use on 96 GB, and split GGUF adds complexity we don't need.

### Auto-Selection Logic

The current `model_for_ram()` picks the largest model that fits total RAM. Replace with:

1. **Always start with 9B** as the assumption.
2. Calculate 9B's memory footprint: model file size + KV cache at full context.
3. Check if loading 9B would cause memory pressure given current available memory.
4. If 9B won't fit, try 4B. If 4B won't fit, fall back to 0.8B.
5. **Never auto-select upward.** 27B and 35B MoE are opt-in only.

The existing memory pressure monitor (`memory_ffi.rs`) handles runtime downgrades. This change only affects initial model selection.

### KV Cache Quantization

**Current state:** `ServerConfig::to_args()` does not emit `--cache-type-k` or `--cache-type-v`, so llama-server defaults to f16 for both. This wastes significant memory — f16 KV cache for 9B at 32K context is ~2 GB; q8_0 is ~1 GB.

**Change:** Add `cache_type_k` and `cache_type_v` fields to each model entry in the manifest. Wire them through `ServerConfig` into `to_args()`.

**Starting value:** `q8_0` for both K and V across all models. This is a safe default with no visible quality loss. Exact values per model will be tuned after we build eval infrastructure.

### Context Windows

Starting values per model. These are tunable after eval:

| Model | ctx_size |
|-------|----------|
| 0.8B | 8192 |
| 4B | 16384 |
| 9B | 32768 |
| 27B | 32768 |
| 35B MoE | 32768 |

### Manifest Schema Changes

New fields on each model entry:

```json
{
  "cache_type_k": "q8_0",
  "cache_type_v": "q8_0",
  "auto_select": true
}
```

Both fields have defaults (`q8_0` and `true`) for backward compatibility with existing manifest entries during development.

### Rust Changes

**`model_manifest.rs`:**
- Add `cache_type_k: String`, `cache_type_v: String`, `auto_select: bool` to `ModelDef`
- Provide serde defaults: `"q8_0"` and `true`
- `model_for_ram()` → rename/refactor to `select_initial_model()`. Filter on `auto_select == true`. Start at 9B (the model with `tier == Default`), walk down only.
- Keep `ModelTier` enum but add tiers as needed for ordering. Alternatively, add a numeric `priority` field to avoid enum explosion. (Implementation detail — either works.)

**`config.rs`:**
- Add `cache_type_k: String` and `cache_type_v: String` to `ServerConfig`
- `from_model_def()` reads them from `ModelDef`
- `to_args()` emits `--cache-type-k <value>` and `--cache-type-v <value>`

**`orchestrator.rs`:**
- Update initial model selection to use the new logic
- No changes to the runtime memory pressure / downgrade path (it already works with tiers)

### What's NOT in scope

- Split GGUF support (dropped with 122B)
- Adjust-up / promotion logic changes (future feature)
- Eval framework for comparing models and quants (separate spec)
- GUI changes beyond reading updated manifest (ModelDownloadManager already parses the JSON dynamically)
- Determining optimal ctx_size or KV quant per model (requires eval infrastructure first)
