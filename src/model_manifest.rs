/// Model manifest: parses the bundled model-manifest.json and provides
/// paths for Application Support model storage.
///
/// This is the single source of truth for model identity in Glass Slipper.
/// Swift, Rust, and release scripts all derive from model-manifest.json.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

/// Top-level manifest structure (matches model-manifest.json).
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub models: Vec<ModelDef>,
    pub default_model: String,
}

/// A single model definition from the manifest.
#[derive(Debug, Deserialize)]
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
}

fn default_min_macos() -> String {
    "15.0".to_string()
}

fn default_arch() -> String {
    "arm64".to_string()
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
}
