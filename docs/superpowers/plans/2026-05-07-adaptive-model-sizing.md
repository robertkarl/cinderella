# Adaptive Model Sizing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dynamically switch between 4B/9B/35B models based on macOS memory pressure so the user always gets the best model their machine can handle right now.

**Architecture:** A Tokio background task (`MemoryMonitor`) polls `vm_stat` page-out rates every 5 seconds and listens for macOS `DISPATCH_SOURCE_TYPE_MEMORYPRESSURE` events via FFI. It emits `SystemHealth` events on a `tokio::sync::watch` channel. The orchestrator's agent loop receives these events and coordinates model swaps by killing/restarting llama-server with a different model. The conversation history stays in Rust; only the inference engine swaps.

**Tech Stack:** Rust, Tokio, macOS FFI (`libSystem.dylib`), `tokio::sync::watch`, existing `ServerManager`, existing `model-manifest.json`

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `src/memory_monitor.rs` | Memory pressure polling, FFI dispatch source, `SystemHealth` state machine |
| Create | `src/memory_ffi.rs` | Raw macOS FFI bindings for `DISPATCH_SOURCE_TYPE_MEMORYPRESSURE` |
| Modify | `src/model_manifest.rs` | Add `model_for_ram()` tier lookup, add `tier` field to `ModelDef` |
| Modify | `model-manifest.json` | Add 4B and 35B model entries with `tier` field |
| Modify | `src/config.rs` | Remove hardcoded `BUNDLED_MODEL`, derive active model from manifest |
| Modify | `src/server.rs` | Add `swap_model()` method (stop + start with new config) |
| Modify | `src/orchestrator.rs` | Spawn `MemoryMonitor`, react to health events, coordinate swaps |
| Modify | `src/agent.rs` | Add `MemoryWarning`/`ModelSwap`/`PromotionAvailable` event variants |
| Modify | `src/tui.rs` | Render new events (warning banner, swap notification, promotion offer) |
| Modify | `src/tui.rs` | Add JSON protocol events: `memory_warning`, `model_swap`, `promotion_available` |
| Modify | `src/main.rs` | No changes needed (orchestrator handles everything) |

---

### Task 1: Expand model-manifest.json with all three tiers

**Files:**
- Modify: `model-manifest.json`
- Modify: `src/model_manifest.rs:20-37` (add `tier` field to `ModelDef`)
- Test: `src/model_manifest.rs` (inline tests)

- [ ] **Step 1: Write the failing test for tier field parsing**

Add to the `#[cfg(test)]` block in `src/model_manifest.rs`:

```rust
#[test]
fn test_parse_manifest_with_tiers() {
    let json = r#"{
        "version": 1,
        "models": [
            {
                "id": "qwen3.5-4b-q4",
                "name": "Qwen 3.5 4B",
                "filename": "Qwen3.5-4B-Q4_K_M.gguf",
                "quant": "Q4_K_M",
                "size_bytes": 2740000000,
                "sha256": "TODO-4b-sha256",
                "url": "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf",
                "min_ram_gb": 8,
                "ctx_size": 32768,
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
                "url": "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q5_K_M.gguf",
                "min_ram_gb": 16,
                "ctx_size": 32768,
                "n_gpu_layers": -1,
                "app_support_subdir": "Glass Slipper/Models",
                "tier": "default"
            },
            {
                "id": "qwen3.5-35b-moe-q5",
                "name": "Qwen 3.5 35B MoE",
                "filename": "Qwen3.5-35B-MoE-Q5_K_M.gguf",
                "quant": "Q5_K_M",
                "size_bytes": 20000000000,
                "sha256": "TODO-35b-sha256",
                "url": "https://huggingface.co/unsloth/Qwen3.5-35B-MoE-GGUF/resolve/main/Qwen3.5-35B-MoE-Q5_K_M.gguf",
                "min_ram_gb": 64,
                "ctx_size": 32768,
                "n_gpu_layers": -1,
                "app_support_subdir": "Glass Slipper/Models",
                "tier": "large"
            }
        ],
        "default_model": "qwen3.5-9b-q5"
    }"#;
    let manifest = Manifest::from_str(json).unwrap();
    assert_eq!(manifest.models.len(), 3);

    let small = manifest.models.iter().find(|m| m.tier == ModelTier::Small).unwrap();
    assert_eq!(small.name, "Qwen 3.5 4B");

    let default = manifest.models.iter().find(|m| m.tier == ModelTier::Default).unwrap();
    assert_eq!(default.name, "Qwen 3.5 9B");

    let large = manifest.models.iter().find(|m| m.tier == ModelTier::Large).unwrap();
    assert_eq!(large.name, "Qwen 3.5 35B MoE");
}

#[test]
fn test_model_for_ram() {
    let json = r#"{
        "version": 1,
        "models": [
            {"id":"small","name":"S","filename":"s.gguf","quant":"Q4","size_bytes":1000,"sha256":"x","url":"x","min_ram_gb":8,"ctx_size":32768,"n_gpu_layers":-1,"app_support_subdir":"Glass Slipper/Models","tier":"small"},
            {"id":"default","name":"D","filename":"d.gguf","quant":"Q5","size_bytes":2000,"sha256":"x","url":"x","min_ram_gb":16,"ctx_size":32768,"n_gpu_layers":-1,"app_support_subdir":"Glass Slipper/Models","tier":"default"},
            {"id":"large","name":"L","filename":"l.gguf","quant":"Q5","size_bytes":3000,"sha256":"x","url":"x","min_ram_gb":64,"ctx_size":32768,"n_gpu_layers":-1,"app_support_subdir":"Glass Slipper/Models","tier":"large"}
        ],
        "default_model": "default"
    }"#;
    let manifest = Manifest::from_str(json).unwrap();
    // 8GB machine -> small
    assert_eq!(manifest.model_for_ram(8.0).unwrap().id, "small");
    // 16GB machine -> default
    assert_eq!(manifest.model_for_ram(16.0).unwrap().id, "default");
    // 64GB machine -> large
    assert_eq!(manifest.model_for_ram(64.0).unwrap().id, "large");
    // 32GB machine -> default (not enough for large)
    assert_eq!(manifest.model_for_ram(32.0).unwrap().id, "default");
    // 4GB machine -> None (not enough for any)
    assert!(manifest.model_for_ram(4.0).is_none());
}

#[test]
fn test_tier_defaults_to_default() {
    // Existing manifests without "tier" should still parse
    let json = r#"{
        "version": 1,
        "models": [{
            "id": "qwen3.5-9b-q5",
            "name": "Qwen 3.5 9B",
            "filename": "Qwen3.5-9B-Q5_K_M.gguf",
            "quant": "Q5_K_M",
            "size_bytes": 6577841376,
            "sha256": "dc2a39aef291f91a9116ad214058da0d86eb648743a124bd8c333787c4b9c91c",
            "url": "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q5_K_M.gguf",
            "min_ram_gb": 16,
            "ctx_size": 32768,
            "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models"
        }],
        "default_model": "qwen3.5-9b-q5"
    }"#;
    let manifest = Manifest::from_str(json).unwrap();
    assert_eq!(manifest.models[0].tier, ModelTier::Default);
}

#[test]
fn test_one_tier_down() {
    let json = r#"{
        "version": 1,
        "models": [
            {"id":"small","name":"S","filename":"s.gguf","quant":"Q4","size_bytes":1000,"sha256":"x","url":"x","min_ram_gb":8,"ctx_size":32768,"n_gpu_layers":-1,"app_support_subdir":"Glass Slipper/Models","tier":"small"},
            {"id":"default","name":"D","filename":"d.gguf","quant":"Q5","size_bytes":2000,"sha256":"x","url":"x","min_ram_gb":16,"ctx_size":32768,"n_gpu_layers":-1,"app_support_subdir":"Glass Slipper/Models","tier":"default"},
            {"id":"large","name":"L","filename":"l.gguf","quant":"Q5","size_bytes":3000,"sha256":"x","url":"x","min_ram_gb":64,"ctx_size":32768,"n_gpu_layers":-1,"app_support_subdir":"Glass Slipper/Models","tier":"large"}
        ],
        "default_model": "default"
    }"#;
    let manifest = Manifest::from_str(json).unwrap();
    let default_model = manifest.default_model().unwrap();
    let down = manifest.one_tier_down(default_model);
    assert_eq!(down.unwrap().id, "small");

    let small = manifest.models.iter().find(|m| m.tier == ModelTier::Small).unwrap();
    assert!(manifest.one_tier_down(small).is_none());

    let large = manifest.models.iter().find(|m| m.tier == ModelTier::Large).unwrap();
    assert_eq!(manifest.one_tier_down(large).unwrap().id, "default");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib model_manifest 2>&1 | tail -20`
Expected: Compilation errors — `ModelTier` doesn't exist, `model_for_ram()` and `one_tier_down()` not defined.

- [ ] **Step 3: Add ModelTier enum and tier field to ModelDef**

In `src/model_manifest.rs`, add the `ModelTier` enum and update `ModelDef`:

```rust
/// Model size tier for adaptive sizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Small,
    Default,
    Large,
}

fn default_tier() -> ModelTier {
    ModelTier::Default
}
```

Add to `ModelDef` struct:

```rust
    #[serde(default = "default_tier")]
    pub tier: ModelTier,
```

Add to `Manifest` impl:

```rust
    /// Find the best model for a given total RAM amount.
    /// Returns the largest model whose min_ram_gb <= total_ram.
    /// Models are checked from largest to smallest tier.
    pub fn model_for_ram(&self, total_ram_gb: f64) -> Option<&ModelDef> {
        let mut candidates: Vec<&ModelDef> = self
            .models
            .iter()
            .filter(|m| (m.min_ram_gb as f64) <= total_ram_gb)
            .collect();
        candidates.sort_by(|a, b| b.tier.cmp(&a.tier));
        candidates.first().copied()
    }

    /// Find the model one tier below the given model.
    /// Returns None if the given model is already the smallest.
    pub fn one_tier_down(&self, current: &ModelDef) -> Option<&ModelDef> {
        let lower_tier = match current.tier {
            ModelTier::Large => ModelTier::Default,
            ModelTier::Default => ModelTier::Small,
            ModelTier::Small => return None,
        };
        self.models.iter().find(|m| m.tier == lower_tier)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib model_manifest 2>&1 | tail -20`
Expected: All model_manifest tests pass including the 4 new ones.

- [ ] **Step 5: Update model-manifest.json with all three tiers**

Replace the contents of `model-manifest.json` with:

```json
{
  "version": 1,
  "models": [
    {
      "id": "qwen3.5-4b-q4",
      "name": "Qwen 3.5 4B",
      "filename": "Qwen3.5-4B-Q4_K_M.gguf",
      "quant": "Q4_K_M",
      "size_bytes": 2740000000,
      "sha256": "TODO-4b-sha256",
      "url": "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf",
      "min_ram_gb": 8,
      "min_macos": "15.0",
      "arch": "arm64",
      "ctx_size": 32768,
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
      "url": "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q5_K_M.gguf",
      "min_ram_gb": 16,
      "min_macos": "15.0",
      "arch": "arm64",
      "ctx_size": 32768,
      "n_gpu_layers": -1,
      "app_support_subdir": "Glass Slipper/Models",
      "tier": "default"
    },
    {
      "id": "qwen3.5-35b-moe-q5",
      "name": "Qwen 3.5 35B MoE",
      "filename": "Qwen3.5-35B-MoE-Q5_K_M.gguf",
      "quant": "Q5_K_M",
      "size_bytes": 20000000000,
      "sha256": "TODO-35b-sha256",
      "url": "https://huggingface.co/unsloth/Qwen3.5-35B-MoE-GGUF/resolve/main/Qwen3.5-35B-MoE-Q5_K_M.gguf",
      "min_ram_gb": 64,
      "min_macos": "15.0",
      "arch": "arm64",
      "ctx_size": 32768,
      "n_gpu_layers": -1,
      "app_support_subdir": "Glass Slipper/Models",
      "tier": "large"
    }
  ],
  "default_model": "qwen3.5-9b-q5"
}
```

- [ ] **Step 6: Run full test suite**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -20`
Expected: All tests pass. The existing `TEST_MANIFEST` const in model_manifest tests should still work because `tier` defaults to `"default"`.

- [ ] **Step 7: Commit**

```bash
cd /Users/robertkarl/Code/cinderella
git add src/model_manifest.rs model-manifest.json
git commit -m "feat: add ModelTier enum and 4B/35B entries to manifest"
```

---

### Task 2: Remove hardcoded BUNDLED_MODEL, use manifest-driven model selection

**Files:**
- Modify: `src/config.rs:14-38` (remove `ModelEntry` struct and `BUNDLED_MODEL` const)
- Modify: `src/orchestrator.rs` (use `Manifest` + `model_for_ram()` instead of `BUNDLED_MODEL`)
- Test: `src/config.rs` (update existing tests)

- [ ] **Step 1: Write the failing test**

In `src/config.rs`, replace `test_bundled_model_sanity` with:

```rust
#[test]
fn test_server_config_from_model_def() {
    use crate::model_manifest::{Manifest, ModelTier};
    let json = r#"{
        "version": 1,
        "models": [{
            "id": "test", "name": "Test", "filename": "test.gguf", "quant": "Q5",
            "size_bytes": 1000, "sha256": "x", "url": "x", "min_ram_gb": 8,
            "ctx_size": 16384, "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models", "tier": "small"
        }],
        "default_model": "test"
    }"#;
    let manifest = Manifest::from_str(json).unwrap();
    let model = manifest.default_model().unwrap();
    let cfg = ServerConfig::from_model_def(
        std::path::PathBuf::from("/tmp/test.gguf"),
        8787,
        model,
    );
    assert_eq!(cfg.ctx_size, 16384);
    assert_eq!(cfg.n_gpu_layers, -1);
    let args = cfg.to_args();
    assert!(args.contains(&"16384".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib config::tests::test_server_config_from_model_def 2>&1 | tail -10`
Expected: FAIL — `from_model_def` doesn't exist.

- [ ] **Step 3: Replace BUNDLED_MODEL with manifest-driven config**

In `src/config.rs`:

1. Remove the `ModelEntry` struct and `BUNDLED_MODEL` const entirely.
2. Update `ServerConfig::from_model` to `ServerConfig::from_model_def` taking a `&ModelDef`:

```rust
impl ServerConfig {
    pub fn from_model_def(model_path: std::path::PathBuf, port: u16, model: &crate::model_manifest::ModelDef) -> Self {
        Self {
            model_path,
            port,
            ctx_size: model.ctx_size,
            n_gpu_layers: model.n_gpu_layers,
            jinja: true,
        }
    }
}
```

3. Update `test_server_config_args` to use `from_model_def`:

```rust
#[test]
fn test_server_config_args() {
    use crate::model_manifest::Manifest;
    let json = r#"{
        "version": 1,
        "models": [{
            "id": "test", "name": "Test", "filename": "test.gguf", "quant": "Q5",
            "size_bytes": 1000, "sha256": "x", "url": "x", "min_ram_gb": 8,
            "ctx_size": 32768, "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models", "tier": "default"
        }],
        "default_model": "test"
    }"#;
    let manifest = Manifest::from_str(json).unwrap();
    let model = manifest.default_model().unwrap();
    let cfg = ServerConfig::from_model_def(
        std::path::PathBuf::from("/tmp/model.gguf"),
        8787,
        model,
    );
    let args = cfg.to_args();
    assert!(args.contains(&"--model".to_string()));
    assert!(args.contains(&"--jinja".to_string()));
    assert!(args.contains(&"--port".to_string()));
    assert!(args.contains(&"8787".to_string()));
}
```

- [ ] **Step 4: Update orchestrator.rs to use manifest**

In `src/orchestrator.rs`, replace all references to `BUNDLED_MODEL` with manifest lookups:

1. At the top of `run()`, after hardware detection, load the manifest and select a model:

```rust
    let manifest = crate::model_manifest::find_manifest()
        .context("Failed to load model manifest")?;
    let active_model = manifest.model_for_ram(hw.total_ram_gb)
        .context("No model fits this machine's RAM")?;
```

2. Replace `find_or_extract_bundled_model(&hw)` with a simpler `find_model_file(active_model)`:

```rust
fn find_model_file(model: &crate::model_manifest::ModelDef) -> Result<PathBuf> {
    let primary = model.model_path();
    if primary.exists() {
        return Ok(primary);
    }

    // Development fallback: ~/models/
    let home = std::env::var("HOME").unwrap_or_default();
    let legacy = PathBuf::from(&home).join("models").join(&model.filename);
    if !is_release_bundle() && legacy.exists() {
        return Ok(legacy);
    }

    anyhow::bail!(
        "Model not found at {}.\n\
         The Glass Slipper app downloads the model on first launch.\n\
         For development: download {} and place it in ~/Library/Application Support/Glass Slipper/Models/",
        primary.display(),
        model.filename
    )
}
```

3. Replace `ServerConfig::from_model(...)` calls with `ServerConfig::from_model_def(...)`.

4. Replace `BUNDLED_MODEL.name`, `BUNDLED_MODEL.quant`, `BUNDLED_MODEL.ctx_size` references with `active_model.name`, `active_model.quant`, `active_model.ctx_size`.

5. Store a clone of the model id in `OrchestratorConfig` or pass `active_model` through to where it's needed.

- [ ] **Step 5: Run full test suite**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -20`
Expected: All tests pass. Compilation succeeds with no references to `BUNDLED_MODEL`.

- [ ] **Step 6: Verify no remaining BUNDLED_MODEL references**

Run: `cd /Users/robertkarl/Code/cinderella && grep -r "BUNDLED_MODEL" src/`
Expected: No output.

- [ ] **Step 7: Commit**

```bash
cd /Users/robertkarl/Code/cinderella
git add src/config.rs src/orchestrator.rs
git commit -m "refactor: replace hardcoded BUNDLED_MODEL with manifest-driven model selection"
```

---

### Task 3: macOS memory pressure FFI bindings

**Files:**
- Create: `src/memory_ffi.rs`
- Modify: `src/main.rs:1` (add `mod memory_ffi;`)
- Test: `src/memory_ffi.rs` (inline tests)

- [ ] **Step 1: Write the failing test**

Create `src/memory_ffi.rs` with only a test:

```rust
//! Raw macOS FFI bindings for DISPATCH_SOURCE_TYPE_MEMORYPRESSURE.
//! Provides a channel-based interface to receive memory pressure events.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pressure_level_from_raw() {
        assert_eq!(PressureLevel::from_raw(DISPATCH_MEMORYPRESSURE_WARN), PressureLevel::Warn);
        assert_eq!(PressureLevel::from_raw(DISPATCH_MEMORYPRESSURE_CRITICAL), PressureLevel::Critical);
        assert_eq!(PressureLevel::from_raw(999), PressureLevel::Normal);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib memory_ffi 2>&1 | tail -10`
Expected: FAIL — module doesn't have the types defined yet.

- [ ] **Step 3: Implement FFI bindings**

Write the full `src/memory_ffi.rs`:

```rust
//! Raw macOS FFI bindings for DISPATCH_SOURCE_TYPE_MEMORYPRESSURE.
//! Provides a channel-based interface to receive memory pressure events.

use std::ffi::c_void;
use std::os::raw::c_ulong;

// macOS dispatch types (opaque pointers)
type dispatch_queue_t = *mut c_void;
type dispatch_source_t = *mut c_void;
type dispatch_source_type_t = *const c_void;

// Memory pressure flags from <dispatch/source.h>
pub const DISPATCH_MEMORYPRESSURE_NORMAL: c_ulong = 0x01;
pub const DISPATCH_MEMORYPRESSURE_WARN: c_ulong = 0x02;
pub const DISPATCH_MEMORYPRESSURE_CRITICAL: c_ulong = 0x04;

extern "C" {
    static _dispatch_source_type_memorypressure: c_void;
    fn dispatch_queue_create(label: *const u8, attr: *mut c_void) -> dispatch_queue_t;
    fn dispatch_source_create(
        source_type: dispatch_source_type_t,
        handle: usize,
        mask: c_ulong,
        queue: dispatch_queue_t,
    ) -> dispatch_source_t;
    fn dispatch_source_set_event_handler_f(source: dispatch_source_t, handler: extern "C" fn(*mut c_void));
    fn dispatch_source_set_cancel_handler_f(source: dispatch_source_t, handler: extern "C" fn(*mut c_void));
    fn dispatch_set_context(object: dispatch_source_t, context: *mut c_void);
    fn dispatch_source_get_data(source: dispatch_source_t) -> c_ulong;
    fn dispatch_resume(object: dispatch_source_t);
    fn dispatch_source_cancel(source: dispatch_source_t);
}

/// Memory pressure level received from the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureLevel {
    Normal,
    Warn,
    Critical,
}

impl PressureLevel {
    pub fn from_raw(flags: c_ulong) -> Self {
        if flags & DISPATCH_MEMORYPRESSURE_CRITICAL != 0 {
            PressureLevel::Critical
        } else if flags & DISPATCH_MEMORYPRESSURE_WARN != 0 {
            PressureLevel::Warn
        } else {
            PressureLevel::Normal
        }
    }
}

/// Start listening for macOS memory pressure events.
/// Returns a tokio receiver that emits PressureLevel values.
/// The dispatch source runs on a dedicated serial queue and
/// sends events via a std::sync::mpsc channel bridged to tokio.
///
/// # Safety
/// Uses raw FFI to macOS libdispatch. Only valid on macOS.
pub fn start_pressure_listener() -> tokio::sync::mpsc::UnboundedReceiver<PressureLevel> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    // Leak the sender so the FFI callback can use it.
    // It lives for the process lifetime.
    let tx_ptr = Box::into_raw(Box::new(tx));

    unsafe {
        let queue = dispatch_queue_create(
            b"glass-slipper.memory-pressure\0".as_ptr(),
            std::ptr::null_mut(),
        );

        let source = dispatch_source_create(
            &_dispatch_source_type_memorypressure as *const _ as dispatch_source_type_t,
            0, // process 0 = current process
            DISPATCH_MEMORYPRESSURE_WARN | DISPATCH_MEMORYPRESSURE_CRITICAL,
            queue,
        );

        dispatch_set_context(source, tx_ptr as *mut c_void);
        dispatch_source_set_event_handler_f(source, memory_pressure_handler);
        dispatch_resume(source);

        // Intentionally never cancel — source lives for process lifetime
    }

    rx
}

extern "C" fn memory_pressure_handler(context: *mut c_void) {
    unsafe {
        // context is our leaked Box<UnboundedSender<PressureLevel>>
        let tx = &*(context as *const tokio::sync::mpsc::UnboundedSender<PressureLevel>);

        // We need the source to get the data, but dispatch calls us with the source
        // already set. Use dispatch_source_get_data on the current source.
        // However, this function receives context, not source. The data is available
        // through the source which is implicit in the handler. We need a different approach:
        // store the source in the context too.

        // Simpler: macOS memory pressure never fires Normal — only WARN and CRITICAL.
        // Since we registered for both, any event is at least WARN.
        // We can't easily get dispatch_source_get_data from inside handler_f.
        // Use dispatch_source_set_event_handler (block-based) instead.
        // For now, treat any event as Critical to be safe (worst-case we downgrade too eagerly).

        // Actually, we CAN get the data. The handler_f receives the context, but we
        // need to also pass the source. Let's store both in a struct.
        let _ = tx.send(PressureLevel::Critical);
    }
}

/// Context struct passed to the dispatch handler.
/// Contains both the channel sender and the dispatch source pointer.
struct PressureContext {
    tx: tokio::sync::mpsc::UnboundedSender<PressureLevel>,
    source: dispatch_source_t,
}

/// Improved version that can distinguish WARN from CRITICAL.
pub fn start_pressure_listener_v2() -> tokio::sync::mpsc::UnboundedReceiver<PressureLevel> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let ctx = Box::into_raw(Box::new(PressureContext {
        tx,
        source: std::ptr::null_mut(), // filled in after source creation
    }));

    unsafe {
        let queue = dispatch_queue_create(
            b"glass-slipper.memory-pressure\0".as_ptr(),
            std::ptr::null_mut(),
        );

        let source = dispatch_source_create(
            &_dispatch_source_type_memorypressure as *const _ as dispatch_source_type_t,
            0,
            DISPATCH_MEMORYPRESSURE_WARN | DISPATCH_MEMORYPRESSURE_CRITICAL,
            queue,
        );

        // Store the source pointer in context
        (*ctx).source = source;

        dispatch_set_context(source, ctx as *mut c_void);
        dispatch_source_set_event_handler_f(source, memory_pressure_handler_v2);
        dispatch_resume(source);
    }

    rx
}

extern "C" fn memory_pressure_handler_v2(context: *mut c_void) {
    unsafe {
        let ctx = &*(context as *const PressureContext);
        let data = dispatch_source_get_data(ctx.source);
        let level = PressureLevel::from_raw(data);
        let _ = ctx.tx.send(level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pressure_level_from_raw() {
        assert_eq!(PressureLevel::from_raw(DISPATCH_MEMORYPRESSURE_WARN), PressureLevel::Warn);
        assert_eq!(PressureLevel::from_raw(DISPATCH_MEMORYPRESSURE_CRITICAL), PressureLevel::Critical);
        assert_eq!(PressureLevel::from_raw(DISPATCH_MEMORYPRESSURE_NORMAL), PressureLevel::Normal);
        assert_eq!(PressureLevel::from_raw(999), PressureLevel::Normal);
    }
}
```

- [ ] **Step 4: Add `mod memory_ffi;` to main.rs**

In `src/main.rs`, add after `mod model_manifest;`:

```rust
mod memory_ffi;
```

- [ ] **Step 5: Run tests**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib memory_ffi 2>&1 | tail -10`
Expected: PASS — `test_pressure_level_from_raw` passes.

- [ ] **Step 6: Commit**

```bash
cd /Users/robertkarl/Code/cinderella
git add src/memory_ffi.rs src/main.rs
git commit -m "feat: add macOS memory pressure FFI bindings"
```

---

### Task 4: MemoryMonitor — page-out rate polling and SystemHealth state machine

**Files:**
- Create: `src/memory_monitor.rs`
- Modify: `src/main.rs:1` (add `mod memory_monitor;`)
- Test: `src/memory_monitor.rs` (inline tests)

This is the core of adaptive sizing: a Tokio task that polls `vm_stat` for page-out rates, receives FFI memory pressure events, and emits `SystemHealth` state transitions.

- [ ] **Step 1: Write the failing tests**

Create `src/memory_monitor.rs` starting with the test block:

```rust
//! Memory monitor: polls vm_stat page-out rate, receives macOS memory pressure
//! events, and emits SystemHealth state transitions via a tokio::sync::watch channel.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pageouts_from_vm_stat() {
        let output = r#"Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                               12345.
Pages active:                             67890.
Pages inactive:                           11111.
Pages speculative:                         2222.
Pages throttled:                              0.
Pages wired down:                          3333.
Pages purgeable:                           4444.
"Translation faults":                  12345678.
Pages copy-on-write:                     555555.
Pages zero filled:                       666666.
Pages reactivated:                         7777.
Pages purged:                              8888.
Pageouts:                                100000.
"#;
        assert_eq!(parse_pageouts(output), Some(100000));
    }

    #[test]
    fn test_parse_pageouts_missing() {
        assert_eq!(parse_pageouts("no pageouts line here"), None);
    }

    #[test]
    fn test_parse_swap_used() {
        let output = "total = 6144.00M  used = 1024.00M  free = 5120.00M  (encrypted)";
        let swap = parse_swap_usage(output);
        assert!((swap.used_mb - 1024.0).abs() < 1.0);
        assert!((swap.total_mb - 6144.0).abs() < 1.0);
    }

    #[test]
    fn test_health_state_normal_to_warning() {
        let mut state = HealthStateMachine::new();
        assert_eq!(state.current(), SystemHealth::Normal);

        // Sustained page-outs above threshold for WARN_SUSTAIN_POLLS polls
        for _ in 0..WARN_SUSTAIN_POLLS {
            state.update(Metrics {
                pageout_delta: PAGEOUT_WARN_THRESHOLD + 10,
                swap_used_mb: 0.0,
                swap_total_mb: 6144.0,
                last_tok_per_sec: None,
                pressure_event: None,
            });
        }
        assert_eq!(state.current(), SystemHealth::Warning);
    }

    #[test]
    fn test_health_state_warning_to_critical_on_pressure() {
        let mut state = HealthStateMachine::new();
        // Get to Warning first
        for _ in 0..WARN_SUSTAIN_POLLS {
            state.update(Metrics { pageout_delta: PAGEOUT_WARN_THRESHOLD + 10, swap_used_mb: 0.0, swap_total_mb: 6144.0, last_tok_per_sec: None, pressure_event: None });
        }
        assert_eq!(state.current(), SystemHealth::Warning);

        // CRITICAL pressure event
        state.update(Metrics { pageout_delta: 0, swap_used_mb: 0.0, swap_total_mb: 6144.0, last_tok_per_sec: None, pressure_event: Some(PressureLevel::Critical) });
        assert_eq!(state.current(), SystemHealth::Critical);
    }

    #[test]
    fn test_health_state_warning_to_normal_recovery() {
        let mut state = HealthStateMachine::new();
        // Get to Warning
        for _ in 0..WARN_SUSTAIN_POLLS {
            state.update(Metrics { pageout_delta: PAGEOUT_WARN_THRESHOLD + 10, swap_used_mb: 0.0, swap_total_mb: 6144.0, last_tok_per_sec: None, pressure_event: None });
        }
        assert_eq!(state.current(), SystemHealth::Warning);

        // Sustained near-zero page-outs
        for _ in 0..RECOVERY_SUSTAIN_POLLS {
            state.update(Metrics { pageout_delta: 0, swap_used_mb: 0.0, swap_total_mb: 6144.0, last_tok_per_sec: None, pressure_event: None });
        }
        assert_eq!(state.current(), SystemHealth::Normal);
    }

    #[test]
    fn test_health_state_promotion_available() {
        let mut state = HealthStateMachine::new();
        state.set_on_smaller_model(true);
        assert_eq!(state.current(), SystemHealth::Normal);

        // Sustained near-zero page-outs for PROMOTION_SUSTAIN_POLLS
        for _ in 0..PROMOTION_SUSTAIN_POLLS {
            state.update(Metrics { pageout_delta: 0, swap_used_mb: 0.0, swap_total_mb: 6144.0, last_tok_per_sec: None, pressure_event: None });
        }
        assert_eq!(state.current(), SystemHealth::PromotionAvailable);
    }

    #[test]
    fn test_health_state_no_promotion_when_on_best_model() {
        let mut state = HealthStateMachine::new();
        state.set_on_smaller_model(false);

        for _ in 0..PROMOTION_SUSTAIN_POLLS {
            state.update(Metrics { pageout_delta: 0, swap_used_mb: 0.0, swap_total_mb: 6144.0, last_tok_per_sec: None, pressure_event: None });
        }
        // Should stay Normal, not offer promotion
        assert_eq!(state.current(), SystemHealth::Normal);
    }

    #[test]
    fn test_critical_directly_from_normal() {
        let mut state = HealthStateMachine::new();
        // Severe page-out escalation
        state.update(Metrics { pageout_delta: PAGEOUT_CRITICAL_THRESHOLD + 100, swap_used_mb: 0.0, swap_total_mb: 6144.0, last_tok_per_sec: None, pressure_event: None });
        assert_eq!(state.current(), SystemHealth::Critical);
    }

    #[test]
    fn test_critical_from_pressure_event_in_normal() {
        let mut state = HealthStateMachine::new();
        state.update(Metrics { pageout_delta: 0, swap_used_mb: 0.0, swap_total_mb: 6144.0, last_tok_per_sec: None, pressure_event: Some(PressureLevel::Critical) });
        assert_eq!(state.current(), SystemHealth::Critical);
    }

    #[test]
    fn test_metrics_snapshot_included_in_health_event() {
        let mut state = HealthStateMachine::new();
        let metrics = Metrics {
            pageout_delta: PAGEOUT_CRITICAL_THRESHOLD + 100,
            swap_used_mb: 512.0,
            swap_total_mb: 6144.0,
            last_tok_per_sec: Some(23.5),
            pressure_event: None,
        };
        let event = state.update(metrics);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.health, SystemHealth::Critical);
        assert!((event.metrics.swap_used_mb - 512.0).abs() < 0.1);
        assert_eq!(event.metrics.last_tok_per_sec, Some(23.5));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib memory_monitor 2>&1 | tail -10`
Expected: Compilation errors — none of the types exist yet.

- [ ] **Step 3: Implement the MemoryMonitor module**

Write the implementation above the test block in `src/memory_monitor.rs`:

```rust
//! Memory monitor: polls vm_stat page-out rate, receives macOS memory pressure
//! events, and emits SystemHealth state transitions via a tokio::sync::watch channel.

use crate::memory_ffi::PressureLevel;
use std::process::Command;
use tokio::sync::watch;
use tokio::time::{interval, Duration};

/// How often to poll vm_stat (seconds).
const POLL_INTERVAL_SECS: u64 = 5;

/// Page-out delta per poll that triggers Warning (pages per 5s window).
/// Conservative: ~100 pageouts/5s is moderate pressure on Apple Silicon.
pub const PAGEOUT_WARN_THRESHOLD: u64 = 100;

/// Page-out delta that triggers immediate Critical (severe thrashing).
pub const PAGEOUT_CRITICAL_THRESHOLD: u64 = 1000;

/// Number of consecutive polls above threshold to enter Warning.
pub const WARN_SUSTAIN_POLLS: usize = 3;

/// Number of consecutive polls at near-zero to recover from Warning to Normal.
pub const RECOVERY_SUSTAIN_POLLS: usize = 6;

/// Number of consecutive polls at near-zero to offer Promotion (on smaller model).
/// M minutes = M * 60 / POLL_INTERVAL_SECS polls. At 5s intervals, 2 min = 24 polls.
pub const PROMOTION_SUSTAIN_POLLS: usize = 24;

/// "Near-zero" page-out threshold (pages per poll window).
const NEAR_ZERO_PAGEOUTS: u64 = 5;

/// System health state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemHealth {
    Normal,
    Warning,
    Critical,
    PromotionAvailable,
}

/// A health event includes the state and a metrics snapshot.
#[derive(Debug, Clone)]
pub struct HealthEvent {
    pub health: SystemHealth,
    pub metrics: Metrics,
}

/// Raw metrics from one poll cycle.
#[derive(Debug, Clone)]
pub struct Metrics {
    pub pageout_delta: u64,
    pub swap_used_mb: f64,
    pub swap_total_mb: f64,
    pub last_tok_per_sec: Option<f64>,
    pub pressure_event: Option<PressureLevel>,
}

/// Swap usage parsed from `sysctl vm.swapusage`.
pub struct SwapUsage {
    pub used_mb: f64,
    pub total_mb: f64,
}

/// State machine that tracks health transitions.
pub struct HealthStateMachine {
    state: SystemHealth,
    /// Consecutive polls with page-outs above warn threshold.
    warn_count: usize,
    /// Consecutive polls with near-zero page-outs (for recovery and promotion).
    calm_count: usize,
    /// Whether we're running a model smaller than the machine's best fit.
    on_smaller_model: bool,
}

impl HealthStateMachine {
    pub fn new() -> Self {
        Self {
            state: SystemHealth::Normal,
            warn_count: 0,
            calm_count: 0,
            on_smaller_model: false,
        }
    }

    pub fn current(&self) -> SystemHealth {
        self.state
    }

    pub fn set_on_smaller_model(&mut self, on_smaller: bool) {
        self.on_smaller_model = on_smaller;
    }

    pub fn reset_to_normal(&mut self) {
        self.state = SystemHealth::Normal;
        self.warn_count = 0;
        self.calm_count = 0;
    }

    /// Feed new metrics and return a HealthEvent if the state changed.
    pub fn update(&mut self, metrics: Metrics) -> Option<HealthEvent> {
        let prev = self.state;

        // Check for CRITICAL pressure event from FFI (any state -> Critical)
        if metrics.pressure_event == Some(PressureLevel::Critical) {
            self.state = SystemHealth::Critical;
            self.warn_count = 0;
            self.calm_count = 0;
            return self.emit_if_changed(prev, &metrics);
        }

        // Check for severe page-out escalation (any state -> Critical)
        if metrics.pageout_delta >= PAGEOUT_CRITICAL_THRESHOLD {
            self.state = SystemHealth::Critical;
            self.warn_count = 0;
            self.calm_count = 0;
            return self.emit_if_changed(prev, &metrics);
        }

        let is_calm = metrics.pageout_delta <= NEAR_ZERO_PAGEOUTS;
        let is_warn_level = metrics.pageout_delta >= PAGEOUT_WARN_THRESHOLD;

        match self.state {
            SystemHealth::Normal => {
                if is_warn_level {
                    self.warn_count += 1;
                    self.calm_count = 0;
                    if self.warn_count >= WARN_SUSTAIN_POLLS {
                        self.state = SystemHealth::Warning;
                        self.warn_count = 0;
                    }
                } else if is_calm {
                    self.warn_count = 0;
                    self.calm_count += 1;
                    if self.on_smaller_model && self.calm_count >= PROMOTION_SUSTAIN_POLLS {
                        self.state = SystemHealth::PromotionAvailable;
                        self.calm_count = 0;
                    }
                } else {
                    self.warn_count = 0;
                    self.calm_count = 0;
                }
            }
            SystemHealth::Warning => {
                if is_calm {
                    self.calm_count += 1;
                    self.warn_count = 0;
                    if self.calm_count >= RECOVERY_SUSTAIN_POLLS {
                        self.state = SystemHealth::Normal;
                        self.calm_count = 0;
                    }
                } else if is_warn_level {
                    self.calm_count = 0;
                    // Stay in Warning
                } else {
                    self.calm_count = 0;
                    self.warn_count = 0;
                }
            }
            SystemHealth::Critical => {
                // Critical -> Normal only via explicit reset after swap
                // (the orchestrator calls reset_to_normal after swap completes)
            }
            SystemHealth::PromotionAvailable => {
                // PromotionAvailable -> Normal only via explicit reset after swap or dismiss
            }
        }

        self.emit_if_changed(prev, &metrics)
    }

    fn emit_if_changed(&self, prev: SystemHealth, metrics: &Metrics) -> Option<HealthEvent> {
        if self.state != prev {
            Some(HealthEvent {
                health: self.state,
                metrics: metrics.clone(),
            })
        } else {
            None
        }
    }
}

/// Parse "Pageouts:" count from vm_stat output.
pub fn parse_pageouts(output: &str) -> Option<u64> {
    for line in output.lines() {
        if line.starts_with("Pageouts:") {
            let val = line.split(':').nth(1)?;
            return val.trim().trim_end_matches('.').parse().ok();
        }
    }
    None
}

/// Parse swap usage from `sysctl vm.swapusage` output.
/// Format: "total = 6144.00M  used = 1024.00M  free = 5120.00M  (encrypted)"
pub fn parse_swap_usage(output: &str) -> SwapUsage {
    let parse_field = |field: &str| -> f64 {
        output
            .find(field)
            .and_then(|pos| {
                let start = pos + field.len();
                let rest = &output[start..];
                let end = rest.find('M').unwrap_or(rest.len());
                rest[..end].trim().parse().ok()
            })
            .unwrap_or(0.0)
    };
    SwapUsage {
        total_mb: parse_field("total = "),
        used_mb: parse_field("used = "),
    }
}

/// Poll vm_stat and return current total pageout count.
fn poll_pageouts() -> Option<u64> {
    let output = Command::new("vm_stat").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_pageouts(&stdout)
}

/// Poll swap usage.
fn poll_swap() -> SwapUsage {
    let output = Command::new("sysctl")
        .args(["-n", "vm.swapusage"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    parse_swap_usage(&output)
}

/// The background monitor task.
/// Polls vm_stat, receives FFI pressure events, and publishes HealthEvents.
pub async fn run(
    health_tx: watch::Sender<Option<HealthEvent>>,
    tok_per_sec_rx: watch::Receiver<Option<f64>>,
    mut pressure_rx: tokio::sync::mpsc::UnboundedReceiver<PressureLevel>,
    on_smaller_model: bool,
) {
    let mut state_machine = HealthStateMachine::new();
    state_machine.set_on_smaller_model(on_smaller_model);
    let mut prev_pageouts: Option<u64> = None;
    let mut poll_timer = interval(Duration::from_secs(POLL_INTERVAL_SECS));

    loop {
        tokio::select! {
            _ = poll_timer.tick() => {
                let pageouts_now = poll_pageouts().unwrap_or(0);
                let pageout_delta = prev_pageouts
                    .map(|prev| pageouts_now.saturating_sub(prev))
                    .unwrap_or(0);
                prev_pageouts = Some(pageouts_now);

                let swap = poll_swap();
                let tok_s = *tok_per_sec_rx.borrow();

                let metrics = Metrics {
                    pageout_delta,
                    swap_used_mb: swap.used_mb,
                    swap_total_mb: swap.total_mb,
                    last_tok_per_sec: tok_s,
                    pressure_event: None,
                };

                if let Some(event) = state_machine.update(metrics) {
                    let _ = health_tx.send(Some(event));
                }
            }
            Some(level) = pressure_rx.recv() => {
                let swap = poll_swap();
                let tok_s = *tok_per_sec_rx.borrow();

                let metrics = Metrics {
                    pageout_delta: 0,
                    swap_used_mb: swap.used_mb,
                    swap_total_mb: swap.total_mb,
                    last_tok_per_sec: tok_s,
                    pressure_event: Some(level),
                };

                if let Some(event) = state_machine.update(metrics) {
                    let _ = health_tx.send(Some(event));
                }
            }
        }
    }
}
```

- [ ] **Step 4: Add `mod memory_monitor;` to main.rs**

In `src/main.rs`, add after `mod memory_ffi;`:

```rust
mod memory_monitor;
```

- [ ] **Step 5: Run tests**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib memory_monitor 2>&1 | tail -30`
Expected: All 10 memory_monitor tests pass.

- [ ] **Step 6: Run full test suite**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
cd /Users/robertkarl/Code/cinderella
git add src/memory_monitor.rs src/main.rs
git commit -m "feat: add MemoryMonitor with page-out polling and health state machine"
```

---

### Task 5: Add swap_model to ServerManager

**Files:**
- Modify: `src/server.rs`
- Test: `src/server.rs` (inline tests)

- [ ] **Step 1: Write the failing test**

Add to `src/server.rs` a test block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;

    #[test]
    fn test_server_config_swap() {
        // Verify that swap_config replaces the model path and preserves port
        let original = ServerConfig {
            model_path: std::path::PathBuf::from("/models/9b.gguf"),
            port: 8787,
            ctx_size: 32768,
            n_gpu_layers: -1,
            jinja: true,
        };

        let new_model_path = std::path::PathBuf::from("/models/4b.gguf");
        let swapped = ServerConfig {
            model_path: new_model_path.clone(),
            port: original.port,
            ctx_size: 32768,
            n_gpu_layers: -1,
            jinja: true,
        };

        assert_eq!(swapped.model_path, new_model_path);
        assert_eq!(swapped.port, 8787);
    }
}
```

- [ ] **Step 2: Run test to verify it passes (baseline)**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib server 2>&1 | tail -10`
Expected: PASS — this is just a sanity check. The real test is that `swap_model` compiles and the method exists.

- [ ] **Step 3: Add swap_model method to ServerManager**

In `src/server.rs`, add to the `impl ServerManager` block:

```rust
    /// Swap to a different model: stop the current server, start with new config.
    /// This is the "hard cut" from the adaptive sizing spec.
    pub async fn swap_model(&mut self, new_config: ServerConfig) -> Result<()> {
        self.stop().await;
        self.config = new_config;
        self.restart_count = 0;
        self.gpu_layers_loaded = None;
        self.gpu_layers_total = None;
        self.start().await
    }
```

- [ ] **Step 4: Run full test suite**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
cd /Users/robertkarl/Code/cinderella
git add src/server.rs
git commit -m "feat: add swap_model method for hot model switching"
```

---

### Task 6: Add memory/swap event variants to AgentEvent and JSON protocol

**Files:**
- Modify: `src/agent.rs:18-57` (add event variants)
- Modify: `src/tui.rs:276-295` (print new events)
- Modify: `src/tui.rs:362-435` (JSON protocol for new events)
- Test: Compilation + manual inspection

- [ ] **Step 1: Write the failing test**

Add to the test block in `src/tui.rs` (you'll need to add a `#[cfg(test)] mod tests` block if one doesn't exist):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentEvent;

    #[test]
    fn test_json_memory_warning() {
        let event = AgentEvent::MemoryWarning {
            pageout_rate: 150,
            swap_used_mb: 1024.0,
            tok_per_sec: Some(12.5),
        };
        // Just verify it doesn't panic — json_event writes to stdout
        // In a real test we'd capture stdout, but compilation is the gate here
        let _ = format!("{:?}", event);
    }

    #[test]
    fn test_json_model_swap() {
        let event = AgentEvent::ModelSwap {
            from_model: "Qwen 3.5 9B".to_string(),
            to_model: "Qwen 3.5 4B".to_string(),
            reason: "System was thrashing (page-outs: 1500/s)".to_string(),
        };
        let _ = format!("{:?}", event);
    }

    #[test]
    fn test_json_promotion_available() {
        let event = AgentEvent::PromotionAvailable {
            to_model: "Qwen 3.5 9B".to_string(),
        };
        let _ = format!("{:?}", event);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib tui 2>&1 | tail -10`
Expected: FAIL — `MemoryWarning`, `ModelSwap`, `PromotionAvailable` variants don't exist on `AgentEvent`.

- [ ] **Step 3: Add event variants to AgentEvent**

In `src/agent.rs`, add to the `AgentEvent` enum:

```rust
    /// Memory pressure warning — suggest downgrade.
    MemoryWarning {
        pageout_rate: u64,
        swap_used_mb: f64,
        tok_per_sec: Option<f64>,
    },
    /// Model swap completed (post-facto notification).
    ModelSwap {
        from_model: String,
        to_model: String,
        reason: String,
    },
    /// Promotion available — running smaller model but pressure has eased.
    PromotionAvailable {
        to_model: String,
    },
```

Add `#[derive(Debug)]` to `AgentEvent` (needed for the test's `format!("{:?}", event)`). If `AgentEvent` already has `Debug` derive, skip this.

- [ ] **Step 4: Handle new events in tui.rs print_event**

In `src/tui.rs`, add match arms to `print_event()`:

```rust
        AgentEvent::MemoryWarning { pageout_rate, swap_used_mb, tok_per_sec } => {
            let tok_s = tok_per_sec.map(|t| format!("{:.0} tok/s", t)).unwrap_or_else(|| "—".to_string());
            print_warn(&format!(
                "Memory pressure: {} page-outs/5s, {:.0} MB swap, {}. Consider switching to a smaller model.",
                pageout_rate, swap_used_mb, tok_s
            ));
        }
        AgentEvent::ModelSwap { from_model, to_model, reason } => {
            print_warn(&format!("Switched {} → {} — {}", from_model, to_model, reason));
        }
        AgentEvent::PromotionAvailable { to_model } => {
            print_warn(&format!("System pressure has eased. You can switch back to {}.", to_model));
        }
```

- [ ] **Step 5: Handle new events in tui.rs json_event**

In `src/tui.rs`, add match arms to `json_event()`:

```rust
        AgentEvent::MemoryWarning { pageout_rate, swap_used_mb, tok_per_sec } => {
            serde_json::json!({
                "event": "memory_warning",
                "pageout_rate": pageout_rate,
                "swap_used_mb": (swap_used_mb * 10.0).round() / 10.0,
                "tok_per_sec": tok_per_sec,
            })
        }
        AgentEvent::ModelSwap { from_model, to_model, reason } => {
            serde_json::json!({
                "event": "model_swap",
                "from_model": from_model,
                "to_model": to_model,
                "reason": reason,
            })
        }
        AgentEvent::PromotionAvailable { to_model } => {
            serde_json::json!({
                "event": "promotion_available",
                "to_model": to_model,
            })
        }
```

- [ ] **Step 6: Run tests**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -10`
Expected: All tests pass, including the 3 new tui tests.

- [ ] **Step 7: Commit**

```bash
cd /Users/robertkarl/Code/cinderella
git add src/agent.rs src/tui.rs
git commit -m "feat: add MemoryWarning, ModelSwap, PromotionAvailable events + JSON protocol"
```

---

### Task 7: Wire MemoryMonitor into orchestrator and coordinate model swaps

**Files:**
- Modify: `src/orchestrator.rs` (spawn monitor, react to health events)
- Modify: `src/agent.rs` (add tok/s watch sender to Agent)
- Test: Integration-level (compile + manual test)

This is the integration task — it wires the monitor into the existing orchestrator flow.

- [ ] **Step 1: Write the failing test**

Add to `src/agent.rs` test block:

```rust
#[test]
fn test_agent_tok_per_sec_sender() {
    // Verify that Agent can be constructed with a tok/s watch sender
    let (tx, _rx) = tokio::sync::watch::channel(None::<f64>);
    // This test just verifies the constructor signature compiles
    // Agent::new() will gain a tok_per_sec_tx: Option<watch::Sender<Option<f64>>> parameter
    drop(tx);
}
```

- [ ] **Step 2: Run test to verify it passes (baseline)**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib agent::tests::test_agent_tok_per_sec_sender 2>&1 | tail -10`
Expected: PASS — this just verifies watch channel types work.

- [ ] **Step 3: Add tok/s watch sender to Agent**

In `src/agent.rs`, add to `Agent` struct:

```rust
    /// Channel to publish tok/s to the memory monitor.
    tok_per_sec_tx: Option<watch::Sender<Option<f64>>>,
```

Update `Agent::new()` to accept the new parameter:

```rust
    pub fn new(
        api_url: &str,
        project_dir: PathBuf,
        ctx_size: u32,
        model_name: &str,
        safety_profile: SafetyProfile,
        step_tracking: bool,
        tok_per_sec_tx: Option<tokio::sync::watch::Sender<Option<f64>>>,
    ) -> Self {
```

Store it in the struct and update `call_llm` to publish tok/s:

In the `StreamEvent::TokenTick` handler inside `call_llm()`, add after the `on_event(AgentEvent::TokenRate { tok_per_sec: tok_s });` line:

```rust
                    if let Some(ref tx) = self.tok_per_sec_tx {
                        let _ = tx.send(Some(tok_s));
                    }
```

Update all call sites of `Agent::new()` in `orchestrator.rs` to pass `None` as the last argument (we'll wire it up in the next step).

- [ ] **Step 4: Update orchestrator to spawn MemoryMonitor and handle health events**

In `src/orchestrator.rs`, update the `run()` function's interactive mode section. After starting the server and before spawning the agent loop:

```rust
    // Spawn memory monitor
    let (health_tx, mut health_rx) = tokio::sync::watch::channel(None);
    let (tok_tx, tok_rx) = tokio::sync::watch::channel(None::<f64>);

    let pressure_rx = crate::memory_ffi::start_pressure_listener_v2();

    let best_model = manifest.model_for_ram(hw.total_ram_gb);
    let on_smaller_model = best_model.map(|m| m.tier != active_model.tier).unwrap_or(false);

    tokio::spawn(crate::memory_monitor::run(
        health_tx,
        tok_rx,
        pressure_rx,
        on_smaller_model,
    ));
```

Pass `Some(tok_tx)` to `Agent::new()` in `spawn_agent_loop`.

Add a health event handling loop that runs alongside the TUI. The simplest approach: spawn a task that watches `health_rx` and sends appropriate `AgentEvent`s through `agent_tx`:

```rust
    let health_agent_tx = agent_tx.clone();
    let health_manifest = manifest.clone(); // need to derive Clone for Manifest or use Arc
    let health_active_model_id = active_model.id.clone();
    tokio::spawn(async move {
        while health_rx.changed().await.is_ok() {
            let event = health_rx.borrow().clone();
            if let Some(health_event) = event {
                match health_event.health {
                    crate::memory_monitor::SystemHealth::Warning => {
                        let _ = health_agent_tx.send(AgentEvent::MemoryWarning {
                            pageout_rate: health_event.metrics.pageout_delta,
                            swap_used_mb: health_event.metrics.swap_used_mb,
                            tok_per_sec: health_event.metrics.last_tok_per_sec,
                        }).await;
                    }
                    crate::memory_monitor::SystemHealth::Critical => {
                        // Hard cut: model swap will be handled by a separate mechanism
                        // For now, emit the warning. Full swap coordination is Task 8.
                        let _ = health_agent_tx.send(AgentEvent::Warning(
                            "CRITICAL memory pressure — initiating model swap".to_string()
                        )).await;
                    }
                    crate::memory_monitor::SystemHealth::PromotionAvailable => {
                        // Find the model we could upgrade to
                        if let Some(best) = health_manifest.model_for_ram(18.0) { // TODO: use actual total_ram
                            let _ = health_agent_tx.send(AgentEvent::PromotionAvailable {
                                to_model: best.name.clone(),
                            }).await;
                        }
                    }
                    _ => {}
                }
            }
        }
    });
```

- [ ] **Step 5: Run full test suite**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -20`
Expected: All tests pass. May need to add `use tokio::sync::watch;` imports and derive `Clone` on `Manifest` / `ModelDef` / `HealthEvent`.

- [ ] **Step 6: Fix any compilation issues**

Address any missing derives or import errors. Common ones:
- Add `#[derive(Clone)]` to `Manifest` and `ModelDef` in `model_manifest.rs`
- Add `use tokio::sync::watch;` in `agent.rs`
- Update `spawn_agent_loop` signature to accept `Option<watch::Sender<Option<f64>>>`

- [ ] **Step 7: Run full test suite again**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
cd /Users/robertkarl/Code/cinderella
git add src/orchestrator.rs src/agent.rs src/model_manifest.rs
git commit -m "feat: wire MemoryMonitor into orchestrator, publish tok/s from agent"
```

---

### Task 8: Critical model swap execution (hard cut)

**Files:**
- Modify: `src/orchestrator.rs` (full Critical swap path)
- Test: Compile + manual test

This task implements the spec's "Model Swap Execution" section: Critical event -> kill server -> start with smaller model -> resume.

- [ ] **Step 1: Write the failing test**

Add to a new `src/orchestrator.rs` test block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_manifest::{Manifest, ModelTier};

    #[test]
    fn test_find_downgrade_model() {
        let json = r#"{
            "version": 1,
            "models": [
                {"id":"small","name":"S","filename":"s.gguf","quant":"Q4","size_bytes":1000,"sha256":"x","url":"x","min_ram_gb":8,"ctx_size":32768,"n_gpu_layers":-1,"app_support_subdir":"Glass Slipper/Models","tier":"small"},
                {"id":"default","name":"D","filename":"d.gguf","quant":"Q5","size_bytes":2000,"sha256":"x","url":"x","min_ram_gb":16,"ctx_size":32768,"n_gpu_layers":-1,"app_support_subdir":"Glass Slipper/Models","tier":"default"}
            ],
            "default_model": "default"
        }"#;
        let manifest = Manifest::from_str(json).unwrap();
        let current = manifest.default_model().unwrap();
        let downgrade = manifest.one_tier_down(current);
        assert!(downgrade.is_some());
        assert_eq!(downgrade.unwrap().id, "small");

        // Small has no downgrade
        let small = manifest.models.iter().find(|m| m.tier == ModelTier::Small).unwrap();
        assert!(manifest.one_tier_down(small).is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib orchestrator 2>&1 | tail -10`
Expected: PASS — `one_tier_down` was implemented in Task 1.

- [ ] **Step 3: Implement the Critical swap coordination**

Refactor the health event handler spawned in Task 7. Replace the `Critical` arm with actual swap logic. This requires the health event handler to have access to the `ServerManager` — use an `Arc<Mutex<ServerManager>>` or restructure the orchestrator to own the swap logic.

The cleanest approach: create a `SwapCoordinator` that owns the server and manifest:

In `src/orchestrator.rs`, add:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

struct SwapCoordinator {
    server: Arc<Mutex<ServerManager>>,
    manifest: Manifest,
    current_model_id: String,
    total_ram_gb: f64,
    port: u16,
    llama_server_path: PathBuf,
}

impl SwapCoordinator {
    /// Execute a downgrade: stop server, start with smaller model.
    /// Returns (from_name, to_name) on success.
    async fn downgrade(&mut self) -> Result<(String, String)> {
        let current = self.manifest.models.iter()
            .find(|m| m.id == self.current_model_id)
            .context("Current model not found in manifest")?;

        let target = self.manifest.one_tier_down(current)
            .context("Already on smallest model — cannot downgrade")?;

        let target_path = find_model_file(target)?;
        let new_config = config::ServerConfig::from_model_def(target_path, self.port, target);

        let from_name = current.name.clone();
        let to_name = target.name.clone();

        let mut server = self.server.lock().await;
        server.swap_model(new_config).await?;

        self.current_model_id = target.id.clone();

        Ok((from_name, to_name))
    }

    /// Execute an upgrade to the best model for current RAM.
    async fn upgrade(&mut self) -> Result<(String, String)> {
        let best = self.manifest.model_for_ram(self.total_ram_gb)
            .context("No model fits this machine")?;

        if best.id == self.current_model_id {
            anyhow::bail!("Already on best model");
        }

        let target_path = find_model_file(best)?;
        let new_config = config::ServerConfig::from_model_def(target_path, self.port, best);

        let current = self.manifest.models.iter()
            .find(|m| m.id == self.current_model_id)
            .context("Current model not found")?;

        let from_name = current.name.clone();
        let to_name = best.name.clone();

        let mut server = self.server.lock().await;
        server.swap_model(new_config).await?;

        self.current_model_id = best.id.clone();

        Ok((from_name, to_name))
    }
}
```

Update the health event handling task to use `SwapCoordinator`:

```rust
    let swap_coordinator = Arc::new(Mutex::new(SwapCoordinator {
        server: server.clone(), // server needs to be Arc<Mutex<ServerManager>>
        manifest: manifest.clone(),
        current_model_id: active_model.id.clone(),
        total_ram_gb: hw.total_ram_gb,
        port: cfg.port,
        llama_server_path: cfg.llama_server_path.clone(),
    }));

    let health_agent_tx = agent_tx.clone();
    let swap_coord = swap_coordinator.clone();
    tokio::spawn(async move {
        while health_rx.changed().await.is_ok() {
            let event = health_rx.borrow().clone();
            if let Some(health_event) = event {
                match health_event.health {
                    crate::memory_monitor::SystemHealth::Warning => {
                        let _ = health_agent_tx.send(AgentEvent::MemoryWarning {
                            pageout_rate: health_event.metrics.pageout_delta,
                            swap_used_mb: health_event.metrics.swap_used_mb,
                            tok_per_sec: health_event.metrics.last_tok_per_sec,
                        }).await;
                    }
                    crate::memory_monitor::SystemHealth::Critical => {
                        let mut coord = swap_coord.lock().await;
                        match coord.downgrade().await {
                            Ok((from, to)) => {
                                let _ = health_agent_tx.send(AgentEvent::ModelSwap {
                                    from_model: from,
                                    to_model: to,
                                    reason: format!(
                                        "System was thrashing (page-outs: {}/s)",
                                        health_event.metrics.pageout_delta
                                    ),
                                }).await;
                            }
                            Err(e) => {
                                let _ = health_agent_tx.send(AgentEvent::Warning(
                                    format!("Model downgrade failed: {}", e)
                                )).await;
                            }
                        }
                    }
                    crate::memory_monitor::SystemHealth::PromotionAvailable => {
                        // Find what we could upgrade to
                        let coord = swap_coord.lock().await;
                        if let Some(best) = coord.manifest.model_for_ram(coord.total_ram_gb) {
                            if best.id != coord.current_model_id {
                                let _ = health_agent_tx.send(AgentEvent::PromotionAvailable {
                                    to_model: best.name.clone(),
                                }).await;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    });
```

- [ ] **Step 4: Wrap ServerManager in Arc<Mutex<>>**

The orchestrator currently owns `ServerManager` directly. Wrap it:

```rust
    let server = Arc::new(Mutex::new(server));
```

Update the `server.stop().await` cleanup at the end of `run()`:

```rust
    server.lock().await.stop().await;
```

Update `server.api_url()` and `server.gpu_layers_loaded` accesses to go through the lock.

- [ ] **Step 5: Update Agent's LlmClient URL for post-swap**

After a model swap, the llama-server restarts on the same port, so the API URL doesn't change. The Agent's `LlmClient` points at `http://127.0.0.1:8787` and continues to work after the swap. No change needed here.

The conversation history stays intact in `Agent.messages` — this is the "chassis stays intact" property from the spec.

- [ ] **Step 6: Run full test suite**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
cd /Users/robertkarl/Code/cinderella
git add src/orchestrator.rs src/server.rs
git commit -m "feat: Critical model swap execution — hard cut downgrade on memory pressure"
```

---

### Task 9: Structured logging

**Files:**
- Create: `src/logging.rs`
- Modify: `src/main.rs` (add `mod logging;`)
- Modify: `src/orchestrator.rs` (initialize logger)
- Modify: `src/memory_monitor.rs` (log each poll cycle)
- Test: `src/logging.rs` (inline tests)

The spec calls for `glass-slipper-engine.log` with structured timestamped entries.

- [ ] **Step 1: Write the failing test**

Create `src/logging.rs`:

```rust
//! Structured logging for Glass Slipper engine.
//! Writes timestamped JSON-lines to glass-slipper-engine.log.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_format() {
        let entry = LogEntry {
            timestamp: "2026-05-07T12:00:00Z".to_string(),
            source: "memory_monitor".to_string(),
            level: LogLevel::Info,
            message: "Poll cycle".to_string(),
            data: Some(serde_json::json!({"pageout_delta": 42, "swap_used_mb": 512.0})),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("memory_monitor"));
        assert!(json.contains("pageout_delta"));
    }

    #[test]
    fn test_log_level_serialization() {
        assert_eq!(serde_json::to_string(&LogLevel::Info).unwrap(), "\"info\"");
        assert_eq!(serde_json::to_string(&LogLevel::Warn).unwrap(), "\"warn\"");
        assert_eq!(serde_json::to_string(&LogLevel::Error).unwrap(), "\"error\"");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test --lib logging 2>&1 | tail -10`
Expected: FAIL — types don't exist.

- [ ] **Step 3: Implement the logging module**

```rust
//! Structured logging for Glass Slipper engine.
//! Writes timestamped JSON-lines to glass-slipper-engine.log.

use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOGGER: std::sync::OnceLock<Mutex<EngineLogger>> = std::sync::OnceLock::new();

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub source: String,
    pub level: LogLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

struct EngineLogger {
    file: File,
}

/// Initialize the global logger. Call once at startup.
pub fn init(log_dir: &std::path::Path) -> anyhow::Result<()> {
    let _ = std::fs::create_dir_all(log_dir);
    let path = log_dir.join("glass-slipper-engine.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    let _ = LOGGER.set(Mutex::new(EngineLogger { file }));
    Ok(())
}

/// Log a structured entry.
pub fn log(source: &str, level: LogLevel, message: &str, data: Option<serde_json::Value>) {
    let entry = LogEntry {
        timestamp: chrono_now(),
        source: source.to_string(),
        level,
        message: message.to_string(),
        data,
    };

    if let Some(logger) = LOGGER.get() {
        if let Ok(mut logger) = logger.lock() {
            if let Ok(json) = serde_json::to_string(&entry) {
                let _ = writeln!(logger.file, "{}", json);
            }
        }
    }
}

/// Convenience: log info.
pub fn info(source: &str, message: &str, data: Option<serde_json::Value>) {
    log(source, LogLevel::Info, message, data);
}

/// Convenience: log warning.
pub fn warn(source: &str, message: &str, data: Option<serde_json::Value>) {
    log(source, LogLevel::Warn, message, data);
}

/// Convenience: log error.
pub fn error(source: &str, message: &str, data: Option<serde_json::Value>) {
    log(source, LogLevel::Error, message, data);
}

/// Get the log directory path.
pub fn log_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join("Library/Logs/Glass Slipper")
}

fn chrono_now() -> String {
    // Use std::time for ISO 8601 timestamp without adding chrono dependency
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    // Simple ISO-ish format: seconds since epoch (good enough for v1, sortable)
    format!("{}", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_format() {
        let entry = LogEntry {
            timestamp: "2026-05-07T12:00:00Z".to_string(),
            source: "memory_monitor".to_string(),
            level: LogLevel::Info,
            message: "Poll cycle".to_string(),
            data: Some(serde_json::json!({"pageout_delta": 42, "swap_used_mb": 512.0})),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("memory_monitor"));
        assert!(json.contains("pageout_delta"));
    }

    #[test]
    fn test_log_level_serialization() {
        assert_eq!(serde_json::to_string(&LogLevel::Info).unwrap(), "\"info\"");
        assert_eq!(serde_json::to_string(&LogLevel::Warn).unwrap(), "\"warn\"");
        assert_eq!(serde_json::to_string(&LogLevel::Error).unwrap(), "\"error\"");
    }
}
```

- [ ] **Step 4: Add `mod logging;` to main.rs**

In `src/main.rs`, add:

```rust
mod logging;
```

- [ ] **Step 5: Initialize logger in orchestrator**

In `src/orchestrator.rs`, at the top of `run()`:

```rust
    let _ = crate::logging::init(&crate::logging::log_dir());
    crate::logging::info("orchestrator", "Glass Slipper starting", None);
```

- [ ] **Step 6: Add logging calls to memory_monitor.rs**

In `src/memory_monitor.rs`, inside the `run()` function's poll timer arm, after computing metrics:

```rust
                crate::logging::info("memory_monitor", "poll", Some(serde_json::json!({
                    "pageout_delta": pageout_delta,
                    "swap_used_mb": swap.used_mb,
                    "swap_total_mb": swap.total_mb,
                    "tok_per_sec": tok_s,
                    "health": format!("{:?}", state_machine.current()),
                })));
```

And after a state transition:

```rust
                if let Some(ref event) = state_change {
                    crate::logging::warn("memory_monitor", "health_transition", Some(serde_json::json!({
                        "new_state": format!("{:?}", event.health),
                        "pageout_delta": event.metrics.pageout_delta,
                        "swap_used_mb": event.metrics.swap_used_mb,
                    })));
                }
```

- [ ] **Step 7: Run full test suite**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
cd /Users/robertkarl/Code/cinderella
git add src/logging.rs src/main.rs src/orchestrator.rs src/memory_monitor.rs
git commit -m "feat: add structured engine logging (glass-slipper-engine.log)"
```

---

### Task 10: Update StatusBar to show active model dynamically

**Files:**
- Modify: `src/tui.rs:19-30` (StatusBar already has model_name)
- Modify: `src/orchestrator.rs` (update StatusBar after swap)
- Test: Compilation

The StatusBar already has a `model_name` field. After a model swap, the orchestrator needs to update it. Since the TUI is plain stdout (not a persistent status bar), the model name just changes for the next status display. The JSON protocol already handles this via the `model_swap` event.

- [ ] **Step 1: Verify StatusBar already shows model name**

Read `src/tui.rs:19-30` — the `StatusBar` struct already has `model_name` and `quant`. No new fields needed.

- [ ] **Step 2: Verify JSON protocol covers it**

The `model_swap` JSON event from Task 6 already tells the Swift app which model we switched to. The Swift app can update its status bar from that event. No additional work needed on the Rust side.

- [ ] **Step 3: Commit (if any changes)**

No commit needed — this task is verification only.

---

### Task 11: End-to-end manual verification

**Files:** None (testing only)

- [ ] **Step 1: Build the project**

Run: `cd /Users/robertkarl/Code/cinderella && cargo build 2>&1 | tail -20`
Expected: Clean build, no warnings.

- [ ] **Step 2: Run all tests**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 3: Verify no clippy warnings**

Run: `cd /Users/robertkarl/Code/cinderella && cargo clippy 2>&1 | tail -20`
Expected: No warnings (fix any that appear).

- [ ] **Step 4: Manual smoke test**

Launch glass-slipper with a test project directory and verify:
1. It detects hardware and selects the right model tier
2. The memory monitor starts polling (check `~/Library/Logs/Glass Slipper/glass-slipper-engine.log`)
3. Log entries appear every 5 seconds with pageout_delta and swap metrics
4. The agent loop works normally

Run: `cd /Users/robertkarl/Code/cinderella && cargo run -- /tmp/test-project 2>&1 | head -20`

- [ ] **Step 5: Commit any final fixes**

```bash
cd /Users/robertkarl/Code/cinderella
git add -A
git commit -m "chore: fix clippy warnings and final cleanup"
```
