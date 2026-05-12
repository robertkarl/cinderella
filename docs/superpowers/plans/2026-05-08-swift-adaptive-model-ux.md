# Swift Adaptive Model UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render memory pressure warnings, model swap notifications, and promotion offers in the Glass Slipper macOS app, plus a persistent status bar showing the active model and tok/s.

**Architecture:** The Rust backend already emits `memory_warning`, `model_swap`, and `promotion_available` JSON events on stdout. The Swift app already reads JSON lines from the Rust subprocess and dispatches them through `handleEvent()` into the `SpineViewController` stack. This plan adds: (1) three new `Event` enum cases, (2) three new row views matching the existing token-based design system, (3) a status bar view above the spine, (4) banner dismiss/action callbacks via delegate, (5) structured JSONL app-side logging, and (6) a small Rust change to forward `TokenRate` events in JSON mode (currently silently dropped).

**Tech Stack:** Swift (AppKit, no SwiftUI), NSView/NSStackView, existing design token system (`CinderellaScaffold.swift`), Foundation `JSONSerialization`

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Modify | `glass-slipper/CinderellaScaffold.swift:55-90` | Add amber/red surface + accent tokens for warning/critical banners |
| Modify | `glass-slipper/CinderellaScaffold.swift:162-171` | Add `memoryWarning`, `modelSwap`, `promotionAvailable` to `Event` enum |
| Modify | `glass-slipper/CinderellaScaffold.swift:233-254` | Add factory cases in `EventRowFactory.makeRow(for:)` |
| Modify | `glass-slipper/CinderellaScaffold.swift:910-922` | Add copyable text extraction for new events |
| Modify | `glass-slipper/CinderellaScaffold.swift:937-942` | Update `shouldShowDividerAbove` for new events |
| Create | `glass-slipper/MemoryBannerViews.swift` | `WarningBannerView`, `ModelSwapBannerView`, `PromotionBannerView` |
| Create | `glass-slipper/StatusBarView.swift` | Persistent status bar: green/yellow/red dot, model name, tok/s |
| Modify | `glass-slipper/AppDelegate.swift:12-30` | Add `statusBar` property, `currentModelName`, `warningDismissedUntil`, `appLogHandle` |
| Modify | `glass-slipper/AppDelegate.swift:139-180` | Insert `StatusBarView` between URL bar and spine |
| Modify | `glass-slipper/AppDelegate.swift:502-559` | Handle `memory_warning`, `model_swap`, `promotion_available`, `token_rate` events |
| Modify | `src/tui.rs:426` | Emit `TokenRate` as JSON instead of silently dropping it |

---

### Task 1: Add design tokens for warning/critical banners

The existing color palette has emerald (ok), amber (warn), and red (err) tokens for diagnosis rows. Warning and critical banners need their own surface + accent tokens that don't clash with diagnosis colors. Add them to the existing `NSColor` extension.

**Files:**
- Modify: `glass-slipper/CinderellaScaffold.swift:55-90`

- [ ] **Step 1: Add banner color tokens**

In `glass-slipper/CinderellaScaffold.swift`, after line 77 (the `accentProgress` token), add:

```swift
    // Memory pressure banners
    static let surfaceWarningBanner = NSColor(hex: 0xFEFCE8)       // yellow-50
    static let accentWarningBanner  = NSColor(hex: 0xEAB308)       // yellow-500
    static let textWarningBanner    = NSColor(hex: 0x713F12)       // yellow-900
    static let surfaceCriticalBanner = NSColor(hex: 0xFEF2F2)      // red-50
    static let accentCriticalBanner  = NSColor(hex: 0xEF4444)      // red-500
    static let textCriticalBanner    = NSColor(hex: 0x7F1D1D)      // red-900
    static let surfacePromotionBanner = NSColor(hex: 0xECFDF5)     // emerald-50
    static let accentPromotionBanner  = NSColor(hex: 0x10B981)     // emerald-500
    static let textPromotionBanner    = NSColor(hex: 0x064E3B)     // emerald-900
```

- [ ] **Step 2: Add banner typography token**

After line 100 (`stampLabel`), add:

```swift
    static var bannerBody: NSFont { .systemFont(ofSize: 12, weight: .medium) }
```

- [ ] **Step 3: Build and verify no compile errors**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add glass-slipper/CinderellaScaffold.swift
git commit -m "feat(swift): add design tokens for memory pressure banners"
```

---

### Task 2: Extend Event enum and EventRowFactory

Add three new cases to the `Event` enum and wire them into the row factory, copyable text, and divider logic. The row views don't exist yet — the factory will return placeholder `NSView()`s that get replaced in Tasks 4–6.

**Files:**
- Modify: `glass-slipper/CinderellaScaffold.swift:162-171` (Event enum)
- Modify: `glass-slipper/CinderellaScaffold.swift:233-254` (EventRowFactory)
- Modify: `glass-slipper/CinderellaScaffold.swift:910-922` (copyableText)
- Modify: `glass-slipper/CinderellaScaffold.swift:937-942` (shouldShowDividerAbove)

- [ ] **Step 1: Add new Event cases**

In `glass-slipper/CinderellaScaffold.swift`, replace the `Event` enum (lines 162–171) with:

```swift
enum Event {
    case userPrompt(text: String)
    case plan(items: [String])
    case check(name: String, status: EventStatus, detail: String)
    case thought(text: String)
    case diagnosis(text: String, status: EventStatus)
    case hwInfo(chip: String, ramUsed: Double, ramTotal: Double, gpu: String)
    case connecting
    case modelDownload
    case memoryWarning(pageoutRate: UInt64, swapUsedMB: Double, tokPerSec: Double?)
    case modelSwap(fromModel: String, toModel: String, reason: String)
    case promotionAvailable(toModel: String)
}
```

- [ ] **Step 2: Add factory cases with placeholder views**

In `EventRowFactory.makeRow(for:)` (around line 234), add cases before the closing brace of the switch:

```swift
        case .memoryWarning:
            return NSView() // placeholder — replaced in Task 4
        case .modelSwap:
            return NSView() // placeholder — replaced in Task 5
        case .promotionAvailable:
            return NSView() // placeholder — replaced in Task 6
```

- [ ] **Step 3: Add copyable text for new events**

In `SpineViewController.copyableText(for:)` (around line 911), add cases before the closing brace:

```swift
        case .memoryWarning(let pageoutRate, let swapUsedMB, let tokPerSec):
            let rate = tokPerSec.map { String(format: "%.1f tok/s", $0) } ?? "—"
            return "Memory warning: page-outs \(pageoutRate)/s, swap \(String(format: "%.0f", swapUsedMB)) MB, \(rate)"
        case .modelSwap(let fromModel, let toModel, let reason):
            return "Switched from \(fromModel) to \(toModel): \(reason)"
        case .promotionAvailable(let toModel):
            return "Promotion available: switch to \(toModel)"
```

- [ ] **Step 4: Update shouldShowDividerAbove**

In `shouldShowDividerAbove` (around line 937), add the new banner events to the `false` return group — they have their own visual separation:

Replace:
```swift
        case .diagnosis, .userPrompt, .hwInfo, .connecting, .modelDownload: return false
```
With:
```swift
        case .diagnosis, .userPrompt, .hwInfo, .connecting, .modelDownload,
             .memoryWarning, .modelSwap, .promotionAvailable: return false
```

- [ ] **Step 5: Build and verify**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 6: Commit**

```bash
git add glass-slipper/CinderellaScaffold.swift
git commit -m "feat(swift): add memoryWarning, modelSwap, promotionAvailable to Event enum"
```

---

### Task 3: Create StatusBarView

A persistent bar showing `● Qwen 9B · 23 tok/s`. The dot is green (Normal), yellow (Warning), or red (Critical/swapping). This view lives between the URL bar and the spine — it's always visible, not part of the event scroll.

**Files:**
- Create: `glass-slipper/StatusBarView.swift`

- [ ] **Step 1: Create StatusBarView**

Create `glass-slipper/StatusBarView.swift`:

```swift
//
//  StatusBarView.swift
//  Glass Slipper — persistent model status bar
//

import AppKit

/// Health state for the status dot color.
enum HealthDotState {
    case normal   // green
    case warning  // yellow
    case critical // red (during swap)

    var color: NSColor {
        switch self {
        case .normal:   return NSColor(hex: 0x22C55E)  // green-500
        case .warning:  return NSColor(hex: 0xEAB308)  // yellow-500
        case .critical: return NSColor(hex: 0xEF4444)  // red-500
        }
    }
}

final class StatusBarView: NSView {
    private let dotView = NSView()
    private let label = NSTextField(labelWithString: "")

    private var dotState: HealthDotState = .normal
    private var modelName: String = ""
    private var tokPerSec: Double?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = NSColor.surfaceHeader.cgColor
        translatesAutoresizingMaskIntoConstraints = false

        dotView.wantsLayer = true
        dotView.layer?.cornerRadius = 4  // 8pt circle / 2
        dotView.translatesAutoresizingMaskIntoConstraints = false

        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = .detailText
        label.textColor = .textSecondary
        label.maximumNumberOfLines = 1

        addSubview(dotView)
        addSubview(label)

        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 28),

            dotView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.rowHorizontal),
            dotView.centerYAnchor.constraint(equalTo: centerYAnchor),
            dotView.widthAnchor.constraint(equalToConstant: 8),
            dotView.heightAnchor.constraint(equalToConstant: 8),

            label.leadingAnchor.constraint(equalTo: dotView.trailingAnchor, constant: Spacing.md),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            label.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Spacing.rowHorizontal),
        ])

        render()
    }

    /// Update the model name shown in the status bar.
    func setModelName(_ name: String) {
        modelName = name
        render()
    }

    /// Update tok/s from the latest inference response.
    func setTokPerSec(_ rate: Double) {
        tokPerSec = rate
        render()
    }

    /// Update the health dot color.
    func setHealthState(_ state: HealthDotState) {
        dotState = state
        render()
    }

    private func render() {
        dotView.layer?.backgroundColor = dotState.color.cgColor

        var text = modelName.isEmpty ? "No model" : modelName
        if let rate = tokPerSec {
            text += String(format: " · %.0f tok/s", rate)
        }
        label.stringValue = text
    }
}
```

- [ ] **Step 2: Add StatusBarView.swift to the Xcode project**

Open the Xcode project file and add `StatusBarView.swift` to the `GlassSlipper` target's compile sources. The fastest way is:

Run: `cd /Users/robertkarl/Code/cinderella && python3 -c "
import subprocess, re, os

# Read the project.pbxproj
proj_path = 'glass-slipper/GlassSlipper.xcodeproj/project.pbxproj'
with open(proj_path) as f:
    content = f.read()

# Check if already added
if 'StatusBarView.swift' in content:
    print('Already in project')
else:
    print('NOT in project — must add manually via Xcode or pbxproj edit')
"`

If not in the project, open the Xcode project and drag `StatusBarView.swift` into the GlassSlipper group, ensuring "Add to target: GlassSlipper" is checked. Alternatively, use the same approach the project uses for its other files — check how `ModelDownloadManager.swift` is referenced in the `.pbxproj` and replicate the pattern for `StatusBarView.swift`.

- [ ] **Step 3: Build and verify**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add glass-slipper/StatusBarView.swift glass-slipper/GlassSlipper.xcodeproj/project.pbxproj
git commit -m "feat(swift): add StatusBarView with health dot, model name, tok/s"
```

---

### Task 4: Create WarningBannerView

Inline banner for the `memory_warning` event. Shows page-out rate, tok/s degradation, swap used. Has a "Switch to 4B" action button and a "Dismiss" button. Callbacks fire via closures set by AppDelegate.

**Files:**
- Create: `glass-slipper/MemoryBannerViews.swift`

- [ ] **Step 1: Create MemoryBannerViews.swift with WarningBannerView**

Create `glass-slipper/MemoryBannerViews.swift`:

```swift
//
//  MemoryBannerViews.swift
//  Glass Slipper — inline banners for memory pressure events
//

import AppKit

/// Protocol for banner action callbacks.
protocol MemoryBannerDelegate: AnyObject {
    /// User tapped "Switch to <model>" on a warning banner.
    func warningBannerDidRequestSwap()
    /// User dismissed the warning banner.
    func warningBannerDidDismiss()
    /// User accepted promotion.
    func promotionBannerDidAccept()
    /// User dismissed promotion.
    func promotionBannerDidDismiss()
}

// MARK: - Warning Banner

final class WarningBannerView: NSView {
    weak var delegate: MemoryBannerDelegate?
    private let borderView = NSView()
    private let headerLabel = NSTextField(labelWithString: "")
    private let bodyLabel = NSTextField(wrappingLabelWithString: "")
    private let actionButton = NSButton()
    private let dismissButton = NSButton()

    init(pageoutRate: UInt64, swapUsedMB: Double, tokPerSec: Double?, switchToModel: String) {
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfaceWarningBanner.cgColor
        translatesAutoresizingMaskIntoConstraints = false

        // Left border
        borderView.wantsLayer = true
        borderView.layer?.backgroundColor = NSColor.accentWarningBanner.cgColor
        borderView.translatesAutoresizingMaskIntoConstraints = false

        // Header
        let headerAttr = NSAttributedString(string: "MEMORY PRESSURE", attributes: [
            .font: NSFont.diagnosisLabel,
            .foregroundColor: NSColor.textWarningBanner,
            .kern: 1.0,
        ])
        headerLabel.attributedStringValue = headerAttr
        headerLabel.translatesAutoresizingMaskIntoConstraints = false

        // Body
        var bodyText = "Page-outs: \(pageoutRate)/s · Swap: \(String(format: "%.0f", swapUsedMB)) MB"
        if let rate = tokPerSec {
            bodyText += String(format: " · %.0f tok/s", rate)
        }
        bodyLabel.stringValue = bodyText
        bodyLabel.font = .bannerBody
        bodyLabel.textColor = .textWarningBanner
        bodyLabel.maximumNumberOfLines = 0
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false

        // Action button
        actionButton.title = "Switch to \(switchToModel)"
        actionButton.bezelStyle = .rounded
        actionButton.controlSize = .small
        actionButton.target = self
        actionButton.action = #selector(actionTapped)
        actionButton.translatesAutoresizingMaskIntoConstraints = false

        // Dismiss button
        dismissButton.title = "Dismiss"
        dismissButton.bezelStyle = .rounded
        dismissButton.controlSize = .small
        dismissButton.target = self
        dismissButton.action = #selector(dismissTapped)
        dismissButton.translatesAutoresizingMaskIntoConstraints = false

        addSubview(borderView)
        addSubview(headerLabel)
        addSubview(bodyLabel)
        addSubview(actionButton)
        addSubview(dismissButton)

        NSLayoutConstraint.activate([
            borderView.leadingAnchor.constraint(equalTo: leadingAnchor),
            borderView.topAnchor.constraint(equalTo: topAnchor),
            borderView.bottomAnchor.constraint(equalTo: bottomAnchor),
            borderView.widthAnchor.constraint(equalToConstant: Spacing.diagBorderW),

            headerLabel.leadingAnchor.constraint(equalTo: borderView.trailingAnchor, constant: Spacing.rowHorizontal),
            headerLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.lg),

            bodyLabel.leadingAnchor.constraint(equalTo: headerLabel.leadingAnchor),
            bodyLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            bodyLabel.topAnchor.constraint(equalTo: headerLabel.bottomAnchor, constant: Spacing.sm),

            actionButton.leadingAnchor.constraint(equalTo: headerLabel.leadingAnchor),
            actionButton.topAnchor.constraint(equalTo: bodyLabel.bottomAnchor, constant: Spacing.md),
            actionButton.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.lg),

            dismissButton.leadingAnchor.constraint(equalTo: actionButton.trailingAnchor, constant: Spacing.md),
            dismissButton.centerYAnchor.constraint(equalTo: actionButton.centerYAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }

    @objc private func actionTapped() { delegate?.warningBannerDidRequestSwap() }
    @objc private func dismissTapped() { delegate?.warningBannerDidDismiss() }
}
```

- [ ] **Step 2: Add MemoryBannerViews.swift to the Xcode project**

Same approach as Task 3 Step 2 — add to the GlassSlipper target compile sources in the `.xcodeproj`.

- [ ] **Step 3: Build and verify**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add glass-slipper/MemoryBannerViews.swift glass-slipper/GlassSlipper.xcodeproj/project.pbxproj
git commit -m "feat(swift): add WarningBannerView with action and dismiss buttons"
```

---

### Task 5: Add ModelSwapBannerView and PromotionBannerView

Two more banner views in the same file. The model swap banner is post-facto (no action — just info). The promotion banner has "Switch back" and "Dismiss" buttons.

**Files:**
- Modify: `glass-slipper/MemoryBannerViews.swift`

- [ ] **Step 1: Add ModelSwapBannerView**

Append to `glass-slipper/MemoryBannerViews.swift`:

```swift
// MARK: - Model Swap Banner (post-facto, informational)

final class ModelSwapBannerView: NSView {
    private let borderView = NSView()
    private let headerLabel = NSTextField(labelWithString: "")
    private let bodyLabel = NSTextField(wrappingLabelWithString: "")

    init(fromModel: String, toModel: String, reason: String) {
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfaceCriticalBanner.cgColor
        translatesAutoresizingMaskIntoConstraints = false

        borderView.wantsLayer = true
        borderView.layer?.backgroundColor = NSColor.accentCriticalBanner.cgColor
        borderView.translatesAutoresizingMaskIntoConstraints = false

        let headerAttr = NSAttributedString(string: "MODEL SWITCHED", attributes: [
            .font: NSFont.diagnosisLabel,
            .foregroundColor: NSColor.textCriticalBanner,
            .kern: 1.0,
        ])
        headerLabel.attributedStringValue = headerAttr
        headerLabel.translatesAutoresizingMaskIntoConstraints = false

        bodyLabel.stringValue = "Switched to \(toModel) — \(reason). Current step was cancelled."
        bodyLabel.font = .bannerBody
        bodyLabel.textColor = .textCriticalBanner
        bodyLabel.maximumNumberOfLines = 0
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false

        addSubview(borderView)
        addSubview(headerLabel)
        addSubview(bodyLabel)

        NSLayoutConstraint.activate([
            borderView.leadingAnchor.constraint(equalTo: leadingAnchor),
            borderView.topAnchor.constraint(equalTo: topAnchor),
            borderView.bottomAnchor.constraint(equalTo: bottomAnchor),
            borderView.widthAnchor.constraint(equalToConstant: Spacing.diagBorderW),

            headerLabel.leadingAnchor.constraint(equalTo: borderView.trailingAnchor, constant: Spacing.rowHorizontal),
            headerLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.lg),

            bodyLabel.leadingAnchor.constraint(equalTo: headerLabel.leadingAnchor),
            bodyLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            bodyLabel.topAnchor.constraint(equalTo: headerLabel.bottomAnchor, constant: Spacing.sm),
            bodyLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.lg),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}
```

- [ ] **Step 2: Add PromotionBannerView**

Append to `glass-slipper/MemoryBannerViews.swift`:

```swift
// MARK: - Promotion Banner

final class PromotionBannerView: NSView {
    weak var delegate: MemoryBannerDelegate?
    private let borderView = NSView()
    private let headerLabel = NSTextField(labelWithString: "")
    private let bodyLabel = NSTextField(wrappingLabelWithString: "")
    private let acceptButton = NSButton()
    private let dismissButton = NSButton()

    init(toModel: String) {
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfacePromotionBanner.cgColor
        translatesAutoresizingMaskIntoConstraints = false

        borderView.wantsLayer = true
        borderView.layer?.backgroundColor = NSColor.accentPromotionBanner.cgColor
        borderView.translatesAutoresizingMaskIntoConstraints = false

        let headerAttr = NSAttributedString(string: "UPGRADE AVAILABLE", attributes: [
            .font: NSFont.diagnosisLabel,
            .foregroundColor: NSColor.textPromotionBanner,
            .kern: 1.0,
        ])
        headerLabel.attributedStringValue = headerAttr
        headerLabel.translatesAutoresizingMaskIntoConstraints = false

        bodyLabel.stringValue = "System pressure has eased. Switch back to \(toModel)?"
        bodyLabel.font = .bannerBody
        bodyLabel.textColor = .textPromotionBanner
        bodyLabel.maximumNumberOfLines = 0
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false

        acceptButton.title = "Switch to \(toModel)"
        acceptButton.bezelStyle = .rounded
        acceptButton.controlSize = .small
        acceptButton.target = self
        acceptButton.action = #selector(acceptTapped)
        acceptButton.translatesAutoresizingMaskIntoConstraints = false

        dismissButton.title = "Dismiss"
        dismissButton.bezelStyle = .rounded
        dismissButton.controlSize = .small
        dismissButton.target = self
        dismissButton.action = #selector(dismissTapped)
        dismissButton.translatesAutoresizingMaskIntoConstraints = false

        addSubview(borderView)
        addSubview(headerLabel)
        addSubview(bodyLabel)
        addSubview(acceptButton)
        addSubview(dismissButton)

        NSLayoutConstraint.activate([
            borderView.leadingAnchor.constraint(equalTo: leadingAnchor),
            borderView.topAnchor.constraint(equalTo: topAnchor),
            borderView.bottomAnchor.constraint(equalTo: bottomAnchor),
            borderView.widthAnchor.constraint(equalToConstant: Spacing.diagBorderW),

            headerLabel.leadingAnchor.constraint(equalTo: borderView.trailingAnchor, constant: Spacing.rowHorizontal),
            headerLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.lg),

            bodyLabel.leadingAnchor.constraint(equalTo: headerLabel.leadingAnchor),
            bodyLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            bodyLabel.topAnchor.constraint(equalTo: headerLabel.bottomAnchor, constant: Spacing.sm),

            acceptButton.leadingAnchor.constraint(equalTo: headerLabel.leadingAnchor),
            acceptButton.topAnchor.constraint(equalTo: bodyLabel.bottomAnchor, constant: Spacing.md),
            acceptButton.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.lg),

            dismissButton.leadingAnchor.constraint(equalTo: acceptButton.trailingAnchor, constant: Spacing.md),
            dismissButton.centerYAnchor.constraint(equalTo: acceptButton.centerYAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }

    @objc private func acceptTapped() { delegate?.promotionBannerDidAccept() }
    @objc private func dismissTapped() { delegate?.promotionBannerDidDismiss() }
}
```

- [ ] **Step 3: Build and verify**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add glass-slipper/MemoryBannerViews.swift
git commit -m "feat(swift): add ModelSwapBannerView and PromotionBannerView"
```

---

### Task 6: Wire factory to real banner views

Replace the placeholder `NSView()` returns in `EventRowFactory` with the actual banner views.

**Files:**
- Modify: `glass-slipper/CinderellaScaffold.swift:233-254` (EventRowFactory)

- [ ] **Step 1: Replace placeholder factory cases**

In `EventRowFactory.makeRow(for:)`, replace the three placeholder cases with:

```swift
        case .memoryWarning(let pageoutRate, let swapUsedMB, let tokPerSec):
            return WarningBannerView(pageoutRate: pageoutRate, swapUsedMB: swapUsedMB, tokPerSec: tokPerSec, switchToModel: "smaller model")
        case .modelSwap(let fromModel, let toModel, let reason):
            return ModelSwapBannerView(fromModel: fromModel, toModel: toModel, reason: reason)
        case .promotionAvailable(let toModel):
            return PromotionBannerView(toModel: toModel)
```

Note: The `switchToModel` param in `WarningBannerView` is a placeholder string here — AppDelegate will override by constructing the view directly when it knows the actual target model name. This factory path is the fallback for generic event replay.

- [ ] **Step 2: Build and verify**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add glass-slipper/CinderellaScaffold.swift
git commit -m "feat(swift): wire EventRowFactory to real banner views"
```

---

### Task 7: Emit TokenRate in JSON mode (Rust)

The Rust `json_event()` function currently drops `TokenRate` events silently (`return false`). The Swift status bar needs tok/s updates. Emit them as `{"event": "token_rate", "tok_per_sec": 23.4}`.

**Files:**
- Modify: `src/tui.rs:426`

- [ ] **Step 1: Replace the TokenRate drop with JSON emission**

In `src/tui.rs`, replace line 426:

```rust
        AgentEvent::TokenRate { .. } => return false,
```

With:

```rust
        AgentEvent::TokenRate { tok_per_sec } => {
            serde_json::json!({"event": "token_rate", "tok_per_sec": tok_per_sec})
        }
```

- [ ] **Step 2: Run Rust tests**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs
git commit -m "feat(protocol): emit token_rate JSON event for Swift status bar"
```

---

### Task 8: Add StatusBarView to AppDelegate layout

Insert the `StatusBarView` into the window between the URL bar and the spine. Add `currentModelName` tracking and the `statusBar` property.

**Files:**
- Modify: `glass-slipper/AppDelegate.swift:12-30` (properties)
- Modify: `glass-slipper/AppDelegate.swift:139-180` (setupUI layout)

- [ ] **Step 1: Add properties to AppDelegate**

In `glass-slipper/AppDelegate.swift`, after line 17 (`private var spineVC: SpineViewController!`), add:

```swift
    private var statusBar: StatusBarView!
    /// Currently active model name, updated on hw_info and model_swap events.
    private var currentModelName: String = ""
    /// When non-nil, suppress warning banners until this date.
    private var warningDismissedUntil: Date?
    /// When non-nil, suppress promotion banners until this date.
    private var promotionDismissedUntil: Date?
```

- [ ] **Step 2: Insert StatusBarView into the layout**

In `setupUI()`, after the `diagnoseButton` creation block (after line 159 `contentView.addSubview(diagnoseButton)`) and before the `spineVC` setup (line 162), add:

```swift
        // Status bar
        statusBar = StatusBarView()
        contentView.addSubview(statusBar)
```

Then update the constraints section. Replace the spine's top constraint (line 178):

```swift
            spineVC.view.topAnchor.constraint(equalTo: urlField.bottomAnchor, constant: Spacing.lg),
```

With:

```swift
            // Status bar — below URL field
            statusBar.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            statusBar.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            statusBar.topAnchor.constraint(equalTo: urlField.bottomAnchor, constant: Spacing.sm),

            // Spine — below status bar
            spineVC.view.topAnchor.constraint(equalTo: statusBar.bottomAnchor),
```

- [ ] **Step 3: Build and verify**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add glass-slipper/AppDelegate.swift
git commit -m "feat(swift): add StatusBarView to window layout"
```

---

### Task 9: Handle new JSON events in AppDelegate

Wire `handleEvent()` to parse the four new event types (`memory_warning`, `model_swap`, `promotion_available`, `token_rate`) and update the status bar and spine accordingly. Also extract model name from `hw_info` to initialize the status bar.

**Files:**
- Modify: `glass-slipper/AppDelegate.swift:502-559` (handleEvent)

- [ ] **Step 1: Update hw_info handler to set model name on status bar**

The `hw_info` event currently doesn't carry model name (Rust doesn't send it there). For now, set the model name when a diagnosis starts, using the model filename from the launch arguments. A simpler approach: extract the model name from the `--model` argument that `startDiagnosis()` uses.

In the `hw_info` case (around line 504), after the `spineVC.append(...)` call, add:

```swift
            statusBar.setHealthState(.normal)
```

- [ ] **Step 2: Add initial model name from model path**

In `startDiagnosis()` (the method that constructs process arguments), after the model path is resolved and before the process launches, add a call to set the status bar model name. Find where `modelPath` is determined (around lines 415-432) and after it's set, add:

```swift
        // Set status bar model name from filename
        let modelFileName = (modelPath as NSString).lastPathComponent
        // Extract friendly name: "Qwen3.5-9B-Q5_K_M.gguf" → "Qwen 9B"
        let friendlyName: String
        if modelFileName.contains("4B") {
            friendlyName = "Qwen 4B"
        } else if modelFileName.contains("35B") {
            friendlyName = "Qwen 35B"
        } else {
            friendlyName = "Qwen 9B"
        }
        currentModelName = friendlyName
        statusBar.setModelName(friendlyName)
```

- [ ] **Step 3: Add new event handlers in handleEvent()**

In `handleEvent()`, before the `default:` case (around line 556), add:

```swift
        case "token_rate":
            let rate = event["tok_per_sec"] as? Double ?? 0
            statusBar.setTokPerSec(rate)

        case "memory_warning":
            // Suppress if recently dismissed
            if let until = warningDismissedUntil, Date() < until { break }

            let pageoutRate = event["pageout_rate"] as? UInt64
                ?? (event["pageout_rate"] as? Int).map(UInt64.init) ?? 0
            let swapUsedMB = event["swap_used_mb"] as? Double ?? 0
            let tokPerSec = event["tok_per_sec"] as? Double

            statusBar.setHealthState(.warning)
            spineVC.append(.memoryWarning(pageoutRate: pageoutRate, swapUsedMB: swapUsedMB, tokPerSec: tokPerSec))

            // Set delegate on the most recently added view
            if let banner = spineVC.lastAddedView as? WarningBannerView {
                banner.delegate = self
            }

            logAppEvent("warning_shown", details: [
                "pageout_rate": pageoutRate,
                "swap_used_mb": swapUsedMB,
            ])

        case "model_swap":
            let fromModel = event["from_model"] as? String ?? ""
            let toModel = event["to_model"] as? String ?? ""
            let reason = event["reason"] as? String ?? ""

            // Update status bar
            let friendlyTo: String
            if toModel.contains("4B") || toModel.contains("4b") {
                friendlyTo = "Qwen 4B"
            } else if toModel.contains("35B") || toModel.contains("35b") {
                friendlyTo = "Qwen 35B"
            } else {
                friendlyTo = "Qwen 9B"
            }
            currentModelName = friendlyTo
            statusBar.setModelName(friendlyTo)
            statusBar.setHealthState(.critical)

            spineVC.append(.modelSwap(fromModel: fromModel, toModel: toModel, reason: reason))

            // Briefly show red, then back to green
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) { [weak self] in
                self?.statusBar.setHealthState(.normal)
            }

            logAppEvent("model_swap", details: [
                "from": fromModel,
                "to": toModel,
                "reason": reason,
            ])

        case "promotion_available":
            // Suppress if recently dismissed
            if let until = promotionDismissedUntil, Date() < until { break }

            let toModel = event["to_model"] as? String ?? ""
            spineVC.append(.promotionAvailable(toModel: toModel))

            if let banner = spineVC.lastAddedView as? PromotionBannerView {
                banner.delegate = self
            }

            logAppEvent("promotion_shown", details: ["to_model": toModel])
```

- [ ] **Step 4: Add `lastAddedView` property to SpineViewController**

In `glass-slipper/CinderellaScaffold.swift`, in the `SpineViewController` class (around line 839), add a property:

```swift
    /// The last view added to the stack (for setting delegates on banner views).
    private(set) var lastAddedView: NSView?
```

In the `append(_ event:)` method (around line 889), after `stackView.addArrangedSubview(row)`, add:

```swift
        lastAddedView = row
```

- [ ] **Step 5: Build and verify**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: Build will fail because `logAppEvent` and `MemoryBannerDelegate` conformance don't exist yet. That's expected — they're added in the next two tasks.

- [ ] **Step 6: Commit (WIP)**

```bash
git add glass-slipper/AppDelegate.swift glass-slipper/CinderellaScaffold.swift
git commit -m "wip: handle memory_warning, model_swap, promotion_available, token_rate events"
```

---

### Task 10: Implement MemoryBannerDelegate in AppDelegate

AppDelegate conforms to `MemoryBannerDelegate` so it can handle the "Switch" and "Dismiss" button taps from warning and promotion banners.

**Files:**
- Modify: `glass-slipper/AppDelegate.swift` (add extension at end of file)

- [ ] **Step 1: Add MemoryBannerDelegate conformance**

At the end of `glass-slipper/AppDelegate.swift`, before the final closing brace of the click-to-copy extension (or after it), add:

```swift
// MARK: - MemoryBannerDelegate

extension AppDelegate: MemoryBannerDelegate {
    func warningBannerDidRequestSwap() {
        // The Rust backend handles the actual model swap when it receives
        // a Critical event. For a user-initiated swap from the Warning banner,
        // we don't have a mechanism to send commands back to the subprocess yet.
        // Log the intent — the Rust side will swap if pressure continues.
        logAppEvent("warning_swap_requested", details: [:])
        NSLog("Glass Slipper: user requested model swap from warning banner")
    }

    func warningBannerDidDismiss() {
        // Suppress re-warning for 5 minutes
        warningDismissedUntil = Date().addingTimeInterval(5 * 60)
        statusBar.setHealthState(.normal)
        logAppEvent("warning_dismissed", details: [:])
    }

    func promotionBannerDidAccept() {
        // Like warningBannerDidRequestSwap — no stdin command channel yet.
        // Log the intent for future implementation.
        logAppEvent("promotion_accepted", details: [:])
        NSLog("Glass Slipper: user accepted promotion")
    }

    func promotionBannerDidDismiss() {
        // Suppress re-suggestion for 15 minutes
        promotionDismissedUntil = Date().addingTimeInterval(15 * 60)
        logAppEvent("promotion_dismissed", details: [:])
    }
}
```

- [ ] **Step 2: Build and verify**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: Build will still fail because `logAppEvent` doesn't exist yet. Next task.

- [ ] **Step 3: Commit (WIP)**

```bash
git add glass-slipper/AppDelegate.swift
git commit -m "wip: add MemoryBannerDelegate conformance to AppDelegate"
```

---

### Task 11: Structured app-side logging (glass-slipper-app.log)

The spec calls for a structured JSONL log file owned by Swift. It records UI events, user interactions (dismissed warning, accepted promotion), and Swift-side errors.

**Files:**
- Modify: `glass-slipper/AppDelegate.swift` (add `appLogHandle`, `logAppEvent()`, setup/teardown)

- [ ] **Step 1: Add app log file handle property**

In the AppDelegate properties section (around line 25, near `logFileHandle`), add:

```swift
    /// Structured app-side log (glass-slipper-app.log).
    private var appLogHandle: FileHandle?
```

- [ ] **Step 2: Add logAppEvent utility method**

Add this method to the AppDelegate class body (e.g., after the `handleEvent` method):

```swift
    /// Write a structured JSONL entry to the app log.
    private func logAppEvent(_ eventName: String, details: [String: Any]) {
        guard let handle = appLogHandle else { return }
        var entry: [String: Any] = [
            "timestamp": ISO8601DateFormatter().string(from: Date()),
            "event": eventName,
        ]
        for (key, value) in details {
            entry[key] = value
        }
        guard let data = try? JSONSerialization.data(withJSONObject: entry),
              let line = String(data: data, encoding: .utf8) else { return }
        if let lineData = (line + "\n").data(using: .utf8) {
            handle.write(lineData)
            handle.synchronizeFile()
        }
    }
```

- [ ] **Step 3: Open app log in startDiagnosis()**

In `startDiagnosis()`, near where the existing `logFileHandle` is opened (around line 302-306), add after that block:

```swift
        // App-side structured log
        let appLogPath = NSTemporaryDirectory() + "glass-slipper-app.log"
        FileManager.default.createFile(atPath: appLogPath, contents: nil)
        appLogHandle = FileHandle(forWritingAtPath: appLogPath)
        appLogHandle?.truncateFile(atOffset: 0)
        NSLog("Glass Slipper app log: %@", appLogPath)
```

- [ ] **Step 4: Close app log in taskDidTerminate()**

In `taskDidTerminate()` (around line 595-596 where `logFileHandle` is closed), add:

```swift
        appLogHandle?.closeFile()
        appLogHandle = nil
```

- [ ] **Step 5: Build and verify**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug build 2>&1 | tail -5`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 6: Commit**

```bash
git add glass-slipper/AppDelegate.swift
git commit -m "feat(swift): add structured JSONL app-side logging (glass-slipper-app.log)"
```

---

### Task 12: Squash WIP commits and final verification

Clean up the WIP commits from Tasks 9–10 into proper feature commits, then do a full build and visual smoke test.

**Files:**
- No new changes — just verification

- [ ] **Step 1: Full clean build**

Run: `cd /Users/robertkarl/Code/cinderella/glass-slipper && xcodebuild -project GlassSlipper.xcodeproj -scheme GlassSlipper -configuration Debug clean build 2>&1 | tail -10`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 2: Run Rust tests to verify no regressions**

Run: `cd /Users/robertkarl/Code/cinderella && cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 3: Visual smoke test**

Launch the app and verify:
1. Status bar appears between URL field and spine, showing "No model" initially
2. Clicking "Diagnose" updates the status bar to show the model name
3. tok/s updates appear in the status bar as inference runs

For banner testing, you can temporarily add test events in `applicationDidFinishLaunching`:

```swift
// Temporary — remove after visual verification
DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
    self?.spineVC.append(.memoryWarning(pageoutRate: 1500, swapUsedMB: 4200, tokPerSec: 8.2))
    self?.spineVC.append(.modelSwap(fromModel: "Qwen 9B", toModel: "Qwen 4B", reason: "System was thrashing (page-outs: 1500/s)"))
    self?.spineVC.append(.promotionAvailable(toModel: "Qwen 9B"))
}
```

- [ ] **Step 4: Interactive squash of WIP commits**

Squash the two WIP commits from Tasks 9 and 10 into a single clean commit:

```bash
git rebase -i HEAD~4
```

Mark the WIP commits as `fixup` to fold them into the preceding feature commit. The result should be a single commit like:

```
feat(swift): handle memory pressure events and delegate callbacks
```

- [ ] **Step 5: Remove smoke test code if added**

If you added the temporary test events in Step 3, remove them and amend the last commit.

- [ ] **Step 6: Final commit log review**

Run: `git log --oneline -10`

Expected commit sequence (most recent first):
```
feat(swift): handle memory pressure events and delegate callbacks
feat(swift): add structured JSONL app-side logging (glass-slipper-app.log)
feat(swift): wire EventRowFactory to real banner views
feat(swift): add ModelSwapBannerView and PromotionBannerView
feat(swift): add WarningBannerView with action and dismiss buttons
feat(swift): add StatusBarView with health dot, model name, tok/s
feat(protocol): emit token_rate JSON event for Swift status bar
feat(swift): add memoryWarning, modelSwap, promotionAvailable to Event enum
feat(swift): add design tokens for memory pressure banners
```
