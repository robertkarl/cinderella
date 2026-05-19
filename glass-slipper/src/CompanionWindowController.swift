//
//  CompanionWindowController.swift
//  Glass Slipper — Claude Code companion window
//
//  Setup checklist (first-run) → savings dashboard (daily use).
//  Reads mcp-activity.jsonl via MCPActivityLog for live stats.
//

import AppKit

final class CompanionWindowController: NSWindowController, MCPActivityLogDelegate, LlamaServerManagerDelegate, ModelDownloadManagerDelegate {

    private let activityLog = MCPActivityLog()
    private let serverManager = LlamaServerManager()
    private var downloadManager: ModelDownloadManager?

    // Setup views
    private var setupStack: NSStackView!
    private var modelRow: SetupRow!
    private var serverRow: SetupRow!
    private var mcpRow: SetupRow!
    private var codexRow: SetupRow!

    // Dashboard views
    private var dashboardStack: NSStackView!
    private var savedLabel: NSTextField!
    private var delegatedLabel: NSTextField!
    private var tokensSavedLabel: NSTextField!
    private var activityStack: NSStackView!
    private var statusLine: NSTextField!

    // State
    private var isModelDownloaded: Bool {
        FileManager.default.fileExists(atPath: LlamaServerManager.modelFilePath())
    }

    private var isServerRunning: Bool {
        serverManager.state == .running
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
        serverManager.delegate = self
        // Auto-start if model is already downloaded
        if isModelDownloaded {
            serverManager.start()
        }
    }

    var llamaServerManager: LlamaServerManager { serverManager }

    // MARK: - UI Construction

    private func buildUI() {
        // Replace the window's content view with an appearance-aware one
        let awareView = AppearanceAwareView()
        awareView.setDynamicBackground(.surfacePrimary)
        window?.contentView = awareView
        guard let contentView = window?.contentView else { return }

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

        codexRow = SetupRow(
            step: "",
            title: "Codex CLI",
            detail: CodexMCPInstaller.isCodexAvailable ? "Not configured" : "Codex CLI not found",
            actionTitle: CodexMCPInstaller.isCodexAvailable ? "Install" : "Open Terminal",
            action: { [weak self] in self?.handleCodexInstall() }
        )

        setupStack = NSStackView(views: [modelRow, serverRow, mcpRow, codexRow])
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

        // Surface manifest errors eagerly so the user sees them before clicking
        if !modelOK {
            if ModelDownloadManager.loadManifest() == nil {
                modelRow.showError("Model manifest not found. The app bundle may be damaged — try reinstalling.")
            }
        }

        // Codex row: optional, not gating allDone
        let codexAvailable = CodexMCPInstaller.isCodexAvailable
        codexRow.setComplete(CodexMCPInstaller.isInstalled)
        if !codexAvailable {
            codexRow.updateDetail("Codex CLI not found")
        } else if CodexMCPInstaller.isInstalled {
            codexRow.updateDetail("Configured")
        } else {
            codexRow.updateDetail("Not configured")
        }

        // Fire async refresh so cached state catches up with reality
        CodexMCPInstaller.refreshState()

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
        guard let manifest = ModelDownloadManager.loadManifest() else {
            modelRow.showError("Could not load model manifest. The app bundle may be damaged — try reinstalling.")
            AppLogger.log("manifest_load_failed")
            return
        }
        guard let modelDef = manifest.models.first(where: { $0.id == manifest.default_model }) else {
            modelRow.showError("Default model '\(manifest.default_model)' not found in manifest.")
            return
        }

        let manager = ModelDownloadManager(model: modelDef)
        manager.delegate = self
        self.downloadManager = manager

        if manager.isModelPresent {
            modelRow.setComplete(true)
            refreshState()
            return
        }

        modelRow.updateDetail("Downloading…")
        AppLogger.log("model_download_start", ["model": modelDef.id])
        manager.startDownload()
    }

    private func handleServerStart() {
        serverRow.clearError()
        serverManager.start()
    }

    private func handleMCPInstall() {
        mcpRow.clearError()
        if let error = MCPInstaller.install() {
            mcpRow.showError(error)
        }
        refreshState()
    }

    private func handleCodexInstall() {
        if !CodexMCPInstaller.isCodexAvailable {
            CodexMCPInstaller.openTerminalFallback()
            return
        }

        codexRow.clearError()
        if CodexMCPInstaller.isInstalled {
            CodexMCPInstaller.uninstall { [weak self] error in
                if let error = error {
                    self?.codexRow.showError(error)
                }
                self?.refreshState()
            }
        } else {
            CodexMCPInstaller.install { [weak self] error in
                if let error = error {
                    self?.codexRow.showError(error)
                }
                self?.refreshState()
            }
        }
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

    // MARK: - LlamaServerManagerDelegate

    func serverStateDidChange(_ state: LlamaServerState) {
        switch state {
        case .starting:
            serverRow.clearError()
            serverRow.updateDetail("Starting…")
            AppLogger.log("server_starting")
        case .running:
            serverRow.setComplete(true)
            serverRow.updateDetail("llama-server · Port 8787")
            AppLogger.log("server_running")
            refreshState()
        case .failed(let msg):
            serverRow.setComplete(false)
            serverRow.updateDetail("llama-server · Port 8787")
            serverRow.showError(msg)
            AppLogger.log("server_failed", ["error": msg])
        case .notRunning:
            serverRow.setComplete(false)
            serverRow.updateDetail("llama-server · Port 8787")
        }
    }

    // MARK: - ModelDownloadManagerDelegate

    func downloadDidUpdateProgress(downloaded: Int64, total: Int64) {
        let pct = total > 0 ? Double(downloaded) / Double(total) : 0
        let dlGB = String(format: "%.1f", Double(downloaded) / 1_073_741_824)
        let totalGB = String(format: "%.1f", Double(total) / 1_073_741_824)
        modelRow.updateDetail("\(dlGB) / \(totalGB) GB (\(Int(pct * 100))%)")
    }

    func downloadDidBeginVerification() {
        modelRow.updateDetail("Verifying integrity…")
    }

    func downloadDidFinish() {
        modelRow.setComplete(true)
        modelRow.updateDetail("Qwen 3.5 9B · Q5_K_M")
        AppLogger.log("model_download_complete")
        serverManager.start()
        refreshState()
    }

    func downloadDidFail(error: String) {
        modelRow.updateDetail("Qwen 3.5 9B · Q5_K_M")
        modelRow.showError(error)
        AppLogger.log("model_download_failed", ["error": error])
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

        let displayName = Self.toolDisplayName(tool: entry.tool, detail: entry.detail)
        let toolLabel = NSTextField(labelWithString: displayName)
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

    /// "local_summarize" + "noisy-build.sh" → "Summarize — noisy-build.sh"
    private static func toolDisplayName(tool: String, detail: String) -> String {
        let verb: String
        switch tool {
        case "local_summarize": verb = "Summarize"
        case "local_explain":   verb = "Explain"
        case "local_ask":       verb = "Ask"
        case "local_web_fetch": verb = "Fetch"
        case "local_review":    verb = "Review"
        case "local_draft":     verb = "Draft"
        case "local_status":    verb = "Status"
        case "local_pass_fail": verb = "Pass/Fail"
        default:                verb = tool
        }
        if detail.isEmpty {
            return verb
        }
        return "\(verb) — \(detail)"
    }
}

// MARK: - SetupRow

final class SetupRow: AppearanceAwareView {
    private let stepLabel = NSTextField(labelWithString: "")
    private let titleLabel = NSTextField(labelWithString: "")
    private let detailLabel = NSTextField(labelWithString: "")
    private let errorLabel = NSTextField(wrappingLabelWithString: "")
    private let actionButton: NSButton
    private let checkmark = NSTextField(labelWithString: "✓")
    private var onAction: (() -> Void)?

    init(step: String, title: String, detail: String, actionTitle: String, action: @escaping () -> Void) {
        self.actionButton = NSButton(title: actionTitle, target: nil, action: nil)
        self.onAction = action
        super.init(frame: .zero)

        setDynamicBackground(.setupStepBg)
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

        errorLabel.font = .detailText
        errorLabel.textColor = .statusERRFg
        errorLabel.translatesAutoresizingMaskIntoConstraints = false
        errorLabel.isHidden = true
        errorLabel.maximumNumberOfLines = 3

        addSubview(stepLabel)
        addSubview(titleLabel)
        addSubview(detailLabel)
        addSubview(errorLabel)
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
            detailLabel.trailingAnchor.constraint(lessThanOrEqualTo: actionButton.leadingAnchor, constant: -Spacing.md),

            errorLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            errorLabel.topAnchor.constraint(equalTo: detailLabel.bottomAnchor, constant: 2),
            errorLabel.trailingAnchor.constraint(lessThanOrEqualTo: actionButton.leadingAnchor, constant: -Spacing.md),
            errorLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.md),

            actionButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.lg),
            actionButton.centerYAnchor.constraint(equalTo: centerYAnchor),

            checkmark.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Spacing.lg),
            checkmark.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])

        // When no error, detail is the bottom anchor
        detailBottomConstraint = detailLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Spacing.md)
        detailBottomConstraint?.priority = .defaultHigh
        detailBottomConstraint?.isActive = true
    }

    private var detailBottomConstraint: NSLayoutConstraint?

    required init?(coder: NSCoder) { fatalError("not in IB") }

    func setComplete(_ complete: Bool) {
        actionButton.isHidden = complete
        checkmark.isHidden = !complete
        if complete { clearError() }
    }

    func updateDetail(_ text: String) {
        detailLabel.stringValue = text
    }

    func showError(_ message: String) {
        errorLabel.stringValue = message
        errorLabel.isHidden = false
        detailBottomConstraint?.isActive = false
    }

    func clearError() {
        errorLabel.stringValue = ""
        errorLabel.isHidden = true
        detailBottomConstraint?.isActive = true
    }

    @objc private func buttonClicked() {
        onAction?()
    }
}
