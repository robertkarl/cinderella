# Model Lineup Expansion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand the model manifest from 3 to 5 models, add KV cache quantization flags, and update auto-selection to start at 9B and only walk down.

**Architecture:** The manifest (`model-manifest.json`) is the single source of truth. Rust structs (`ModelDef`, `ServerConfig`) get new fields with serde defaults for backward compatibility. Auto-selection logic changes from "largest model that fits total RAM" to "start at 9B default, walk down if it won't fit." KV cache flags (`--cache-type-k`, `--cache-type-v`) get wired through `ServerConfig::to_args()`.

**Tech Stack:** Rust (serde, anyhow), JSON manifest, llama-server CLI flags

**Spec:** `docs/superpowers/specs/2026-05-14-model-lineup-expansion-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `model-manifest.json` | Modify | Add 0.8B and 27B models, update 4B quant, update 35B filename, add `cache_type_k`/`cache_type_v`/`auto_select` fields |
| `src/model_manifest.rs` | Modify | Add new fields to `ModelDef`, update `model_for_ram()` → `select_initial_model()`, expand `ModelTier` |
| `src/config.rs` | Modify | Add cache type fields to `ServerConfig`, emit in `to_args()` |
| `src/orchestrator.rs` | Modify | Call `select_initial_model()` instead of `model_for_ram()` |

---

### Task 1: Add KV cache type fields to ModelDef and ServerConfig

**Files:**
- Modify: `src/model_manifest.rs:34-53` (ModelDef struct)
- Modify: `src/config.rs:12-18` (ServerConfig struct)
- Modify: `src/config.rs:21-29` (from_model_def)
- Modify: `src/config.rs:31-48` (to_args)

- [ ] **Step 1: Write failing test for cache type fields in ModelDef**

In `src/model_manifest.rs`, add to the `tests` module:

```rust
#[test]
fn test_cache_type_defaults() {
    // Manifest without cache_type fields should get q8_0 defaults
    let manifest = Manifest::from_str(TEST_MANIFEST).unwrap();
    let model = manifest.default_model().unwrap();
    assert_eq!(model.cache_type_k, "q8_0");
    assert_eq!(model.cache_type_v, "q8_0");
}

#[test]
fn test_cache_type_explicit() {
    let json = r#"{
        "version": 1,
        "models": [{
            "id": "test",
            "name": "Test",
            "filename": "test.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 100,
            "sha256": "abc",
            "url": "https://example.com/test.gguf",
            "min_ram_gb": 8,
            "ctx_size": 4096,
            "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models",
            "cache_type_k": "q4_0",
            "cache_type_v": "q5_0"
        }],
        "default_model": "test"
    }"#;
    let manifest = Manifest::from_str(json).unwrap();
    let model = manifest.default_model().unwrap();
    assert_eq!(model.cache_type_k, "q4_0");
    assert_eq!(model.cache_type_v, "q5_0");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_cache_type_defaults test_cache_type_explicit -- --nocapture`
Expected: Compile error — `cache_type_k` and `cache_type_v` don't exist on `ModelDef`.

- [ ] **Step 3: Add cache type fields to ModelDef**

In `src/model_manifest.rs`, add default functions after the existing `default_arch()`:

```rust
fn default_cache_type() -> String {
    "q8_0".to_string()
}
```

Add fields to `ModelDef` struct after `n_gpu_layers`:

```rust
    #[serde(default = "default_cache_type")]
    pub cache_type_k: String,
    #[serde(default = "default_cache_type")]
    pub cache_type_v: String,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test test_cache_type -- --nocapture`
Expected: Both `test_cache_type_defaults` and `test_cache_type_explicit` PASS.

- [ ] **Step 5: Write failing test for cache type in ServerConfig args**

In `src/config.rs`, add to the `tests` module:

```rust
#[test]
fn test_server_config_args_include_cache_types() {
    let cfg = ServerConfig {
        model_path: std::path::PathBuf::from("/tmp/model.gguf"),
        port: 8787,
        ctx_size: 32768,
        n_gpu_layers: -1,
        jinja: true,
        cache_type_k: "q8_0".to_string(),
        cache_type_v: "q8_0".to_string(),
    };
    let args = cfg.to_args();
    assert!(args.contains(&"--cache-type-k".to_string()));
    assert!(args.contains(&"q8_0".to_string()));
    assert!(args.contains(&"--cache-type-v".to_string()));
}
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cargo test test_server_config_args_include_cache_types -- --nocapture`
Expected: Compile error — `cache_type_k` and `cache_type_v` don't exist on `ServerConfig`.

- [ ] **Step 7: Add cache type fields to ServerConfig and wire through**

In `src/config.rs`, add to `ServerConfig` struct:

```rust
    pub cache_type_k: String,
    pub cache_type_v: String,
```

Update `from_model_def`:

```rust
    pub fn from_model_def(model_path: std::path::PathBuf, port: u16, model: &crate::model_manifest::ModelDef) -> Self {
        Self {
            model_path,
            port,
            ctx_size: model.ctx_size,
            n_gpu_layers: model.n_gpu_layers,
            jinja: true,
            cache_type_k: model.cache_type_k.clone(),
            cache_type_v: model.cache_type_v.clone(),
        }
    }
```

Update `to_args` — add after the `--n-gpu-layers` line and before the `if self.jinja` block:

```rust
        args.push("--cache-type-k".to_string());
        args.push(self.cache_type_k.clone());
        args.push("--cache-type-v".to_string());
        args.push(self.cache_type_v.clone());
```

- [ ] **Step 8: Fix existing tests and ServerConfig construction sites**

The existing tests in `config.rs` and `server.rs` construct `ServerConfig` without the new fields. Fix them:

In `src/config.rs` tests, update `TEST_MANIFEST_JSON` to NOT include cache_type fields (tests backward compat via serde defaults — the `from_model_def` path handles it).

In `src/server.rs` tests, update the `make_config` helper:

```rust
    fn make_config(model_name: &str, port: u16, ctx_size: u32) -> ServerConfig {
        ServerConfig {
            model_path: PathBuf::from(format!("/tmp/{}", model_name)),
            port,
            ctx_size,
            n_gpu_layers: -1,
            jinja: true,
            cache_type_k: "q8_0".to_string(),
            cache_type_v: "q8_0".to_string(),
        }
    }
```

- [ ] **Step 9: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 10: Commit**

```bash
git add src/model_manifest.rs src/config.rs src/server.rs
git commit -m "feat: add KV cache type fields to ModelDef and ServerConfig

Wire --cache-type-k and --cache-type-v through to llama-server args.
Defaults to q8_0 (was implicitly f16). Halves KV cache memory usage
with no visible quality loss."
```

---

### Task 2: Add auto_select field to ModelDef

**Files:**
- Modify: `src/model_manifest.rs:34-53` (ModelDef struct)

- [ ] **Step 1: Write failing test for auto_select**

In `src/model_manifest.rs` tests:

```rust
#[test]
fn test_auto_select_defaults_to_true() {
    let manifest = Manifest::from_str(TEST_MANIFEST).unwrap();
    let model = manifest.default_model().unwrap();
    assert!(model.auto_select);
}

#[test]
fn test_auto_select_explicit_false() {
    let json = r#"{
        "version": 1,
        "models": [{
            "id": "test",
            "name": "Test",
            "filename": "test.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 100,
            "sha256": "abc",
            "url": "https://example.com/test.gguf",
            "min_ram_gb": 8,
            "ctx_size": 4096,
            "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models",
            "auto_select": false
        }],
        "default_model": "test"
    }"#;
    let manifest = Manifest::from_str(json).unwrap();
    let model = manifest.default_model().unwrap();
    assert!(!model.auto_select);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_auto_select -- --nocapture`
Expected: Compile error — `auto_select` doesn't exist on `ModelDef`.

- [ ] **Step 3: Add auto_select field to ModelDef**

Add default function:

```rust
fn default_auto_select() -> bool {
    true
}
```

Add field to `ModelDef` after the cache_type fields:

```rust
    #[serde(default = "default_auto_select")]
    pub auto_select: bool,
```

- [ ] **Step 4: Run tests**

Run: `cargo test test_auto_select -- --nocapture`
Expected: Both pass.

- [ ] **Step 5: Commit**

```bash
git add src/model_manifest.rs
git commit -m "feat: add auto_select field to ModelDef

Controls whether a model participates in automatic selection.
Defaults to true for backward compatibility. Models like 27B and
35B MoE will be opt-in only (auto_select: false)."
```

---

### Task 3: Replace model_for_ram with select_initial_model

The new logic: filter to `auto_select == true` models, find the one with `tier == Default` (9B), check if it fits, walk down if not. Never walk up.

**Files:**
- Modify: `src/model_manifest.rs:87-96` (model_for_ram)
- Modify: `src/orchestrator.rs:51` (call site)

- [ ] **Step 1: Write failing tests for select_initial_model**

In `src/model_manifest.rs` tests, add a new manifest constant that includes the full 5-model lineup. Place it after the existing `TIERED_MANIFEST`:

```rust
const FIVE_MODEL_MANIFEST: &str = r#"{
    "version": 1,
    "models": [
        {
            "id": "qwen3.5-0.8b-q5",
            "name": "Qwen 3.5 0.8B",
            "filename": "Qwen3.5-0.8B-Q5_K_M.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 590057728,
            "sha256": "TODO-0.8b",
            "url": "https://example.com/0.8b.gguf",
            "min_ram_gb": 4,
            "ctx_size": 8192,
            "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models",
            "tier": "small"
        },
        {
            "id": "qwen3.5-4b-q5",
            "name": "Qwen 3.5 4B",
            "filename": "Qwen3.5-4B-Q5_K_M.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 3143656608,
            "sha256": "TODO-4b",
            "url": "https://example.com/4b.gguf",
            "min_ram_gb": 8,
            "ctx_size": 16384,
            "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models",
            "tier": "small"
        },
        {
            "id": "qwen3.5-9b-q5",
            "name": "Qwen 3.5 9B",
            "filename": "Qwen3.5-9B-Q5_K_M.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 6577841376,
            "sha256": "dc2a39aef291f91a9116ad214058da0d86eb648743a124bd8c333787c4b9c91c",
            "url": "https://example.com/9b.gguf",
            "min_ram_gb": 16,
            "ctx_size": 32768,
            "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models",
            "tier": "default"
        },
        {
            "id": "qwen3.5-27b-q5",
            "name": "Qwen 3.5 27B",
            "filename": "Qwen3.5-27B-Q5_K_M.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 19608995744,
            "sha256": "TODO-27b",
            "url": "https://example.com/27b.gguf",
            "min_ram_gb": 32,
            "ctx_size": 32768,
            "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models",
            "tier": "large",
            "auto_select": false
        },
        {
            "id": "qwen3.5-35b-moe-q5",
            "name": "Qwen 3.5 35B MoE",
            "filename": "Qwen3.5-35B-MoE-Q5_K_M.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 20000000000,
            "sha256": "TODO-35b",
            "url": "https://example.com/35b.gguf",
            "min_ram_gb": 64,
            "ctx_size": 32768,
            "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models",
            "tier": "large",
            "auto_select": false
        }
    ],
    "default_model": "qwen3.5-9b-q5"
}"#;

#[test]
fn test_select_initial_model_picks_9b_when_ram_sufficient() {
    let manifest = Manifest::from_str(FIVE_MODEL_MANIFEST).unwrap();
    // 96 GB — plenty of RAM, but should still pick 9B (never auto-selects up)
    let model = manifest.select_initial_model(96).unwrap();
    assert_eq!(model.id, "qwen3.5-9b-q5");
}

#[test]
fn test_select_initial_model_falls_to_4b_when_9b_too_big() {
    let manifest = Manifest::from_str(FIVE_MODEL_MANIFEST).unwrap();
    // 12 GB — 9B needs 16 GB min, should fall to 4B
    let model = manifest.select_initial_model(12).unwrap();
    assert_eq!(model.id, "qwen3.5-4b-q5");
}

#[test]
fn test_select_initial_model_falls_to_0_8b() {
    let manifest = Manifest::from_str(FIVE_MODEL_MANIFEST).unwrap();
    // 6 GB — 4B needs 8 GB, should fall to 0.8B
    let model = manifest.select_initial_model(6).unwrap();
    assert_eq!(model.id, "qwen3.5-0.8b-q5");
}

#[test]
fn test_select_initial_model_none_when_nothing_fits() {
    let manifest = Manifest::from_str(FIVE_MODEL_MANIFEST).unwrap();
    // 2 GB — nothing fits
    let result = manifest.select_initial_model(2);
    assert!(result.is_none());
}

#[test]
fn test_select_initial_model_never_picks_opt_in() {
    let manifest = Manifest::from_str(FIVE_MODEL_MANIFEST).unwrap();
    // 128 GB — even with tons of RAM, should pick 9B not 27B/35B
    let model = manifest.select_initial_model(128).unwrap();
    assert_eq!(model.id, "qwen3.5-9b-q5");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_select_initial_model -- --nocapture`
Expected: Compile error — `select_initial_model` doesn't exist.

- [ ] **Step 3: Implement select_initial_model**

In `src/model_manifest.rs`, add this method to `impl Manifest` (keep `model_for_ram` for now — we'll remove it after updating callers):

```rust
    /// Select the initial model for startup.
    /// Starts at the Default tier (9B), walks down if RAM is insufficient.
    /// Never auto-selects upward. Models with auto_select=false are skipped.
    pub fn select_initial_model(&self, available_ram_gb: u32) -> Option<&ModelDef> {
        // Collect auto-selectable models, sorted by min_ram_gb descending
        let mut candidates: Vec<&ModelDef> = self
            .models
            .iter()
            .filter(|m| m.auto_select)
            .collect();
        candidates.sort_by(|a, b| b.min_ram_gb.cmp(&a.min_ram_gb));

        // Find the Default tier model (9B) — this is our starting point
        let default_model = candidates.iter().find(|m| m.tier == ModelTier::Default);

        // If the default fits, use it (never go higher)
        if let Some(model) = default_model {
            if model.min_ram_gb <= available_ram_gb {
                return Some(model);
            }
        }

        // Walk down: pick the largest auto-selectable model that fits
        // but is smaller than Default tier
        candidates
            .into_iter()
            .filter(|m| m.tier < ModelTier::Default && m.min_ram_gb <= available_ram_gb)
            .next()
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test test_select_initial_model -- --nocapture`
Expected: All 5 tests pass.

- [ ] **Step 5: Update orchestrator call site**

In `src/orchestrator.rs`, change line 51-52 from:

```rust
    let active_model = manifest.model_for_ram(hw.total_ram_gb as u32)
        .context("No model fits this machine's RAM")?;
```

to:

```rust
    let active_model = manifest.select_initial_model(hw.total_ram_gb as u32)
        .context("No model fits this machine's available RAM")?;
```

Also update the second call at line 145:

```rust
    let best_model = manifest.model_for_ram(hw.total_ram_gb as u32);
```

to:

```rust
    let best_model = manifest.select_initial_model(hw.total_ram_gb as u32);
```

- [ ] **Step 6: Remove model_for_ram**

Delete the `model_for_ram` method from `impl Manifest` in `src/model_manifest.rs` (lines 87-96). Also remove or update the old `test_model_for_ram_picks_largest_fitting` test — it tested the old behavior. The `TIERED_MANIFEST` constant can stay since `one_tier_down` tests use it.

- [ ] **Step 7: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/model_manifest.rs src/orchestrator.rs
git commit -m "feat: replace model_for_ram with select_initial_model

Always starts at 9B (Default tier), walks down if RAM insufficient.
Never auto-selects upward. Opt-in models (auto_select: false) are
skipped by auto-selection."
```

---

### Task 4: Expand ModelTier for the new lineup

The current `ModelTier` has Small, Default, Large. We need the 0.8B and 4B to be distinguishable for `one_tier_down` fallback ordering. Two Small-tier models can't be ordered. Add a `Tiny` tier.

**Files:**
- Modify: `src/model_manifest.rs:13-19` (ModelTier enum)
- Modify: `src/model_manifest.rs:101-108` (one_tier_down)

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_one_tier_down_with_tiny() {
    let json = r#"{
        "version": 1,
        "models": [
            {
                "id": "tiny", "name": "Tiny", "filename": "t.gguf",
                "quant": "Q5", "size_bytes": 100, "sha256": "a",
                "url": "https://example.com/t.gguf", "min_ram_gb": 4,
                "ctx_size": 4096, "n_gpu_layers": -1,
                "app_support_subdir": "Glass Slipper/Models",
                "tier": "tiny"
            },
            {
                "id": "small", "name": "Small", "filename": "s.gguf",
                "quant": "Q5", "size_bytes": 200, "sha256": "b",
                "url": "https://example.com/s.gguf", "min_ram_gb": 8,
                "ctx_size": 8192, "n_gpu_layers": -1,
                "app_support_subdir": "Glass Slipper/Models",
                "tier": "small"
            },
            {
                "id": "default", "name": "Default", "filename": "d.gguf",
                "quant": "Q5", "size_bytes": 300, "sha256": "c",
                "url": "https://example.com/d.gguf", "min_ram_gb": 16,
                "ctx_size": 32768, "n_gpu_layers": -1,
                "app_support_subdir": "Glass Slipper/Models",
                "tier": "default"
            }
        ],
        "default_model": "default"
    }"#;
    let manifest = Manifest::from_str(json).unwrap();

    let small = manifest.models.iter().find(|m| m.id == "small").unwrap();
    let down = manifest.one_tier_down(small).unwrap();
    assert_eq!(down.id, "tiny");

    let tiny = manifest.models.iter().find(|m| m.id == "tiny").unwrap();
    assert!(manifest.one_tier_down(tiny).is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_one_tier_down_with_tiny -- --nocapture`
Expected: Deserialization error — `"tiny"` is not a valid `ModelTier` variant.

- [ ] **Step 3: Add Tiny variant to ModelTier**

Update the enum in `src/model_manifest.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Tiny,
    Small,
    Default,
    Large,
}
```

Update `one_tier_down` to handle the new tier:

```rust
    pub fn one_tier_down(&self, current: &ModelDef) -> Option<&ModelDef> {
        let lower_tier = match current.tier {
            ModelTier::Large => ModelTier::Default,
            ModelTier::Default => ModelTier::Small,
            ModelTier::Small => ModelTier::Tiny,
            ModelTier::Tiny => return None,
        };
        self.models.iter().find(|m| m.tier == lower_tier)
    }
```

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests pass including existing `one_tier_down` tests.

- [ ] **Step 5: Commit**

```bash
git add src/model_manifest.rs
git commit -m "feat: add Tiny tier to ModelTier enum

Supports the 0.8B model as a distinct fallback below Small (4B).
Enables one_tier_down to walk: Default → Small → Tiny."
```

---

### Task 5: Update model-manifest.json with the full lineup

**Files:**
- Modify: `model-manifest.json`

**Note:** The 35B MoE file on disk is `Qwen3.6-35B-A3B-UD-Q5_K_M.gguf` (25.6 GB), not the `Qwen3.5-35B-MoE-Q5_K_M.gguf` currently in the manifest. Update accordingly. The 4B on disk is Q5_K_M (not Q4_K_M as the current manifest says).

- [ ] **Step 1: Replace model-manifest.json**

```json
{
  "version": 1,
  "models": [
    {
      "id": "qwen3.5-0.8b-q5",
      "name": "Qwen 3.5 0.8B",
      "filename": "Qwen3.5-0.8B-Q5_K_M.gguf",
      "quant": "Q5_K_M",
      "size_bytes": 590057728,
      "sha256": "TODO-0.8b-sha256",
      "url": "https://huggingface.co/unsloth/Qwen3.5-0.8B-GGUF/resolve/main/Qwen3.5-0.8B-Q5_K_M.gguf",
      "min_ram_gb": 4,
      "min_macos": "15.0",
      "arch": "arm64",
      "ctx_size": 8192,
      "n_gpu_layers": -1,
      "app_support_subdir": "Glass Slipper/Models",
      "tier": "tiny",
      "cache_type_k": "q8_0",
      "cache_type_v": "q8_0",
      "auto_select": true
    },
    {
      "id": "qwen3.5-4b-q5",
      "name": "Qwen 3.5 4B",
      "filename": "Qwen3.5-4B-Q5_K_M.gguf",
      "quant": "Q5_K_M",
      "size_bytes": 3143656608,
      "sha256": "TODO-4b-sha256",
      "url": "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q5_K_M.gguf",
      "min_ram_gb": 8,
      "min_macos": "15.0",
      "arch": "arm64",
      "ctx_size": 16384,
      "n_gpu_layers": -1,
      "app_support_subdir": "Glass Slipper/Models",
      "tier": "small",
      "cache_type_k": "q8_0",
      "cache_type_v": "q8_0",
      "auto_select": true
    },
    {
      "id": "qwen3.5-9b-q5",
      "name": "Qwen 3.5 9B",
      "filename": "Qwen3.5-9B-Q5_K_M.gguf",
      "quant": "Q5_K_M",
      "size_bytes": 6577841376,
      "sha256": "dc2a39aef291f91a9116ad214058da0d86eb648743a124bd8c333787c4b9c91c",
      "url": "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q5_K_M.gguf",
      "min_ram_gb": 16,
      "min_macos": "15.0",
      "arch": "arm64",
      "ctx_size": 32768,
      "n_gpu_layers": -1,
      "app_support_subdir": "Glass Slipper/Models",
      "tier": "default",
      "cache_type_k": "q8_0",
      "cache_type_v": "q8_0",
      "auto_select": true
    },
    {
      "id": "qwen3.5-27b-q5",
      "name": "Qwen 3.5 27B",
      "filename": "Qwen3.5-27B-Q5_K_M.gguf",
      "quant": "Q5_K_M",
      "size_bytes": 19608995744,
      "sha256": "TODO-27b-sha256",
      "url": "https://huggingface.co/unsloth/Qwen3.5-27B-GGUF/resolve/main/Qwen3.5-27B-Q5_K_M.gguf",
      "min_ram_gb": 32,
      "min_macos": "15.0",
      "arch": "arm64",
      "ctx_size": 32768,
      "n_gpu_layers": -1,
      "app_support_subdir": "Glass Slipper/Models",
      "tier": "large",
      "cache_type_k": "q8_0",
      "cache_type_v": "q8_0",
      "auto_select": false
    },
    {
      "id": "qwen3.6-35b-a3b-q5",
      "name": "Qwen 3.6 35B MoE",
      "filename": "Qwen3.6-35B-A3B-UD-Q5_K_M.gguf",
      "quant": "Q5_K_M",
      "size_bytes": 26456194016,
      "sha256": "TODO-35b-sha256",
      "url": "https://huggingface.co/unsloth/Qwen3.6-35B-A3B-UD-GGUF/resolve/main/Qwen3.6-35B-A3B-UD-Q5_K_M.gguf",
      "min_ram_gb": 64,
      "min_macos": "15.0",
      "arch": "arm64",
      "ctx_size": 32768,
      "n_gpu_layers": -1,
      "app_support_subdir": "Glass Slipper/Models",
      "tier": "large",
      "cache_type_k": "q8_0",
      "cache_type_v": "q8_0",
      "auto_select": false
    }
  ],
  "default_model": "qwen3.5-9b-q5"
}
```

- [ ] **Step 2: Run full test suite**

Run: `cargo test`
Expected: All tests pass. The TIERED_MANIFEST tests still work because they use inline JSON, not the file.

- [ ] **Step 3: Verify the manifest loads correctly**

Run: `cargo run -- --help` (just to verify the binary compiles and the manifest parses at startup)
Expected: Compiles and prints help text.

- [ ] **Step 4: Commit**

```bash
git add model-manifest.json
git commit -m "feat: expand model manifest to 5 models with KV cache config

Add Qwen 3.5 0.8B (tiny fallback) and 27B (opt-in power mode).
Update 4B from Q4_K_M to Q5_K_M to match downloaded file.
Update 35B to Qwen 3.6-35B-A3B-UD (correct filename on disk).
All models now specify cache_type_k/v (q8_0) and auto_select."
```

---

### Task 6: Update existing test constants

The `TIERED_MANIFEST` in tests uses the old 3-model layout with the old 4B id (`qwen3.5-4b-q4`). Update test constants to reflect the new schema while preserving test coverage.

**Files:**
- Modify: `src/model_manifest.rs` (test module)

- [ ] **Step 1: Update TIERED_MANIFEST**

Replace the `TIERED_MANIFEST` constant in the test module. Keep the same 3-model structure (the tiered tests don't need all 5 models) but update the 4B id and add the new fields:

```rust
const TIERED_MANIFEST: &str = r#"{
    "version": 1,
    "models": [
        {
            "id": "qwen3.5-4b-q5",
            "name": "Qwen 3.5 4B",
            "filename": "Qwen3.5-4B-Q5_K_M.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 3143656608,
            "sha256": "TODO-4b-sha256",
            "url": "https://example.com/4b.gguf",
            "min_ram_gb": 8,
            "ctx_size": 16384,
            "n_gpu_layers": -1,
            "app_support_subdir": "GlassSlipper/Models",
            "tier": "small"
        },
        {
            "id": "qwen3.5-9b-q5",
            "name": "Qwen 3.5 9B",
            "filename": "Qwen3.5-9B-Q5_K_M.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 6577841376,
            "sha256": "dc2a39aef291f91a9116ad214058da0d86eb648743a124bd8c333787c4b9c91c",
            "url": "https://example.com/9b.gguf",
            "min_ram_gb": 16,
            "ctx_size": 32768,
            "n_gpu_layers": -1,
            "app_support_subdir": "GlassSlipper/Models",
            "tier": "default"
        },
        {
            "id": "qwen3.5-35b-moe-q5",
            "name": "Qwen 3.5 35B MoE",
            "filename": "Qwen3.5-35B-MoE-Q5_K_M.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 20000000000,
            "sha256": "TODO-35b-sha256",
            "url": "https://example.com/35b.gguf",
            "min_ram_gb": 64,
            "ctx_size": 32768,
            "n_gpu_layers": -1,
            "app_support_subdir": "GlassSlipper/Models",
            "tier": "large"
        }
    ],
    "default_model": "qwen3.5-9b-q5"
}"#;
```

- [ ] **Step 2: Update test assertions that reference old ids**

In `test_parse_manifest_with_tiers`, change:
```rust
let small = manifest.models.iter().find(|m| m.id == "qwen3.5-4b-q4").unwrap();
```
to:
```rust
let small = manifest.models.iter().find(|m| m.id == "qwen3.5-4b-q5").unwrap();
```

In `test_one_tier_down`, change:
```rust
assert_eq!(down.id, "qwen3.5-4b-q4");
```
to:
```rust
assert_eq!(down.id, "qwen3.5-4b-q5");
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/model_manifest.rs
git commit -m "test: update test constants for new model lineup

Align TIERED_MANIFEST with new 4B id (q5 not q4) and schema."
```

---

### Task 7: Smoke test — build and dry-run

**Files:** None (verification only)

- [ ] **Step 1: Full build**

Run: `cargo build`
Expected: Compiles without warnings.

- [ ] **Step 2: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 3: Dry-run with --help**

Run: `cargo run -- --help`
Expected: Prints help text with all CLI flags.

- [ ] **Step 4: Verify manifest loads**

Run: `cargo run -- /tmp --prompt "hello" 2>&1 | head -5`
Expected: Starts up, prints hardware info and "Model: Qwen 3.5 9B Q5_K_M (6.1 GiB)" (or similar), then connects to llama-server. It will fail if llama-server isn't running — that's fine. The point is the manifest loaded and 9B was selected.
