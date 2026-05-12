---
status: ACTIVE
planning_mode: PRODUCT
design_doc: /Users/robertkarl/.gauntlette/designs/cinderella/graceful-lifecycle-design-20260512-083600.md
---
# Graceful Lifecycle (Release 0.1.6)

Created by /gauntlette-start on 2026-05-12
Branch: master | Repo: cinderella
Design doc: /Users/robertkarl/.gauntlette/designs/cinderella/graceful-lifecycle-design-20260512-083600.md

## Problem Statement

Glass Slipper launches into the wrong window (network debug instead of Companion), doesn't clean up llama-server on quit, and can't be codesigned for distribution due to a CFBundleExecutable/target name mismatch. These block a release to glass-slipper.cc for early testers, investors, and HN readers.

## Vision

Launch the app, see the Companion window with setup or savings dashboard. Quit the app, llama-server cleans up. Build a DMG, it signs and notarizes. Hand someone a link, they can run it.

## Planning Mode

PRODUCT — real early testers (investors, founders, HN readers) will use this today. The Companion window is the product surface; the network debug flow is a power-user tool behind Cmd+2.

## Feature Spec

**On launch:** The Claude Companion window appears (setup checklist if first run, savings dashboard if configured). No network debug UI visible.

**Cmd+1:** Show/focus the Claude Companion window.

**Cmd+2:** Open the network debug window (URL field + Diagnose button). Created lazily on first use.

**Cmd+Q:** Before quitting, find any process on port 8787 via `lsof -ti :8787` and send SIGTERM. If a diagnosis process is running, terminate it and wait 0.5s for cleanup. Then quit.

**Codesign fix:** `CFBundleExecutable` in Info.plist changed from `"Glass Slipper"` to `"GlassSlipper"` to match the Xcode target name. Display name stays "Glass Slipper" via `CFBundleDisplayName`.

**Version:** 0.1.5 -> 0.1.6 in both VERSION and Info.plist.

## Scope

| Item | Decision | Effort | Why |
|------|----------|--------|-----|
| Companion as default window | ACCEPTED | S | Code already written in AppDelegate.swift |
| Cmd+2 for network debug | ACCEPTED | S | Code already written |
| Graceful Cmd+Q (kill llama-server) | ACCEPTED | S | Code already written |
| Fix CFBundleExecutable in Info.plist | ACCEPTED | S | Blocks codesign, trivial fix |
| Version bump to 0.1.6 | ACCEPTED | S | Patch release |
| Full GlassSlipper/Glass Slipper naming audit | DEFERRED | M | Recurring pain (broken builds 3-4x) but too risky for time-sensitive release |
| package-macos.sh space-in-name audit | ACCEPTED | S | Must verify script works with fixed Info.plist |

## Resolved Decisions

| Decision | Why | Rejected |
|----------|-----|----------|
| Companion as primary window | MCP offloading is the product now, network debug is a power-user feature | Keep network debug as default |
| Kill llama-server via lsof on port 8787 | Orphaned processes cause port conflicts for devs and testers | Track child PIDs (fragile, server may be started externally) |
| Fix Info.plist not Xcode target | Simpler, lower risk than renaming the Xcode target | Rename target to "Glass Slipper" (more invasive) |
| 0.1.6 not 0.2.0 | Small targeted changes, not a new capability | 0.2.0 (companion-first is UX shift but code is minimal) |

## Codebase Health

STATUS: HEALTHY

- Stack: Rust (CLI + MCP server) + Swift (native macOS GUI, no storyboards)
- Structure: Clean separation — Rust handles inference/tools, Swift handles UI/process management
- Test coverage: 116+ Rust unit tests, XCTest target exists for Swift
- Documentation: README, CHANGELOG, design docs in ~/.gauntlette/designs/
- Dependency freshness: Good (Cargo.lock updated recently)
- Git hygiene: Clean master, feature branches used, worktrees present

## Relevant Code

- `glass-slipper/AppDelegate.swift` — All changes live here (window setup, menu bar, graceful quit). Already modified, uncommitted.
- `glass-slipper/CompanionWindowController.swift` — The Companion window. No changes needed.
- `glass-slipper/Info.plist` — CFBundleExecutable fix needed (line 5: "Glass Slipper" -> "GlassSlipper").
- `VERSION` — Bump from 0.1.5 to 0.1.6.
- `scripts/package-macos.sh` — Packaging pipeline. Must verify it works with the Info.plist fix.
- `scripts/notarize-macos.sh` — Notarization. No changes expected.

## Relevant Design History

- Distribution design (2026-05-04): Established the packaging pipeline (`package-macos.sh`, `notarize-macos.sh`, DMG creation). This release uses that pipeline.
- Cocoa design (2026-04-30): Established the Swift GUI architecture (AppDelegate, no storyboards).

## Open Wounds

- The "Glass Slipper" vs "GlassSlipper" naming inconsistency has broken builds 3-4 times. Each time it surfaces as a different symptom (symlink, codesign, path resolution). A methodical audit is overdue.
- `CFBundleShortVersionString` in Info.plist was 0.2.0 while VERSION was 0.1.5 — version tracking is informal.
- Stale worktrees in `.claude/worktrees/` (cache-aware-savings, agent-aa535315).

## Tech Debt

- `TODO_FILL_MODEL_URL` in ModelDownloadManager.swift (placeholder URL for model download)
- `TODO-4b-sha256`, `TODO-35b-sha256` in model_manifest.rs (placeholder checksums)
- `TODO: wire TUI confirmation flow` in bash.rs
- `TODO: Protocol deviations` in tui.rs

## Out of Scope

- Full GlassSlipper/Glass Slipper naming audit (tracked as follow-up)
- New features or capabilities
- Rust code changes
- Website content updates
- Auto-updater
- Multiple model support

## Architecture

### ASCII: Architecture

```
Launch
  |
  v
AppDelegate.applicationDidFinishLaunching
  |
  +-- setupMenuBar()
  |     Cmd+1 -> showCompanionWindow()
  |     Cmd+2 -> showNetworkDebugWindow() [lazy creation]
  |     Cmd+Q -> applicationShouldTerminate()
  |
  +-- setupCompanionWindow()  [NEW: was setupWindow+setupUI]
  |     |
  |     v
  |   CompanionWindowController
  |     Setup checklist OR savings dashboard
  |
  +-- Cmd+Q flow:
        killLlamaServer()  [lsof -ti :8787 -> SIGTERM]
        terminate diagnosis process if running
        .terminateLater / .terminateNow
```

## Implementation Approaches

### Approach A: Ship as-is with fixes
Summary: Commit existing AppDelegate changes, fix Info.plist, bump version, build DMG.
Effort: S
Risk: Low
Completeness: 9/10
Reuses: Existing packaging pipeline, existing Companion window.

### Approach B: Full naming audit first
Summary: Rename all identifiers to GlassSlipper, then release.
Effort: M
Risk: Medium
Completeness: 10/10
Reuses: Same pipeline.

### Recommended
Approach A. Code is written, fixes are surgical, release is time-sensitive.

## Implementation

Files to modify:
- `glass-slipper/Info.plist` — Change CFBundleExecutable, bump version
- `VERSION` — Bump to 0.1.6

Files already modified (uncommitted):
- `glass-slipper/AppDelegate.swift` — Window reorg + graceful quit

Implementation order:
1. Fix Info.plist (CFBundleExecutable + version)
2. Bump VERSION file
3. Clean Xcode DerivedData (remove stale symlink)
4. Verify xcodebuild succeeds with codesign
5. Commit all changes
6. Run `make build_dmg`
7. Verify DMG mounts and app launches correctly
8. Notarize + deploy

Checkpoints:
1. After Info.plist fix: xcodebuild succeeds, no symlink in MacOS/
2. After DMG build: app launches to Companion window, Cmd+2 opens debug, Cmd+Q is clean
3. After notarize: DMG runs on a clean Mac without Gatekeeper issues

## Priorities

1. Fix codesign (blocks all distribution)
2. Commit the window reorg + graceful quit changes
3. Build + verify DMG
4. Notarize + deploy to glass-slipper.cc

## Follow-up: GlassSlipper Naming Audit

The space in "Glass Slipper" has broken builds 3-4 times. A methodical pass is needed:
- **Identifiers** (target name, executable name, bundle paths, script variables): `GlassSlipper`
- **Display names** (window titles, menu items, CFBundleDisplayName, CFBundleName): `Glass Slipper`
- Audit: Info.plist, project.pbxproj, package-macos.sh, notarize-macos.sh, AppDelegate.swift window titles, README references

## Gauntlette Review Report

| Review | Trigger | Runs | Status | Findings |
|--------|---------|------|--------|----------|
| Planning Kickoff | `/gauntlette-start` | 1 | DONE | Release-focused plan for 0.1.6: companion-first, graceful quit, codesign fix |
| CEO Review | `/gauntlette-ceo-review` | 1 | CLEAR | Version → 0.1.6. Quit logic revised. Naming strategy defined. |
| Design Review | `/gauntlette-design-review` | 0 | — | — |
| Engineering Review | `/gauntlette-eng-review` | 1 | CLEAR | Quit 2s deadline, window lifecycle, glass-slipper-mcp signing |
| Fresh Eyes | `/gauntlette-fresh-eyes` | 1 | CLEAR | 9 findings. Added process name check before SIGTERM. lsof stays sync. |
| Implementation | `/gauntlette-implement` | 0 | — | — |
| Code Review | `/gauntlette-code-review` | 0 | — | — |
| QA | `/gauntlette-quality-check` | 0 | — | — |
| Human Review | `/gauntlette-human-review` | 0 | — | — |
| Ship It | `/gauntlette-ship-it` | 0 | — | — |

**VERDICT:** CLEAR — fresh eyes complete, proceed to implementation
