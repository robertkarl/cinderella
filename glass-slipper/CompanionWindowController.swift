//
//  CompanionWindowController.swift
//  Glass Slipper — Claude Code companion window
//
//  Setup checklist (first-run) → savings dashboard (daily use).
//  Reads mcp-activity.jsonl via MCPActivityLog for live stats.
//

import AppKit

final class CompanionWindowController: NSWindowController, MCPActivityLogDelegate {

    private let activityLog = MCPActivityLog()

    // Setup views
    private var setupStack: NSStackView!
    private var modelRow: SetupRow!
    private var serverRow: SetupRow!
    private var mcpRow: SetupRow!

    // Dashboard views
    private var dashboardStack: NSStackView!
    private var savedLabel: NSTextField!
    private var delegatedLabel: NSTextField!
    private var tokensSavedLabel: NSTextField!
    private var activityStack: NSStackView!
    private var statusLine: NSTextField!

    // State
    private var isModelDownloaded: Bool {
        // Check directly — the manifest Codable may fail on extra fields.
        let path = NSHomeDirectory() + "/Library/Application Support/Glass Slipper/Models/Qwen3.5-9B-Q5_K_M.gguf"
        return FileManager.default.fileExists(atPath: path)
    }

    private var isServerRunning: Bool {
        let semaphore = DispatchSemaphore(value: 0)
        var healthy = false
        guard let url = URL(string: "http://127.0.0.1:8787/health") else { return false }
        URLSession.shared.dataTask(with: url) { _, response, _ in
            if let http = response as? HTTPURLResponse, http.statusCode == 200 {
                healthy = true
            }
            semaphore.signal()
        }.resume()
        _ = semaphore.wait(timeout: .now() + 2)
        return healthy
    }

    convenience init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 420, height: 480),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Glass Slipper — Claude Companion"
        window.minSize = NSSize(width: 360, height: 320)
        window.center()

        self.init(window: window)
        buildUI()
        refreshState()
        activityLog.delegate = self
        activityLog.startPolling()
    }

    // MARK: - UI Construction

    private func buildUI() {
        guard let contentView = window?.contentView else { return }
        contentView.wantsLayer = true
        contentView.layer?.backgroundColor = NSColor.surfacePrimary.cgColor

        // Setup rows
        modelRow = SetupRow(
            step: "1",
            title: "Model",
            detail: "Qwen 3.5 9B · Q5_K_M",
            actionTitle: "Download",
            action: { [weak self] in self?.handleModelDownload() }
        )
        serverRow = SetupRow(
            step: "2",
            title: "Server",
            detail: "llama-server · Port 8787",
            actionTitle: "Start",
            action: { [weak self] in self?.handleServerStart() }
        )
        mcpRow = SetupRow(
            step: "3",
            title: "Claude Code MCP",
            detail: "Not configured",
            actionTitle: "Install",
            action: { [weak self] in self?.handleMCPInstall() }
        )

        setupStack = NSStackView(views: [modelRow, serverRow, mcpRow])
        setupStack.orientation = .vertical
        setupStack.spacing = Spacing.md
        setupStack.translatesAutoresizingMaskIntoConstraints = false

        // Stats
        savedLabel = makeStatLabel(color: .savingsGreen)
        savedLabel.stringValue = "$0.00"

        delegatedLabel = makeStatLabel(color: .companionBlue)
        delegatedLabel.stringValue = "0"

        tokensSavedLabel = makeStatLabel(color: .companionPurple)
        tokensSavedLabel.stringValue = "0"

        let savedSub = makeSubtitleLabel("saved today")
        let delegatedSub = makeSubtitleLabel("delegated")
        let tokensSub = makeSubtitleLabel("tokens saved")

        let savedCol = makeStatColumn(value: savedLabel, subtitle: savedSub)
        let delegatedCol = makeStatColumn(value: delegatedLabel, subtitle: delegatedSub)
        let tokensCol = makeStatColumn(value: tokensSavedLabel, subtitle: tokensSub)

        let statsRow = NSStackView(views: [savedCol, delegatedCol, tokensCol])
        statsRow.distribution = .fillEqually
        statsRow.translatesAutoresizingMaskIntoConstraints = false

        // Activity header
        let activityHeader = NSTextField(labelWithString: "ACTIVITY")
        activityHeader.font = .sectionHeader
        activityHeader.textColor = .textQuiet
        activityHeader.translatesAutoresizingMaskIntoConstraints = false

        // Activity scroll
        activityStack = NSStackView()
        activityStack.orientation = .vertical
        activityStack.spacing = 2
        activityStack.translatesAutoresizingMaskIntoConstraints = false

        let docView = FlippedView()
        docView.translatesAutoresizingMaskIntoConstraints = false
        docView.addSubview(activityStack)
        NSLayoutConstraint.activate([
            activityStack.topAnchor.constraint(equalTo: docView.topAnchor),
            activityStack.leadingAnchor.constraint(equalTo: docView.leadingAnchor),
            activityStack.trailingAnchor.constraint(equalTo: docView.trailingAnchor),
        ])

        let scrollView = NSScrollView()
        scrollView.documentView = docView
        scrollView.hasVerticalScroller = true
        scrollView.translatesAutoresizingMaskIntoConstraints = false

        // Status line (collapsed setup)
        statusLine = NSTextField(labelWithString: "")
        statusLine.font = .detailText
        statusLine.textColor = .textSecondary
        statusLine.translatesAutoresizingMaskIntoConstraints = false
        statusLine.isHidden = true

        dashboardStack = NSStackView(views: [statusLine, statsRow, activityHeader, scrollView])
        dashboardStack.orientation = .vertical
        dashboardStack.spacing = Spacing.lg
        dashboardStack.translatesAutoresizingMaskIntoConstraints = false

        contentView.addSubview(setupStack)
        contentView.addSubview(dashboardStack)

        NSLayoutConstraint.activate([
            setupStack.topAnchor.constraint(equalTo: contentView.topAnchor, constant: Spacing.xxl),
            setupStack.leadingAnchor.constraint(equalTo: contentView.leadingAnchor, constant: Spacing.xl),
            setupStack.trailingAnchor.constraint(equalTo: contentView.trailingAnchor, constant: -Spacing.xl),

            dashboardStack.topAnchor.constraint(equalTo: contentView.topAnchor, constant: Spacing.xxl),
            dashboardStack.leadingAnchor.constraint(equalTo: contentView.leadingAnchor, constant: Spacing.xl),
            dashboardStack.trailingAnchor.constraint(equalTo: contentView.trailingAnchor, constant: -Spacing.xl),
            dashboardStack.bottomAnchor.constraint(equalTo: contentView.bottomAnchor, constant: -Spacing.xl),

            scrollView.heightAnchor.constraint(greaterThanOrEqualToConstant: 200),
        ])
    }

    // MARK: - State

    private func refreshState() {
        let modelOK = isModelDownloaded
        let serverOK = isServerRunning
        let mcpOK = MCPInstaller.isInstalled

        modelRow.setComplete(modelOK)
        serverRow.setComplete(serverOK)
        mcpRow.setComplete(mcpOK)
        if mcpOK {
            mcpRow.updateDetail("Configured")
        }

        let allDone = modelOK && serverOK && mcpOK
        setupStack.isHidden = allDone
        dashboardStack.isHidden = !allDone

        if allDone {
            statusLine.isHidden = false
            statusLine.stringValue = "● Qwen 3.5 9B · Running · MCP Connected"
            statusLine.textColor = .setupCheckmark
        }
    }

    // MARK: - Actions

    private func handleModelDownload() {
        refreshState()
    }

    private func handleServerStart() {
        refreshState()
    }

    private func handleMCPInstall() {
        if let error = MCPInstaller.install() {
            let alert = NSAlert()
            alert.messageText = "MCP Install Failed"
            alert.informativeText = error
            alert.runModal()
        }
        refreshState()
    }

    // MARK: - MCPActivityLogDelegate

    func activityLogDidUpdate(entries: [MCPActivityEntry], summary: MCPSavingsSummary) {
        savedLabel.stringValue = String(format: "$%.2f", summary.totalCostSaved)
        delegatedLabel.stringValue = "\(summary.totalTasksDelegated)"

        if summary.totalTokensSaved >= 1000 {
            tokensSavedLabel.stringValue = String(format: "%.1fk", Double(summary.totalTokensSaved) / 1000.0)
        } else {
            tokensSavedLabel.stringValue = "\(summary.totalTokensSaved)"
        }

        let recentEntries = entries.suffix(50).reversed()
        activityStack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for entry in recentEntries {
            let row = makeActivityRow(entry: entry)
            activityStack.addArrangedSubview(row)
        }
    }

    // MARK: - Helpers

    private func makeStatLabel(color: NSColor) -> NSTextField {
        let field = NSTextField(labelWithString: "")
        field.font = .systemFont(ofSize: 24, weight: .bold)
        field.textColor = color
        field.alignment = .center
        field.translatesAutoresizingMaskIntoConstraints = false
        return field
    }

    private func makeSubtitleLabel(_ text: String) -> NSTextField {
        let field = NSTextField(labelWithString: text)
        field.font = .systemFont(ofSize: 10)
        field.textColor = .textQuiet
        field.alignment = .center
        field.translatesAutoresizingMaskIntoConstraints = false
        return field
    }

    private func makeStatColumn(value: NSTextField, subtitle: NSTextField) -> NSStackView {
        let col = NSStackView(views: [value, subtitle])
        col.orientation = .vertical
        col.spacing = 2
        col.alignment = .centerX
        col.translatesAutoresizingMaskIntoConstraints = false
        return col
    }

    private func makeActivityRow(entry: MCPActivityEntry) -> NSView {
        let container = NSView()
        container.translatesAutoresizingMaskIntoConstraints = false

        let toolLabel = NSTextField(labelWithString: entry.tool)
        toolLabel.font = .detailText
        toolLabel.textColor = .textPrimary
        toolLabel.translatesAutoresizingMaskIntoConstraints = false

        let costLabel = NSTextField(labelWithString: String(format: "$%.3f", entry.estimatedCloudCostUSD))
        costLabel.font = .detailText
        costLabel.textColor = .savingsGreen
        costLabel.alignment = .right
        costLabel.translatesAutoresizingMaskIntoConstraints = false

        let tokenLabel = NSTextField(labelWithString: "\(entry.inputTokens)→\(entry.outputTokens) tok")
        tokenLabel.font = .systemFont(ofSize: 10)
        tokenLabel.textColor = .textQuiet
        tokenLabel.translatesAutoresizingMaskIntoConstraints = false

        container.addSubview(toolLabel)
        container.addSubview(costLabel)
        container.addSubview(tokenLabel)

        NSLayoutConstraint.activate([
            toolLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            toolLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 2),
            tokenLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            tokenLabel.topAnchor.constraint(equalTo: toolLabel.bottomAnchor),
            tokenLabel.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -2),
            costLabel.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            costLabel.centerYAnchor.constraint(equalTo: container.centerYAnchor),
        ])

        return container
    }
}

// MARK: - SetupRow

final class SetupRow: NSView {
    private let stepLabel = NSTextField(labelWithString: "")
    private let titleLabel = NSTextField(labelWithString: "")
    private let detailLabel = NSTextField(labelWithString: "")
    private let actionButton: NSButton
    private let checkmark = NSTextField(labelWithString: "✓")
    private var onAction: (() -> Void)?

    init(step: String, title: String, detail: String, actionTitle: String, action: @escaping () -> Void) {
        self.actionButton = NSButton(title: actionTitle, target: nil, action: nil)
        self.onAction = action
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.setupStepBg.cgColor
        layer?.cornerRadius = 6
        translatesAutoresizingMaskIntoConstraints = false

        stepLabel.stringValue = step
        stepLabel.font = .sectionHeader
        stepLabel.textColor = .textQuiet
        stepLabel.translatesAutoresizingMaskIntoConstraints = false

        titleLabel.stringValue = title
        titleLabel.font = .cardTitle
        titleLabel.textColor = .textPrimary
        titleLabel.translatesAutoresizingMaskIntoConstraints = false

        detailLabel.stringValue = detail
        detailLabel.font = .detailText
        detailLabel.textColor = .textSecondary
        detailLabel.translatesAutoresizingMaskIntoConstraints = false

        actionButton.bezelStyle = .rounded
        actionButton.target = self
        actionButton.action = #selector(buttonClicked)
        actionButton.translatesAutoresizingMaskIntoConstraints = false

        checkmark.font = .systemFont(ofSize: 16, weight: .bold)
        checkmark.textColor = .setupCheckmark
        checkmark.translatesAutoresizingMaskIntoConstraints = false
        checkmark.isHidden = true

        addSubview(stepLabel)
        addSubview(titleLabel)
        addSubview(detailLabel)
        addSubview(actionButton)
        addSubview(checkmark)

        NSLayoutConstraint.activate([
            heightAnchor.constraint(greaterThanOrEqualToConstant: 48),

            stepLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Spacing.lg),
            stepLabel.centerYAnchor.constraint(equalTo: centerYAnchor),

            titleLabel.leadingAnchor.constraint(equalTo: stepLabel.trailingAnchor, constant: Spacing.md),
            titleLabel.topAnchor.constraint(equalTo: topAnchor, constant: Spacing.md),

            detailLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            detailLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 2),
            detailLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.md),

            actionButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.lg),
            actionButton.centerYAnchor.constraint(equalTo: centerYAnchor),

            checkmark.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.lg),
            checkmark.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) { fatalError("not in IB") }

    func setComplete(_ complete: Bool) {
        actionButton.isHidden = complete
        checkmark.isHidden = !complete
    }

    func updateDetail(_ text: String) {
        detailLabel.stringValue = text
    }

    @objc private func buttonClicked() {
        onAction?()
    }
}
