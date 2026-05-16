/// Model manifest: parses the bundled model-manifest.json and provides
/// paths for Application Support model storage.
///
/// This is the single source of truth for model identity in Glass Slipper.
/// Swift, Rust, and release scripts all derive from model-manifest.json.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

/// Model size tier for adaptive model sizing.
/// Ordered from smallest to largest so derive(Ord) gives the right ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Tiny,
    Small,
    Default,
    Large,
}

fn default_tier() -> ModelTier {
    ModelTier::Default
}

/// Top-level manifest structure (matches model-manifest.json).
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub models: Vec<ModelDef>,
    pub default_model: String,
}

/// A single model definition from the manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelDef {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub quant: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub url: String,
    pub min_ram_gb: u32,
    #[serde(default = "default_min_macos")]
    pub min_macos: String,
    #[serde(default = "default_arch")]
    pub arch: String,
    pub ctx_size: u32,
    pub n_gpu_layers: i32,
    pub app_support_subdir: String,
    #[serde(default = "default_tier")]
    pub tier: ModelTier,
    #[serde(default = "default_cache_type")]
    pub cache_type_k: String,
    #[serde(default = "default_cache_type")]
    pub cache_type_v: String,
    #[serde(default = "default_auto_select")]
    pub auto_select: bool,
}

fn default_min_macos() -> String {
    "15.0".to_string()
}

fn default_arch() -> String {
    "arm64".to_string()
}

fn default_cache_type() -> String {
    "q8_0".to_string()
}

fn default_auto_select() -> bool {
    true
}

impl Manifest {
    /// Load manifest from a JSON file path.
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest: {}", path.display()))?;
        Self::from_str(&data)
    }

    /// Parse manifest from a JSON string.
    pub fn from_str(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to parse model manifest JSON")
    }

    /// Get the default model definition.
    pub fn default_model(&self) -> Result<&ModelDef> {
        self.models
            .iter()
            .find(|m| m.id == self.default_model)
            .with_context(|| format!("Default model '{}' not found in manifest", self.default_model))
    }

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

    /// Given a model, return the model one tier below it.
    /// Returns None if the model is already the smallest tier or if no
    /// model exists at the tier below.
    pub fn one_tier_down(&self, current: &ModelDef) -> Option<&ModelDef> {
        let lower_tier = match current.tier {
            ModelTier::Large => ModelTier::Default,
            ModelTier::Default => ModelTier::Small,
            ModelTier::Small => ModelTier::Tiny,
            ModelTier::Tiny => return None,
        };
        self.models.iter().find(|m| m.tier == lower_tier)
    }
}

impl ModelDef {
    /// Expected model file path under Application Support.
    /// ~/Library/Application Support/{app_support_subdir}/{filename}
    pub fn model_path(&self) -> PathBuf {
        app_support_dir(&self.app_support_subdir).join(&self.filename)
    }

    /// Path for partial download (.part file).
    pub fn part_path(&self) -> PathBuf {
        let mut path = self.model_path();
        let mut name = path.file_name().unwrap().to_os_string();
        name.push(".part");
        path.set_file_name(name);
        path
    }

    /// Check if the model file exists and has correct size.
    /// Full SHA-256 verification is expensive; use verify_sha256() for that.
    pub fn quick_check(&self) -> ModelState {
        let path = self.model_path();
        if !path.exists() {
            // Check for partial download
            if self.part_path().exists() {
                return ModelState::Partial;
            }
            return ModelState::Missing;
        }
        match std::fs::metadata(&path) {
            Ok(meta) if meta.len() == self.size_bytes => ModelState::Present,
            Ok(meta) => ModelState::SizeMismatch {
                expected: self.size_bytes,
                actual: meta.len(),
            },
            Err(_) => ModelState::Missing,
        }
    }

    /// Full SHA-256 verification. Expensive for large files.
    pub fn verify_sha256(&self) -> Result<bool> {
        use sha2::{Digest, Sha256};
        use std::io::Read;

        let path = self.model_path();
        if !path.exists() {
            return Ok(false);
        }

        // Skip verification if SHA is still a placeholder (debug only)
        #[cfg(debug_assertions)]
        if self.sha256.starts_with("TODO") {
            return Ok(true);
        }

        let mut file = std::fs::File::open(&path)
            .with_context(|| format!("Cannot open model for verification: {}", path.display()))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 8 * 1024 * 1024]; // 8 MB chunks
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let hash = format!("{:x}", hasher.finalize());
        Ok(hash == self.sha256)
    }
}

/// Model readiness state.
#[derive(Debug, PartialEq)]
pub enum ModelState {
    /// Model file exists and size matches manifest.
    Present,
    /// No model file found.
    Missing,
    /// Partial download exists (.part file).
    Partial,
    /// File exists but size doesn't match.
    SizeMismatch { expected: u64, actual: u64 },
}

/// Application Support directory for a given subdir.
/// Creates the directory if it doesn't exist.
pub fn app_support_dir(subdir: &str) -> PathBuf {
    let home = std::env::var("HOME").expect("$HOME must be set");
    let dir = PathBuf::from(home)
        .join("Library/Application Support")
        .join(subdir);
    // Create on first access (ignore errors — caller will handle missing dir)
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Find the bundled manifest file.
/// In release mode: look inside the app bundle (Contents/Resources/model-manifest.json).
/// In development: look at the repo root.
pub fn find_manifest() -> Result<Manifest> {
    // Try bundle path first (release mode)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bundle_dir) = exe.parent().and_then(|p| p.parent()) {
            let bundle_manifest = bundle_dir.join("Resources/model-manifest.json");
            if bundle_manifest.exists() {
                return Manifest::from_file(&bundle_manifest);
            }
        }
    }

    // Development fallback: repo root (adjacent to Cargo.toml)
    let dev_paths = [
        // Running from target/release or target/debug
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("model-manifest.json"),
    ];

    for path in &dev_paths {
        if path.exists() {
            return Manifest::from_file(path);
        }
    }

    anyhow::bail!(
        "model-manifest.json not found. \
         Expected in app bundle (Contents/Resources/) or repo root."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MANIFEST: &str = r#"{
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
            "min_macos": "15.0",
            "arch": "arm64",
            "ctx_size": 32768,
            "n_gpu_layers": -1,
            "app_support_subdir": "Glass Slipper/Models"
        }],
        "default_model": "qwen3.5-9b-q5"
    }"#;

    #[test]
    fn test_parse_manifest() {
        let manifest = Manifest::from_str(TEST_MANIFEST).unwrap();
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.models.len(), 1);
        assert_eq!(manifest.default_model, "qwen3.5-9b-q5");
    }

    #[test]
    fn test_default_model() {
        let manifest = Manifest::from_str(TEST_MANIFEST).unwrap();
        let model = manifest.default_model().unwrap();
        assert_eq!(model.name, "Qwen 3.5 9B");
        assert_eq!(model.filename, "Qwen3.5-9B-Q5_K_M.gguf");
        assert_eq!(model.ctx_size, 32768);
        assert_eq!(model.n_gpu_layers, -1);
        assert_eq!(model.min_ram_gb, 16);
    }

    #[test]
    fn test_model_paths() {
        let manifest = Manifest::from_str(TEST_MANIFEST).unwrap();
        let model = manifest.default_model().unwrap();
        let path = model.model_path();
        assert!(path.to_str().unwrap().contains("Application Support/Glass Slipper/Models"));
        assert!(path.to_str().unwrap().ends_with("Qwen3.5-9B-Q5_K_M.gguf"));

        let part = model.part_path();
        assert!(part.to_str().unwrap().ends_with("Qwen3.5-9B-Q5_K_M.gguf.part"));
    }

    #[test]
    fn test_quick_check_missing() {
        // Use a fake subdir so the model path won't exist on any machine
        let json = TEST_MANIFEST.replace(
            "Glass Slipper/Models",
            "Glass Slipper/TestNonexistent_12345",
        );
        let manifest = Manifest::from_str(&json).unwrap();
        let model = manifest.default_model().unwrap();
        let state = model.quick_check();
        assert!(matches!(state, ModelState::Missing | ModelState::Partial));
    }

    // --- Tier and adaptive sizing tests ---

    const TIERED_MANIFEST: &str = r#"{
        "version": 1,
        "models": [
            {
                "id": "qwen3.5-4b-q5",
                "name": "Qwen 3.5 4B",
                "filename": "Qwen3.5-4B-Q5_K_M.gguf",
                "quant": "Q5_K_M",
                "size_bytes": 2740000000,
                "sha256": "TODO-4b-sha256",
                "url": "https://example.com/4b.gguf",
                "min_ram_gb": 8,
                "ctx_size": 32768,
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

    #[test]
    fn test_backward_compat_no_tier_field() {
        // TEST_MANIFEST has no "tier" field — should default to Default
        let manifest = Manifest::from_str(TEST_MANIFEST).unwrap();
        let model = manifest.default_model().unwrap();
        assert_eq!(model.tier, ModelTier::Default);
    }

    #[test]
    fn test_parse_manifest_with_tiers() {
        let manifest = Manifest::from_str(TIERED_MANIFEST).unwrap();
        assert_eq!(manifest.models.len(), 3);

        let small = manifest.models.iter().find(|m| m.id == "qwen3.5-4b-q5").unwrap();
        assert_eq!(small.tier, ModelTier::Small);

        let default = manifest.models.iter().find(|m| m.id == "qwen3.5-9b-q5").unwrap();
        assert_eq!(default.tier, ModelTier::Default);

        let large = manifest.models.iter().find(|m| m.id == "qwen3.5-35b-moe-q5").unwrap();
        assert_eq!(large.tier, ModelTier::Large);
    }

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

    #[test]
    fn test_cache_type_defaults() {
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

    #[test]
    fn test_one_tier_down() {
        let manifest = Manifest::from_str(TIERED_MANIFEST).unwrap();

        let large = manifest.models.iter().find(|m| m.tier == ModelTier::Large).unwrap();
        let down = manifest.one_tier_down(large).unwrap();
        assert_eq!(down.tier, ModelTier::Default);
        assert_eq!(down.id, "qwen3.5-9b-q5");

        let default = manifest.models.iter().find(|m| m.tier == ModelTier::Default).unwrap();
        let down = manifest.one_tier_down(default).unwrap();
        assert_eq!(down.tier, ModelTier::Small);
        assert_eq!(down.id, "qwen3.5-4b-q5");

        let small = manifest.models.iter().find(|m| m.tier == ModelTier::Small).unwrap();
        let down = manifest.one_tier_down(small);
        assert!(down.is_none());
    }

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
}
