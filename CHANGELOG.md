# Changelog

All notable changes to this project will be documented in this file.

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
