//
//  CinderellaScaffold.swift
//  Glass Slipper — AppKit design tokens, row views, spine controller
//
//  Colors use macOS semantic system colors (NSColor.labelColor, etc.)
//  so light/dark mode works automatically. Spacing and typography are
//  defined as tokens below — use those, not raw literals.
//

import AppKit

// MARK: - Hex helper (used by StatusBarView health dots)

extension NSColor {
    convenience init(hex: UInt32, alpha: CGFloat = 1) {
        let r = CGFloat((hex >> 16) & 0xFF) / 255
        let g = CGFloat((hex >>  8) & 0xFF) / 255
        let b = CGFloat( hex        & 0xFF) / 255
        self.init(srgbRed: r, green: g, blue: b, alpha: alpha)
    }
}

// MARK: - Color tokens
//
// Uses macOS semantic system colors so everything adapts to
// light/dark mode automatically. No hex values, no custom providers.

extension NSColor {
    // Surfaces
    static let surfacePrimary    = NSColor.windowBackgroundColor
    static let surfaceMuted      = NSColor.controlBackgroundColor
    static let surfaceHeader     = NSColor.underPageBackgroundColor
    static let surfaceDiagnosis  = NSColor.controlBackgroundColor
    static let surfaceDiagWarn   = NSColor.controlBackgroundColor
    static let surfaceDiagFail   = NSColor.controlBackgroundColor

    // Text
    static let textPrimary       = NSColor.labelColor
    static let textSecondary     = NSColor.secondaryLabelColor
    static let textQuiet         = NSColor.tertiaryLabelColor

    // Lines & accents
    static let separatorHairline = NSColor.separatorColor
    static let accentDiagnosis   = NSColor.systemGreen
    static let accentDiagLabel   = NSColor.systemGreen
    static let accentDiagWarn    = NSColor.systemYellow
    static let accentDiagWarnLbl = NSColor.systemOrange
    static let accentDiagFail    = NSColor.systemRed
    static let accentDiagFailLbl = NSColor.systemRed
    static let accentProgress    = NSColor.systemBlue

    // Memory pressure banners
    static let surfaceWarningBanner  = NSColor.controlBackgroundColor
    static let accentWarningBanner   = NSColor.systemYellow
    static let textWarningBanner     = NSColor.labelColor
    static let surfaceCriticalBanner = NSColor.controlBackgroundColor
    static let accentCriticalBanner  = NSColor.systemRed
    static let textCriticalBanner    = NSColor.labelColor
    static let surfacePromotionBanner = NSColor.controlBackgroundColor
    static let accentPromotionBanner  = NSColor.systemGreen
    static let textPromotionBanner    = NSColor.labelColor

    // Status pill — backgrounds
    static let statusOKBg        = NSColor.systemGreen.withAlphaComponent(0.15)
    static let statusERRBg       = NSColor.systemRed.withAlphaComponent(0.15)
    static let statusWARNBg      = NSColor.systemYellow.withAlphaComponent(0.15)
    static let statusINFOBg      = NSColor.systemBlue.withAlphaComponent(0.15)

    // Status pill — text
    static let statusOKFg        = NSColor.systemGreen
    static let statusERRFg       = NSColor.systemRed
    static let statusWARNFg      = NSColor.systemOrange
    static let statusINFOFg      = NSColor.systemBlue

    // MCP Companion — savings
    static let savingsGreen       = NSColor.systemGreen
    static let savingsGreenMuted  = NSColor.systemGreen.withAlphaComponent(0.2)
    static let companionBlue      = NSColor.systemBlue
    static let companionPurple    = NSColor.systemPurple
    static let setupStepBg        = NSColor.controlBackgroundColor
    static let setupCheckmark     = NSColor.systemGreen
    static let setupActionBg      = NSColor.systemBlue
    static let setupActionFg      = NSColor.white
}

// MARK: - Typography tokens

extension NSFont {
    static var cardTitle:      NSFont { .systemFont(ofSize: 15, weight: .semibold) }
    static var detailText:     NSFont { .systemFont(ofSize: 13, weight: .regular) }
    static var sectionHeader:  NSFont { .systemFont(ofSize: 11, weight: .semibold) }
    static var diagnosisLabel: NSFont { .systemFont(ofSize: 11, weight: .bold) }
    static var diagnosisText:  NSFont { .systemFont(ofSize: 15, weight: .regular) }
    static var stampLabel:     NSFont { .systemFont(ofSize: 10, weight: .bold) }
    static var bannerBody:     NSFont { .systemFont(ofSize: 12, weight: .medium) }
    static var promptInput:    NSFont { .systemFont(ofSize: 14, weight: .regular) }

    /// Italic 13pt for thought rows. Falls back to regular if the
    /// italic descriptor can't be resolved (rare on system fonts).
    static var detailItalic: NSFont {
        let base = NSFont.systemFont(ofSize: 13, weight: .regular)
        let descriptor = base.fontDescriptor.withSymbolicTraits(.italic)
        return NSFont(descriptor: descriptor, size: 13) ?? base
    }
}

// MARK: - Spacing tokens

enum Spacing {
    static let xs:  CGFloat = 2
    static let sm:  CGFloat = 4
    static let md:  CGFloat = 8
    static let lg:  CGFloat = 12
    static let xl:  CGFloat = 16
    static let xxl: CGFloat = 24

    /// Composite tokens — most rows should use these, not raw values.
    static let rowHorizontal: CGFloat = 24      // px-6 in the mock
    static let rowVertical:   CGFloat = 16      // py-4 in the mock
    static let pillPaddingX:  CGFloat = 10
    static let pillHeight:    CGFloat = 18
    static let pillRadius:    CGFloat = 9       // pillHeight / 2 → fully rounded
    static let diagBorderW:   CGFloat = 4       // emerald left border on diagnosis
}

// MARK: - Event model

enum EventStatus {
    case ok, err, warn, info

    var label: String {
        switch self {
        case .ok:   return "OK"
        case .err:  return "ERR"
        case .warn: return "WARN"
        case .info: return "INFO"
        }
    }
    var background: NSColor {
        switch self {
        case .ok:   return .statusOKBg
        case .err:  return .statusERRBg
        case .warn: return .statusWARNBg
        case .info: return .statusINFOBg
        }
    }
    var foreground: NSColor {
        switch self {
        case .ok:   return .statusOKFg
        case .err:  return .statusERRFg
        case .warn: return .statusWARNFg
        case .info: return .statusINFOFg
        }
    }
}

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

// MARK: - StatusPillView (reusable token-composed view)

final class StatusPillView: NSView {
    private let label = NSTextField(labelWithString: "")

    init(status: EventStatus) {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = Spacing.pillRadius
        layer?.cornerCurve = .continuous

        translatesAutoresizingMaskIntoConstraints = false
        label.translatesAutoresizingMaskIntoConstraints = false
        label.alignment = .center
        addSubview(label)

        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.pillPaddingX),
            label.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.pillPaddingX),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            heightAnchor.constraint(equalToConstant: Spacing.pillHeight),
        ])

        configure(status: status)
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }

    func configure(status: EventStatus) {
        layer?.backgroundColor = status.background.cgColor

        let attr = NSAttributedString(string: status.label, attributes: [
            .font: NSFont.stampLabel,
            .foregroundColor: status.foreground,
            .kern: 0.5,                      // tracked-wider in the mock
        ])
        label.attributedStringValue = attr
    }
}

// MARK: - Hairline divider

final class HairlineDivider: NSView {
    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = NSColor.separatorHairline.cgColor
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 1).isActive = true
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

// MARK: - Appearance-aware background view

/// NSView subclass that re-applies its background color when the system
/// appearance changes (light ↔ dark). Needed because layer.backgroundColor
/// is a CGColor snapshot that doesn't auto-update with NSColor.
class AppearanceAwareView: NSView {
    private var dynamicBgColor: NSColor?

    override var wantsUpdateLayer: Bool { true }

    func setDynamicBackground(_ color: NSColor) {
        dynamicBgColor = color
        wantsLayer = true
        needsDisplay = true
    }

    override func updateLayer() {
        if let color = dynamicBgColor {
            layer?.backgroundColor = color.cgColor
        }
    }
}

// MARK: - Flipped document view (top-aligned scroll content)

final class FlippedView: NSView {
    override var isFlipped: Bool { true }
}

// MARK: - Row factory

enum EventRowFactory {
    static func makeRow(for event: Event) -> NSView {
        switch event {
        case .userPrompt(let text):
            return UserPromptRowView(text: text)
        case .plan(let items):
            return PlanRowView(items: items)
        case .check(let name, let status, let detail):
            return CheckRowView(name: name, status: status, detail: detail)
        case .thought(let text):
            return ThoughtRowView(text: text)
        case .diagnosis(let text, let status):
            return DiagnosisRowView(text: text, status: status)
        case .hwInfo(let chip, let ramUsed, let ramTotal, let gpu):
            return HwInfoRowView(chip: chip, ramUsed: ramUsed, ramTotal: ramTotal, gpu: gpu)
        case .connecting:
            return ConnectingRowView()
        case .modelDownload:
            return ModelDownloadRowView()
        case .memoryWarning(let pageoutRate, let swapUsedMB, let tokPerSec):
            return WarningBannerView(pageoutRate: pageoutRate, swapUsedMB: swapUsedMB, tokPerSec: tokPerSec, switchToModel: "smaller model")
        case .modelSwap(let fromModel, let toModel, let reason):
            return ModelSwapBannerView(fromModel: fromModel, toModel: toModel, reason: reason)
        case .promotionAvailable(let toModel):
            return PromotionBannerView(toModel: toModel)
        }
    }
}

// MARK: - CheckRowView

final class CheckRowView: NSView {
    private let pill: StatusPillView
    private let titleLabel = NSTextField(labelWithString: "")
    private let detailLabel = NSTextField(wrappingLabelWithString: "")

    init(name: String, status: EventStatus, detail: String) {
        self.pill = StatusPillView(status: status)
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfacePrimary.cgColor

        translatesAutoresizingMaskIntoConstraints = false
        pill.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        detailLabel.translatesAutoresizingMaskIntoConstraints = false

        titleLabel.font = .cardTitle
        titleLabel.textColor = .textPrimary
        titleLabel.stringValue = name
        titleLabel.maximumNumberOfLines = 1
        titleLabel.lineBreakMode = .byTruncatingTail

        detailLabel.font = .detailText
        detailLabel.textColor = .textSecondary
        detailLabel.stringValue = detail
        detailLabel.maximumNumberOfLines = 0

        addSubview(pill)
        addSubview(titleLabel)
        addSubview(detailLabel)

        NSLayoutConstraint.activate([
            // Pill — top-left, vertically nudged to align with title baseline
            pill.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.rowHorizontal),
            pill.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.rowVertical + 2),

            // Title — to the right of pill
            titleLabel.leadingAnchor.constraint(equalTo: pill.trailingAnchor, constant: Spacing.lg),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            titleLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.rowVertical),

            // Detail — below title, wraps to row trailing edge
            detailLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            detailLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            detailLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: Spacing.xs),
            detailLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.rowVertical),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

// MARK: - Row views — follow the CheckRowView pattern
//
// Use system colors and spacing tokens. No raw literals.

final class UserPromptRowView: NSView {
    private let glyphLabel = NSTextField(labelWithString: "→")
    private let promptLabel = NSTextField(labelWithString: "")
    private let investigateButton = NSButton(title: "Investigate", target: nil, action: nil)
    private let hairline = HairlineDivider()

    init(text: String) {
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfaceHeader.cgColor

        translatesAutoresizingMaskIntoConstraints = false
        glyphLabel.translatesAutoresizingMaskIntoConstraints = false
        promptLabel.translatesAutoresizingMaskIntoConstraints = false
        investigateButton.translatesAutoresizingMaskIntoConstraints = false
        hairline.translatesAutoresizingMaskIntoConstraints = false

        glyphLabel.font = .promptInput
        glyphLabel.textColor = .textQuiet
        glyphLabel.setContentHuggingPriority(.required, for: .horizontal)
        glyphLabel.setContentCompressionResistancePriority(.required, for: .horizontal)

        promptLabel.font = .promptInput
        promptLabel.textColor = .textPrimary
        promptLabel.stringValue = text
        promptLabel.maximumNumberOfLines = 1
        promptLabel.lineBreakMode = .byTruncatingTail
        promptLabel.isEditable = false
        promptLabel.isSelectable = true
        promptLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        investigateButton.bezelStyle = .rounded
        investigateButton.controlSize = .small
        investigateButton.font = .detailText
        investigateButton.setContentHuggingPriority(.required, for: .horizontal)
        investigateButton.setContentCompressionResistancePriority(.required, for: .horizontal)

        addSubview(glyphLabel)
        addSubview(promptLabel)
        addSubview(investigateButton)
        addSubview(hairline)

        NSLayoutConstraint.activate([
            // Glyph — leading edge, vertically centered
            glyphLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.rowHorizontal),
            glyphLabel.centerYAnchor.constraint(equalTo: centerYAnchor),

            // Prompt text — after glyph, fills middle
            promptLabel.leadingAnchor.constraint(equalTo: glyphLabel.trailingAnchor, constant: Spacing.md),
            promptLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            promptLabel.trailingAnchor.constraint(lessThanOrEqualTo: investigateButton.leadingAnchor, constant: -Spacing.lg),

            // Investigate button — trailing edge, vertically centered
            investigateButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            investigateButton.centerYAnchor.constraint(equalTo: centerYAnchor),

            // Row height from vertical padding
            promptLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.rowVertical),
            promptLabel.bottomAnchor.constraint(equalTo: hairline.topAnchor, constant: -Spacing.rowVertical),

            // Hairline at bottom
            hairline.leadingAnchor.constraint(equalTo: leadingAnchor),
            hairline.trailingAnchor.constraint(equalTo: trailingAnchor),
            hairline.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

final class PlanRowView: NSView {
    private let headerLabel = NSTextField(labelWithString: "")
    private let itemsStack = NSStackView()

    init(items: [String]) {
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfacePrimary.cgColor

        translatesAutoresizingMaskIntoConstraints = false
        headerLabel.translatesAutoresizingMaskIntoConstraints = false
        itemsStack.translatesAutoresizingMaskIntoConstraints = false

        // "PLAN" header — small uppercase, kerned
        let headerAttr = NSAttributedString(string: "PLAN", attributes: [
            .font: NSFont.sectionHeader,
            .foregroundColor: NSColor.textSecondary,
            .kern: 1.0,
        ])
        headerLabel.attributedStringValue = headerAttr

        // Items stack — vertical list of bulleted items
        itemsStack.orientation = .vertical
        itemsStack.alignment = .leading
        itemsStack.spacing = Spacing.sm

        for item in items {
            let row = NSStackView()
            row.orientation = .horizontal
            row.alignment = .firstBaseline
            row.spacing = Spacing.md
            row.translatesAutoresizingMaskIntoConstraints = false

            // Bullet: 1.5pt circle via attributed string
            let bullet = NSTextField(labelWithString: "")
            bullet.translatesAutoresizingMaskIntoConstraints = false
            let bulletAttr = NSAttributedString(string: "\u{2022}", attributes: [
                .font: NSFont.detailText,
                .foregroundColor: NSColor.textQuiet,
            ])
            bullet.attributedStringValue = bulletAttr
            bullet.setContentHuggingPriority(.required, for: .horizontal)

            let label = NSTextField(labelWithString: item)
            label.translatesAutoresizingMaskIntoConstraints = false
            label.font = .detailText
            label.textColor = .textPrimary
            label.maximumNumberOfLines = 1
            label.lineBreakMode = .byTruncatingTail

            row.addArrangedSubview(bullet)
            row.addArrangedSubview(label)
            itemsStack.addArrangedSubview(row)
        }

        addSubview(headerLabel)
        addSubview(itemsStack)

        NSLayoutConstraint.activate([
            headerLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.rowHorizontal),
            headerLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.rowVertical),

            itemsStack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.rowHorizontal),
            itemsStack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            itemsStack.topAnchor.constraint(equalTo: headerLabel.bottomAnchor, constant: Spacing.md),
            itemsStack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.rowVertical),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

final class ThoughtRowView: NSView {
    private let prefixLabel = NSTextField(labelWithString: "")
    private let bodyLabel = NSTextField(wrappingLabelWithString: "")

    init(text: String) {
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfaceMuted.cgColor

        translatesAutoresizingMaskIntoConstraints = false
        prefixLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false

        // "..." prefix in quiet color
        prefixLabel.font = .detailText
        prefixLabel.textColor = .textQuiet
        prefixLabel.stringValue = "\u{2026}"
        prefixLabel.setContentHuggingPriority(.required, for: .horizontal)
        prefixLabel.setContentCompressionResistancePriority(.required, for: .horizontal)

        // Italic body in secondary color
        bodyLabel.font = .detailItalic
        bodyLabel.textColor = .textSecondary
        bodyLabel.stringValue = text
        bodyLabel.maximumNumberOfLines = 0

        addSubview(prefixLabel)
        addSubview(bodyLabel)

        NSLayoutConstraint.activate([
            prefixLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.rowHorizontal),
            prefixLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.lg),

            bodyLabel.leadingAnchor.constraint(equalTo: prefixLabel.trailingAnchor, constant: Spacing.sm),
            bodyLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            bodyLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.lg),
            bodyLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.lg),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

final class DiagnosisRowView: NSView {
    private let borderView = NSView()
    private let headerLabel = NSTextField(labelWithString: "")
    private let bodyLabel = NSTextField(wrappingLabelWithString: "")

    init(text: String, status: EventStatus = .ok) {
        super.init(frame: .zero)

        // Pick colors based on status
        let surfaceColor: NSColor
        let borderColor: NSColor
        let labelColor: NSColor
        switch status {
        case .ok:
            surfaceColor = .surfaceDiagnosis
            borderColor = .accentDiagnosis
            labelColor = .accentDiagLabel
        case .warn:
            surfaceColor = .surfaceDiagWarn
            borderColor = .accentDiagWarn
            labelColor = .accentDiagWarnLbl
        case .err:
            surfaceColor = .surfaceDiagFail
            borderColor = .accentDiagFail
            labelColor = .accentDiagFailLbl
        case .info:
            surfaceColor = .surfaceDiagnosis
            borderColor = .accentDiagnosis
            labelColor = .accentDiagLabel
        }

        wantsLayer = true
        layer?.backgroundColor = surfaceColor.cgColor

        translatesAutoresizingMaskIntoConstraints = false
        borderView.translatesAutoresizingMaskIntoConstraints = false
        headerLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false

        // 4pt left border
        borderView.wantsLayer = true
        borderView.layer?.backgroundColor = borderColor.cgColor

        // "DIAGNOSIS" header — bold uppercase, kerned
        let headerAttr = NSAttributedString(string: "DIAGNOSIS", attributes: [
            .font: NSFont.diagnosisLabel,
            .foregroundColor: labelColor,
            .kern: 1.0,
        ])
        headerLabel.attributedStringValue = headerAttr

        // Body text
        bodyLabel.font = .diagnosisText
        bodyLabel.textColor = .textPrimary
        bodyLabel.stringValue = text
        bodyLabel.maximumNumberOfLines = 0

        addSubview(borderView)
        addSubview(headerLabel)
        addSubview(bodyLabel)

        NSLayoutConstraint.activate([
            // Left border — full height, 4pt wide
            borderView.leadingAnchor.constraint(equalTo: leadingAnchor),
            borderView.topAnchor.constraint(equalTo: topAnchor),
            borderView.bottomAnchor.constraint(equalTo: bottomAnchor),
            borderView.widthAnchor.constraint(equalToConstant: Spacing.diagBorderW),

            // Header — after border with generous padding
            headerLabel.leadingAnchor.constraint(equalTo: borderView.trailingAnchor, constant: Spacing.rowHorizontal),
            headerLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.xxl),

            // Body — below header
            bodyLabel.leadingAnchor.constraint(equalTo: headerLabel.leadingAnchor),
            bodyLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            bodyLabel.topAnchor.constraint(equalTo: headerLabel.bottomAnchor, constant: Spacing.md),
            bodyLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.xxl),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

// MARK: - HwInfoRowView

final class HwInfoRowView: NSView {
    init(chip: String, ramUsed: Double, ramTotal: Double, gpu: String) {
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfaceHeader.cgColor

        translatesAutoresizingMaskIntoConstraints = false

        let pill = StatusPillView(status: .info)
        pill.translatesAutoresizingMaskIntoConstraints = false

        let chipLabel = NSTextField(labelWithString: chip)
        chipLabel.translatesAutoresizingMaskIntoConstraints = false
        chipLabel.font = .cardTitle
        chipLabel.textColor = .textPrimary
        chipLabel.maximumNumberOfLines = 1

        let detail = String(format: "RAM: %.1f / %.0f GB · GPU: %@", ramUsed, ramTotal, gpu)
        let detailLabel = NSTextField(labelWithString: detail)
        detailLabel.translatesAutoresizingMaskIntoConstraints = false
        detailLabel.font = .detailText
        detailLabel.textColor = .textSecondary

        addSubview(pill)
        addSubview(chipLabel)
        addSubview(detailLabel)

        NSLayoutConstraint.activate([
            pill.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.rowHorizontal),
            pill.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.rowVertical + 2),

            chipLabel.leadingAnchor.constraint(equalTo: pill.trailingAnchor, constant: Spacing.lg),
            chipLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            chipLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.rowVertical),

            detailLabel.leadingAnchor.constraint(equalTo: chipLabel.leadingAnchor),
            detailLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            detailLabel.topAnchor.constraint(equalTo: chipLabel.bottomAnchor, constant: Spacing.xs),
            detailLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.rowVertical),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

// MARK: - ConnectingRowView

final class ConnectingRowView: NSView {
    init() {
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfacePrimary.cgColor

        translatesAutoresizingMaskIntoConstraints = false

        let spinner = NSProgressIndicator()
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.translatesAutoresizingMaskIntoConstraints = false
        spinner.startAnimation(nil)

        let label = NSTextField(labelWithString: "Connecting…")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = .cardTitle
        label.textColor = .textSecondary

        addSubview(spinner)
        addSubview(label)

        NSLayoutConstraint.activate([
            spinner.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.rowHorizontal),
            spinner.centerYAnchor.constraint(equalTo: centerYAnchor),
            spinner.widthAnchor.constraint(equalToConstant: 18),
            spinner.heightAnchor.constraint(equalToConstant: 18),

            label.leadingAnchor.constraint(equalTo: spinner.trailingAnchor, constant: Spacing.lg),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),

            // Fixed height
            heightAnchor.constraint(equalToConstant: 48),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

// MARK: - ModelDownloadRowView

final class ModelDownloadRowView: NSView {

    private let titleLabel = NSTextField(labelWithString: "")
    private let detailLabel = NSTextField(labelWithString: "")
    private let progressBar = NSProgressIndicator()
    private let progressLabel = NSTextField(labelWithString: "")
    private let actionButton = NSButton(title: "Download", target: nil, action: nil)
    private let errorLabel = NSTextField(wrappingLabelWithString: "")

    var onAction: (() -> Void)?

    init() {
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.surfacePrimary.cgColor
        translatesAutoresizingMaskIntoConstraints = false

        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        detailLabel.translatesAutoresizingMaskIntoConstraints = false
        progressBar.translatesAutoresizingMaskIntoConstraints = false
        progressLabel.translatesAutoresizingMaskIntoConstraints = false
        actionButton.translatesAutoresizingMaskIntoConstraints = false
        errorLabel.translatesAutoresizingMaskIntoConstraints = false

        titleLabel.font = .cardTitle
        titleLabel.textColor = .textPrimary

        detailLabel.font = .detailText
        detailLabel.textColor = .textSecondary

        progressBar.style = .bar
        progressBar.isIndeterminate = false
        progressBar.minValue = 0
        progressBar.maxValue = 1
        progressBar.isHidden = true

        progressLabel.font = .monospacedDigitSystemFont(ofSize: 13, weight: .regular)
        progressLabel.textColor = .textSecondary
        progressLabel.isHidden = true

        actionButton.bezelStyle = .rounded
        actionButton.controlSize = .regular
        actionButton.target = self
        actionButton.action = #selector(actionClicked)

        errorLabel.font = .detailText
        errorLabel.textColor = .statusERRFg
        errorLabel.maximumNumberOfLines = 0
        errorLabel.isHidden = true

        addSubview(titleLabel)
        addSubview(detailLabel)
        addSubview(progressBar)
        addSubview(progressLabel)
        addSubview(actionButton)
        addSubview(errorLabel)

        NSLayoutConstraint.activate([
            titleLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.rowHorizontal),
            titleLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.rowVertical),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Spacing.rowHorizontal),

            detailLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            detailLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: Spacing.xs),
            detailLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Spacing.rowHorizontal),

            progressBar.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            progressBar.topAnchor.constraint(equalTo: detailLabel.bottomAnchor, constant: Spacing.md),
            progressBar.trailingAnchor.constraint(equalTo: progressLabel.leadingAnchor, constant: -Spacing.md),

            progressLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),
            progressLabel.centerYAnchor.constraint(equalTo: progressBar.centerYAnchor),
            progressLabel.widthAnchor.constraint(greaterThanOrEqualToConstant: 120),

            errorLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            errorLabel.topAnchor.constraint(equalTo: detailLabel.bottomAnchor, constant: Spacing.md),
            errorLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.rowHorizontal),

            actionButton.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            actionButton.topAnchor.constraint(equalTo: progressBar.bottomAnchor, constant: Spacing.md),
            actionButton.topAnchor.constraint(greaterThanOrEqualTo: errorLabel.bottomAnchor, constant: Spacing.md),
            actionButton.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.rowVertical),
            actionButton.widthAnchor.constraint(greaterThanOrEqualToConstant: 100),
        ])
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }

    @objc private func actionClicked() {
        onAction?()
    }

    /// Show the initial "model needed" state.
    func showMissing(name: String, sizeGB: String) {
        titleLabel.stringValue = "Model Required"
        detailLabel.stringValue = "\(name) (\(sizeGB))"
        progressBar.isHidden = true
        progressLabel.isHidden = true
        errorLabel.isHidden = true
        actionButton.title = "Download"
        actionButton.isHidden = false
    }

    /// Show download progress.
    func showProgress(downloaded: Int64, total: Int64) {
        let pct = total > 0 ? Double(downloaded) / Double(total) : 0
        let dlGB = String(format: "%.1f", Double(downloaded) / 1_073_741_824)
        let totalGB = String(format: "%.1f", Double(total) / 1_073_741_824)

        titleLabel.stringValue = "Downloading Model"
        detailLabel.isHidden = true
        progressBar.isHidden = false
        progressBar.doubleValue = pct
        progressLabel.isHidden = false
        progressLabel.stringValue = "\(dlGB) / \(totalGB) GB (\(Int(pct * 100))%)"
        errorLabel.isHidden = true
        actionButton.isHidden = true
    }

    /// Show verification in progress.
    func showVerifying() {
        titleLabel.stringValue = "Verifying integrity\u{2026}"
        detailLabel.isHidden = true
        progressBar.isHidden = false
        progressBar.isIndeterminate = true
        progressBar.startAnimation(nil)
        progressLabel.isHidden = true
        errorLabel.isHidden = true
        actionButton.isHidden = true
    }

    /// Show verified / ready.
    func showReady() {
        titleLabel.stringValue = "Model Ready"
        detailLabel.stringValue = ""
        detailLabel.isHidden = true
        progressBar.isHidden = true
        progressLabel.isHidden = true
        errorLabel.isHidden = true
        actionButton.isHidden = true
    }

    /// Show an error with retry.
    func showError(_ message: String) {
        titleLabel.stringValue = "Download Failed"
        detailLabel.isHidden = true
        progressBar.isHidden = true
        progressLabel.isHidden = true
        errorLabel.stringValue = message
        errorLabel.isHidden = false
        actionButton.title = "Retry"
        actionButton.isHidden = false
    }
}

// MARK: - Spine view controller

final class SpineViewController: NSViewController {

    private let scrollView = NSScrollView()
    private let stackView  = NSStackView()
    private(set) var events: [Event] = []
    /// Copy text associated with each row view (used by click-to-copy).
    var copyTextByView: [ObjectIdentifier: String] = [:]
    /// The last view added to the stack (for setting delegates on banner views).
    private(set) var lastAddedView: NSView?

    override func loadView() {
        let root = AppearanceAwareView(frame: NSRect(x: 0, y: 0, width: 720, height: 720))
        root.setDynamicBackground(.surfacePrimary)
        view = root

        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.hasVerticalScroller = true
        scrollView.drawsBackground = false
        scrollView.automaticallyAdjustsContentInsets = false

        stackView.orientation = .vertical
        stackView.alignment   = .leading
        stackView.distribution = .fill
        stackView.spacing      = 0
        stackView.translatesAutoresizingMaskIntoConstraints = false

        let documentView = FlippedView()
        documentView.translatesAutoresizingMaskIntoConstraints = false
        documentView.addSubview(stackView)
        scrollView.documentView = documentView

        root.addSubview(scrollView)

        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: root.topAnchor),
            scrollView.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: root.bottomAnchor),

            documentView.leadingAnchor.constraint(equalTo: scrollView.contentView.leadingAnchor),
            documentView.trailingAnchor.constraint(equalTo: scrollView.contentView.trailingAnchor),
            documentView.widthAnchor.constraint(equalTo: scrollView.contentView.widthAnchor),

            stackView.topAnchor.constraint(equalTo: documentView.topAnchor),
            stackView.leadingAnchor.constraint(equalTo: documentView.leadingAnchor),
            stackView.trailingAnchor.constraint(equalTo: documentView.trailingAnchor),
            stackView.bottomAnchor.constraint(equalTo: documentView.bottomAnchor),
        ])
    }

    /// Append a new event from the glass-slipper JSON stream. Main thread only.
    func append(_ event: Event) {
        let needsDivider = !events.isEmpty && shouldShowDividerAbove(event)
        let copyText = Self.copyableText(for: event)
        events.append(event)

        if needsDivider {
            stackView.addArrangedSubview(HairlineDivider())
        }
        let row = EventRowFactory.makeRow(for: event)
        row.translatesAutoresizingMaskIntoConstraints = false
        stackView.addArrangedSubview(row)
        lastAddedView = row
        row.widthAnchor.constraint(equalTo: stackView.widthAnchor).isActive = true

        // Click-to-copy
        if !copyText.isEmpty {
            addClickToCopy(to: row, text: copyText)
        }

        scrollToBottom()
    }

    /// Extract copyable text from an event.
    private static func copyableText(for event: Event) -> String {
        switch event {
        case .userPrompt(let text): return text
        case .plan(let items): return items.joined(separator: "\n")
        case .check(_, _, let detail): return detail
        case .thought(let text): return text
        case .diagnosis(let text, _): return text
        case .hwInfo(let chip, let ramUsed, let ramTotal, let gpu):
            return String(format: "%@ · RAM: %.1f/%.0f GB · GPU: %@", chip, ramUsed, ramTotal, gpu)
        case .connecting: return ""
        case .modelDownload: return ""
        case .memoryWarning(let pageoutRate, let swapUsedMB, let tokPerSec):
            let rate = tokPerSec.map { String(format: "%.1f tok/s", $0) } ?? "—"
            return "Memory warning: page-outs \(pageoutRate)/s, swap \(String(format: "%.0f", swapUsedMB)) MB, \(rate)"
        case .modelSwap(let fromModel, let toModel, let reason):
            return "Switched from \(fromModel) to \(toModel): \(reason)"
        case .promotionAvailable(let toModel):
            return "Promotion available: switch to \(toModel)"
        }
    }

    private func scrollToBottom() {
        DispatchQueue.main.async { [weak self] in
            guard let self,
                  let doc = self.scrollView.documentView else { return }
            let y = max(0, doc.frame.height - self.scrollView.contentView.bounds.height)
            self.scrollView.contentView.scroll(to: NSPoint(x: 0, y: y))
            self.scrollView.reflectScrolledClipView(self.scrollView.contentView)
        }
    }

    /// Diagnosis and the user-prompt header have their own visual
    /// separation; don't add a hairline above them.
    private func shouldShowDividerAbove(_ event: Event) -> Bool {
        switch event {
        case .diagnosis, .userPrompt, .hwInfo, .connecting, .modelDownload,
             .memoryWarning, .modelSwap, .promotionAvailable: return false
        default: return true
        }
    }
}

// MARK: - Smoke test
//
// Add this to your AppDelegate's applicationDidFinishLaunching to verify
// the scaffold renders before wiring up the glass-slipper subprocess.
//
//     let vc = SpineViewController()
//     window.contentViewController = vc
//     [
//         .userPrompt(text: "api.example.com/v1/users — intermittent failures"),
//         .plan(items: ["DNS resolution", "Ping connectivity", "HTTP probe", "Sample failure rate"]),
//         .check(name: "DNS",   status: .ok,   detail: "resolves to 52.84.121.4 — 12ms"),
//         .check(name: "Ping",  status: .ok,   detail: "14ms avg · 0% loss · 10/10 packets"),
//         .check(name: "HTTP probe", status: .warn, detail: "503 Service Unavailable on first request"),
//         .thought(text: "503 looks intermittent — sampling to estimate rate"),
//         .check(name: "Sample (n=50)", status: .err, detail: "14/50 returned 503 · 28% failure"),
//         .diagnosis(text: "Service is up but unstable. ~28% intermittent 503s. Likely overloaded backend or rolling deployment."),
//     ].forEach { vc.append($0) }
//
// CheckRowView will render correctly. The other four will render as
// empty boxes until you implement them. That's by design.
