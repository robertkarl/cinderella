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
