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
