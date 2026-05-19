//
//  CinderellaScaffold.swift
//  Glass Slipper — AppKit starter
//
//  PURPOSE
//  -------
//  Design-system scaffold + one fully worked row view (CheckRowView) as a
//  pattern reference. Other row kinds (UserPromptRowView, PlanRowView,
//  ThoughtRowView, DiagnosisRowView, and future WaterfallRowView /
//  MiniChartRowView) are stubbed. Implement them by COMPOSING TOKENS, not
//  by inventing new visual decisions.
//
//  THE ONE RULE
//  ------------
//  If you find yourself reaching for a literal value — a hex color, a font
//  size, a padding number — STOP and add it to the appropriate token
//  section first. Then use the token. This is the only thing keeping the
//  per-view drift that fucked up the last port from happening again.
//
//  WIRING
//  ------
//  - Spine is NSScrollView wrapping NSStackView. ~20 events: stack view
//    is fine. If history grows past a few hundred events later, swap to
//    NSTableView with cell reuse. Don't preoptimize.
//  - Append events from your glass-slipper JSON stream by calling
//    SpineViewController.append(_:) on the main thread.
//
//  DEV LOOP
//  --------
//  Add InjectionIII (https://github.com/johnno1962/InjectionForXcode) so
//  edits to row views hot-reload without rebuild. Without it the loop
//  with Claude Code is too slow to iterate on visual details.
//

import AppKit

// MARK: - Hex helper

extension NSColor {
    /// Init from 0xRRGGBB literal. Keeps the token table readable.
    convenience init(hex: UInt32, alpha: CGFloat = 1) {
        let r = CGFloat((hex >> 16) & 0xFF) / 255
        let g = CGFloat((hex >>  8) & 0xFF) / 255
        let b = CGFloat( hex        & 0xFF) / 255
        self.init(srgbRed: r, green: g, blue: b, alpha: alpha)
    }
}

// MARK: - Color tokens
//
// Mapped 1:1 from the React mock's Tailwind palette. Uses NSColor(name:)
// dynamic providers so colors adapt to system light/dark appearance.

extension NSColor {
    /// Helper: create a dynamic color that resolves per-appearance.
    private static func dynamic(light: UInt32, dark: UInt32) -> NSColor {
        NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            return isDark ? NSColor(hex: dark) : NSColor(hex: light)
        }
    }

    // Surfaces
    static let surfacePrimary    = dynamic(light: 0xFFFFFF, dark: 0x1C1C1E)
    static let surfaceMuted      = dynamic(light: 0xFAFAFA, dark: 0x2C2C2E)
    static let surfaceHeader     = dynamic(light: 0xF4F4F5, dark: 0x3A3A3C)
    static let surfaceDiagnosis  = dynamic(light: 0xECFDF5, dark: 0x0D2818)
    static let surfaceDiagWarn   = dynamic(light: 0xFFFBEB, dark: 0x2D1F00)
    static let surfaceDiagFail   = dynamic(light: 0xFEF2F2, dark: 0x2D0F0F)

    // Text
    static let textPrimary       = dynamic(light: 0x18181B, dark: 0xF4F4F5)
    static let textSecondary     = dynamic(light: 0x71717A, dark: 0xA1A1AA)
    static let textQuiet         = dynamic(light: 0xA1A1AA, dark: 0x71717A)

    // Lines & accents
    static let separatorHairline = dynamic(light: 0xF4F4F5, dark: 0x3A3A3C)
    static let accentDiagnosis   = dynamic(light: 0x10B981, dark: 0x34D399)
    static let accentDiagLabel   = dynamic(light: 0x047857, dark: 0x6EE7B7)
    static let accentDiagWarn    = dynamic(light: 0xF59E0B, dark: 0xFBBF24)
    static let accentDiagWarnLbl = dynamic(light: 0x92400E, dark: 0xFDE68A)
    static let accentDiagFail    = dynamic(light: 0xEF4444, dark: 0xF87171)
    static let accentDiagFailLbl = dynamic(light: 0xB91C1C, dark: 0xFCA5A5)
    static let accentProgress    = dynamic(light: 0x3B82F6, dark: 0x60A5FA)

    // Memory pressure banners
    static let surfaceWarningBanner  = dynamic(light: 0xFEFCE8, dark: 0x2D2800)
    static let accentWarningBanner   = dynamic(light: 0xEAB308, dark: 0xFACC15)
    static let textWarningBanner     = dynamic(light: 0x713F12, dark: 0xFEF08A)
    static let surfaceCriticalBanner = dynamic(light: 0xFEF2F2, dark: 0x2D0F0F)
    static let accentCriticalBanner  = dynamic(light: 0xEF4444, dark: 0xF87171)
    static let textCriticalBanner    = dynamic(light: 0x7F1D1D, dark: 0xFCA5A5)
    static let surfacePromotionBanner = dynamic(light: 0xECFDF5, dark: 0x0D2818)
    static let accentPromotionBanner  = dynamic(light: 0x10B981, dark: 0x34D399)
    static let textPromotionBanner    = dynamic(light: 0x064E3B, dark: 0xA7F3D0)

    // Status pill — backgrounds
    static let statusOKBg        = dynamic(light: 0xD1FAE5, dark: 0x064E3B)
    static let statusERRBg       = dynamic(light: 0xFEE2E2, dark: 0x7F1D1D)
    static let statusWARNBg      = dynamic(light: 0xFEF3C7, dark: 0x713F12)
    static let statusINFOBg      = dynamic(light: 0xDBEAFE, dark: 0x1E3A5F)

    // Status pill — text
    static let statusOKFg        = dynamic(light: 0x047857, dark: 0x6EE7B7)
    static let statusERRFg       = dynamic(light: 0xB91C1C, dark: 0xFCA5A5)
    static let statusWARNFg      = dynamic(light: 0x854D0E, dark: 0xFDE68A)
    static let statusINFOFg      = dynamic(light: 0x1D4ED8, dark: 0x93C5FD)

    // MCP Companion — savings
    static let savingsGreen       = dynamic(light: 0x4ADE80, dark: 0x4ADE80)
    static let savingsGreenMuted  = dynamic(light: 0xBBF7D0, dark: 0x166534)
    static let companionBlue      = dynamic(light: 0x60A5FA, dark: 0x60A5FA)
    static let companionPurple    = dynamic(light: 0xC084FC, dark: 0xC084FC)
    static let setupStepBg        = dynamic(light: 0xF8FAFC, dark: 0x2C2C2E)
    static let setupCheckmark     = dynamic(light: 0x22C55E, dark: 0x4ADE80)
    static let setupActionBg      = dynamic(light: 0x3B82F6, dark: 0x3B82F6)
    static let setupActionFg      = dynamic(light: 0xFFFFFF, dark: 0xFFFFFF)
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

/// NSView subclass that re-applies its dynamic background color when
/// the system appearance changes (light ↔ dark). Use for any long-lived
/// container whose layer background must stay in sync.
class AppearanceAwareView: NSView {
    private var dynamicBgColor: NSColor?

    func setDynamicBackground(_ color: NSColor) {
        dynamicBgColor = color
        wantsLayer = true
        layer?.backgroundColor = color.cgColor
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
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

// MARK: - CheckRowView (THE WORKED EXAMPLE — copy this pattern)
//
// Anatomy:
//   [pill]  Title (cardTitle, textPrimary)
//           detail (detailText, textSecondary, wraps)
//
// Padding: rowHorizontal × rowVertical
// Background: surfacePrimary
//
// Notice: every value comes from a token. No literals. Replicate this
// discipline in the stubs below.

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

// MARK: - Stub rows — implement following the CheckRowView pattern
//
// REMINDER: tokens only. Hex literals, raw font sizes, raw padding numbers
// in this section are bugs. Add to the token sections above first.

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
