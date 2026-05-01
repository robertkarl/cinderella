# Changelog

All notable changes to this project will be documented in this file.

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
