//
//  CinderellaScaffold.swift
//  Cinderella's Glass Slipper — AppKit starter
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
//  - Append events from your cinderella JSON stream by calling
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
// Mapped 1:1 from the React mock's Tailwind palette so visual parity is
// possible to verify with a screenshot diff. Light-mode only for v0.
// Adapt to dynamic appearance later by swapping these for NSColor sets.

extension NSColor {
    // Surfaces
    static let surfacePrimary    = NSColor(hex: 0xFFFFFF)               // white
    static let surfaceMuted      = NSColor(hex: 0xFAFAFA)               // zinc-50  (thought row, hover)
    static let surfaceHeader     = NSColor(hex: 0xF4F4F5)               // zinc-100 (input header bar)
    static let surfaceDiagnosis  = NSColor(hex: 0xECFDF5)               // emerald-50

    // Text
    static let textPrimary       = NSColor(hex: 0x18181B)               // zinc-900
    static let textSecondary     = NSColor(hex: 0x71717A)               // zinc-500
    static let textQuiet         = NSColor(hex: 0xA1A1AA)               // zinc-400

    // Lines & accents
    static let separatorHairline = NSColor(hex: 0xF4F4F5)               // zinc-100
    static let accentDiagnosis   = NSColor(hex: 0x10B981)               // emerald-500 (left border)
    static let accentDiagLabel   = NSColor(hex: 0x047857)               // emerald-700 (DIAGNOSIS text)

    // Status pill — backgrounds
    static let statusOKBg        = NSColor(hex: 0xD1FAE5)               // emerald-100
    static let statusERRBg       = NSColor(hex: 0xFEE2E2)               // red-100
    static let statusWARNBg      = NSColor(hex: 0xFEF3C7)               // yellow-100
    static let statusINFOBg      = NSColor(hex: 0xDBEAFE)               // blue-100

    // Status pill — text
    static let statusOKFg        = NSColor(hex: 0x047857)               // emerald-700
    static let statusERRFg       = NSColor(hex: 0xB91C1C)               // red-700
    static let statusWARNFg      = NSColor(hex: 0x854D0E)               // yellow-800
    static let statusINFOFg      = NSColor(hex: 0x1D4ED8)               // blue-700
}

// MARK: - Typography tokens

extension NSFont {
    static var cardTitle:      NSFont { .systemFont(ofSize: 15, weight: .semibold) }
    static var detailText:     NSFont { .systemFont(ofSize: 13, weight: .regular) }
    static var sectionHeader:  NSFont { .systemFont(ofSize: 11, weight: .semibold) }
    static var diagnosisLabel: NSFont { .systemFont(ofSize: 11, weight: .bold) }
    static var diagnosisText:  NSFont { .systemFont(ofSize: 15, weight: .regular) }
    static var stampLabel:     NSFont { .systemFont(ofSize: 10, weight: .bold) }
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
    case diagnosis(text: String)
    // Future card kinds — add here, then add a case in EventRowFactory and
    // a corresponding NSView subclass:
    // case waterfall(rows: [WaterfallSegment])
    // case miniChart(label: String, samples: [Double])
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
        case .diagnosis(let text):
            return DiagnosisRowView(text: text)
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
    init(items: [String]) {
        super.init(frame: .zero)
        // TODO: small uppercase "PLAN" header (sectionHeader font,
        //       textSecondary, kerning ~1.0), then bulleted list — each
        //       item is a 1.5pt circle (textQuiet) + detailText label.
        //       Use NSStackView for the items list.
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

final class ThoughtRowView: NSView {
    init(text: String) {
        super.init(frame: .zero)
        // TODO: surfaceMuted bg, "···" prefix (textQuiet) + italic
        //       detailItalic body in textSecondary. Tighter vertical
        //       padding than CheckRow (use Spacing.lg, not rowVertical) —
        //       thoughts should feel like asides, not equals to checks.
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

final class DiagnosisRowView: NSView {
    init(text: String) {
        super.init(frame: .zero)
        // TODO: surfaceDiagnosis bg. 4pt left edge in accentDiagnosis
        //       (use a sub-NSView pinned leading, width = diagBorderW).
        //       Inside: "DIAGNOSIS" header (diagnosisLabel font,
        //       accentDiagLabel color, uppercase, kerning), then body
        //       in diagnosisText / textPrimary. Generous vertical
        //       padding — this is the "answer" row, it should breathe.
    }
    required init?(coder: NSCoder) { fatalError("not in IB") }
}

// MARK: - Spine view controller

final class SpineViewController: NSViewController {

    private let scrollView = NSScrollView()
    private let stackView  = NSStackView()
    private(set) var events: [Event] = []

    override func loadView() {
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 720, height: 720))
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor.surfacePrimary.cgColor
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

        let documentView = NSView()
        documentView.translatesAutoresizingMaskIntoConstraints = false
        documentView.addSubview(stackView)
        scrollView.documentView = documentView

        root.addSubview(scrollView)

        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: root.topAnchor),
            scrollView.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: root.bottomAnchor),

            documentView.topAnchor.constraint(equalTo: scrollView.contentView.topAnchor),
            documentView.leadingAnchor.constraint(equalTo: scrollView.contentView.leadingAnchor),
            documentView.trailingAnchor.constraint(equalTo: scrollView.contentView.trailingAnchor),
            documentView.widthAnchor.constraint(equalTo: scrollView.contentView.widthAnchor),

            stackView.topAnchor.constraint(equalTo: documentView.topAnchor),
            stackView.leadingAnchor.constraint(equalTo: documentView.leadingAnchor),
            stackView.trailingAnchor.constraint(equalTo: documentView.trailingAnchor),
            stackView.bottomAnchor.constraint(equalTo: documentView.bottomAnchor),
        ])
    }

    /// Append a new event from the cinderella JSON stream. Main thread only.
    func append(_ event: Event) {
        let needsDivider = !events.isEmpty && shouldShowDividerAbove(event)
        events.append(event)

        if needsDivider {
            stackView.addArrangedSubview(HairlineDivider())
        }
        let row = EventRowFactory.makeRow(for: event)
        row.translatesAutoresizingMaskIntoConstraints = false
        stackView.addArrangedSubview(row)
        row.widthAnchor.constraint(equalTo: stackView.widthAnchor).isActive = true

        scrollToBottom()
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
        case .diagnosis, .userPrompt: return false
        default: return true
        }
    }
}

// MARK: - Smoke test
//
// Add this to your AppDelegate's applicationDidFinishLaunching to verify
// the scaffold renders before wiring up the cinderella subprocess.
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
