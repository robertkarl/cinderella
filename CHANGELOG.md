# Changelog

All notable changes to this project will be documented in this file.

## [0.1.5] - 2026-05-05

### Added
- First-run model download: GUI downloads Qwen 3.5 9B from HuggingFace with progress bar and SHA-256 verification
- ModelDownloadManager (Swift) with resumable URLSession downloads, disk space preflight, and retry UI
- ModelDownloadRowView with missing/progress/verifying/error states
- model-manifest.json as single source of truth for model identity across Swift and Rust
- model_manifest.rs: Rust-side manifest parser with quick_check and verify_sha256
- macOS packaging scripts (package-macos.sh, notarize-macos.sh, build-llama.sh)
- Portable llama-server bundling in app bundle Contents/MacOS

### Changed
- Model storage moved from ~/models/ to ~/Library/Application Support/Cinderella/Models/
- Bundled model changed from Qwen3.5-35B-MoE Q4_K_M to Qwen3.5-9B Q5_K_M (6.1 GB)
- Diagnose button disabled until model is present
- Release mode fails closed: no Homebrew/PATH fallback for llama-server or model
- App title renamed from "Glass Slipper" to "Cinderella"
- Diagnostic runbook uses portable `nc` loop instead of nmap for port scanning

### Fixed
- is_release_bundle traversal correctly detects .app bundle context
- test_quick_check_missing uses fake subdir so it passes on machines with the model downloaded

## [0.1.4] - 2026-05-01

### Added
- PlanRowView, ThoughtRowView, DiagnosisRowView — all 5 event row types now render
- UserPrompt, Plan, Diagnosis JSON event types in Rust protocol
- Swift AppDelegate with full process management (replaces ObjC main.m)
- Click-to-copy on row views via gesture recognizer
- JSONL debug logging to NSTemporaryDirectory for tail -f
- Regression test for Diagnosis emission via STEP marker
- Xcode build phase to auto-build cinderella via cargo
- Ambiguous port resolution step in diagnostic runbook

### Changed
- Glass Slipper is now pure Swift (no ObjC)
- StepTracker.close_step() emits Diagnosis from all step-closing paths
- traceroute uses per-probe timeout (-w 2) instead of outer timeout wrapper

### Removed
- ObjC files: main.m, AppDelegate.h, DiagnosticStepCell.m/h, bridging header, Makefile

### Fixed
- Diagnosis event no longer silently dropped when synthesis step closed by STEP marker
- Click-to-copy no longer abuses NSView.identifier for text storage
- Log file handle no longer leaked on early return from startDiagnosis
- Removed redundant DispatchQueue.main.async (already on main thread)
- Xcode build phase no longer skips cargo rebuild when non-tracked .rs files change

## [0.1.3] - 2026-04-30

### Added
- Xcode project for Glass Slipper (ObjC/Swift mixed build)
- CinderellaScaffold.swift design system with color, typography, and spacing tokens
- UserPromptRowView implementation following CheckRowView token pattern
- SpineViewController with NSScrollView/NSStackView spine layout
- Bridging header for ObjC/Swift interop
- Run Script build phase to symlink cinderella binary into build products
- `findLlamaServer` to pass `--llama-server` path explicitly to cinderella

### Changed
- StepTracker now captures tool command/output for step_complete detail fields
- Summary selection prefers tool output lines over `$` command prefixes

### Fixed
- Diagnostic steps (DNS, connectivity, etc.) no longer show empty detail in Glass Slipper
- .gitignore updated to exclude xcuserdata and .DS_Store
