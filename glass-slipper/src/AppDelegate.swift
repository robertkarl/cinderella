//
//  AppDelegate.swift
//  Glass Slipper — Swift port of main.m
//
//  @main entry point. Window setup, Process management, JSON parsing,
//  event dispatch to SpineViewController. Menu bar, JSONL debug logging,
//  click-to-copy on rows.
//

import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {

    private var window: NSWindow!
    private var urlField: NSTextField!
    private var diagnoseButton: NSButton!
    private var spineVC: SpineViewController!
    private var statusBar: StatusBarView!
    /// Currently active model name, updated on hw_info and model_swap events.
    private var currentModelName: String = ""
    /// When non-nil, suppress warning banners until this date.
    private var warningDismissedUntil: Date?
    /// When non-nil, suppress promotion banners until this date.
    private var promotionDismissedUntil: Date?

    private var process: Process?
    private var stdoutPipe: Pipe?
    private var stderrPipe: Pipe?
    private var lineBuffer = Data()
    private var stderrBuffer = Data()
    private var isRunning = false
    private var logFileHandle: FileHandle?
    /// Tracks worst step status across the run for diagnosis coloring.
    private var worstStatus: EventStatus = .ok
    private var downloadManager: ModelDownloadManager?
    private var downloadRowView: ModelDownloadRowView?
    private var companionWindowController: CompanionWindowController?

    // MARK: - Application lifecycle

    // MARK: - Directories

    static let appSupportDir: String = {
        NSHomeDirectory() + "/Library/Application Support/Glass Slipper"
    }()

    static let logsDir: String = {
        NSHomeDirectory() + "/Library/Logs/Glass Slipper"
    }()

    /// Eagerly create required directories at launch, before anything else
    /// tries to read/write them.
    private func ensureDirectories() {
        let fm = FileManager.default
        for dir in [Self.appSupportDir, Self.appSupportDir + "/Models", Self.logsDir] {
            try? fm.createDirectory(atPath: dir, withIntermediateDirectories: true)
        }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        ensureDirectories()
        AppLogger.start()
        AppLogger.log("app_launch")
        setupMenuBar()
        setupCompanionWindow()
        if #available(macOS 14.0, *) {
            NSApp.activate()
        } else {
            NSApp.activate(ignoringOtherApps: true)
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return false
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag {
            showCompanionWindow()
        }
        return true
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        // Stop managed server first (clean SIGTERM on known PID)
        companionWindowController?.llamaServerManager.stop()

        // Fallback: kill any llama-server on port 8787 we didn't manage
        let killedServer = killLlamaServer()

        var killedProcess = false
        if let proc = process, proc.isRunning {
            proc.terminate()
            killedProcess = true
        }

        if killedServer || killedProcess {
            // 2s hard deadline — exit regardless of child process state
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) {
                NSApp.reply(toApplicationShouldTerminate: true)
            }
            return .terminateLater
        }

        return .terminateNow
    }

    func applicationWillTerminate(_ notification: Notification) {
        downloadManager?.cancelDownload()
    }

    // MARK: - Model check on launch

    private func checkModelOnLaunch() {
        guard let manifest = ModelDownloadManager.loadManifest(),
              let modelDef = manifest.models.first(where: { $0.id == manifest.default_model }) else {
            diagnoseButton.isEnabled = true  // no manifest = dev mode, let it through
            return
        }

        let manager = ModelDownloadManager(model: modelDef)
        manager.delegate = self
        self.downloadManager = manager

        if manager.isModelPresent {
            diagnoseButton.isEnabled = true
            return
        }

        // Show download row
        let sizeGB = String(format: "%.1f GB", Double(modelDef.size_bytes) / 1_073_741_824)
        let row = ModelDownloadRowView()
        row.showMissing(name: modelDef.name, sizeGB: sizeGB)
        row.onAction = { [weak self] in
            self?.handleDownloadAction()
        }
        self.downloadRowView = row

        // Show the download row in place of the spine
        row.translatesAutoresizingMaskIntoConstraints = false
        if let contentView = window.contentView {
            // Remove spine temporarily, show download row in its place
            spineVC.view.isHidden = true
            contentView.addSubview(row)
            NSLayoutConstraint.activate([
                row.topAnchor.constraint(equalTo: urlField.bottomAnchor, constant: Spacing.lg),
                row.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
                row.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            ])
        }
    }

    private func handleDownloadAction() {
        downloadRowView?.showProgress(downloaded: 0, total: downloadManager?.model.size_bytes ?? 0)
        downloadManager?.startDownload()
    }

    // MARK: - Menu bar

    private func setupMenuBar() {
        let menubar = NSMenu()

        // App menu
        let appMenuItem = NSMenuItem()
        menubar.addItem(appMenuItem)
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "About Glass Slipper", action: #selector(showAboutPanel), keyEquivalent: "")
        appMenu.addItem(NSMenuItem.separator())
        appMenu.addItem(withTitle: "Quit Glass Slipper", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appMenuItem.submenu = appMenu

        // Edit menu — required for Cmd+C/V/X/A in text fields
        let editMenuItem = NSMenuItem()
        menubar.addItem(editMenuItem)
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editMenuItem.submenu = editMenu

        // Window menu
        let windowMenuItem = NSMenuItem()
        menubar.addItem(windowMenuItem)
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(withTitle: "Claude Companion", action: #selector(showCompanionWindow), keyEquivalent: "1")
        windowMenu.addItem(withTitle: "Network Debug", action: #selector(showNetworkDebugWindow), keyEquivalent: "2")
        windowMenuItem.submenu = windowMenu

        NSApp.mainMenu = menubar
    }

    private var aboutWindowController: NSWindowController?

    @objc private func showAboutPanel() {
        if let wc = aboutWindowController {
            wc.showWindow(nil)
            wc.window?.makeKeyAndOrderFront(nil)
            return
        }

        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 300, height: 240),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = "About Glass Slipper"
        window.center()

        let contentView = AppearanceAwareView()
        contentView.setDynamicBackground(.surfacePrimary)
        window.contentView = contentView

        let title = NSTextField(labelWithString: "Glass Slipper")
        title.font = .systemFont(ofSize: 18, weight: .bold)
        title.textColor = .textPrimary
        title.alignment = .center
        title.translatesAutoresizingMaskIntoConstraints = false

        let versionLabel = NSTextField(labelWithString: "v\(version)")
        versionLabel.font = .systemFont(ofSize: 13)
        versionLabel.textColor = .textSecondary
        versionLabel.alignment = .center
        versionLabel.translatesAutoresizingMaskIntoConstraints = false

        let subtitle = NSTextField(labelWithString: "Local AI for Claude Code")
        subtitle.font = .systemFont(ofSize: 11)
        subtitle.textColor = .textQuiet
        subtitle.alignment = .center
        subtitle.translatesAutoresizingMaskIntoConstraints = false

        let logsButton = NSButton(title: "Application Logs…", target: self, action: #selector(openLogsDirectory))
        logsButton.bezelStyle = .rounded
        logsButton.translatesAutoresizingMaskIntoConstraints = false

        let supportButton = NSButton(title: "Application Support…", target: self, action: #selector(openAppSupportDirectory))
        supportButton.bezelStyle = .rounded
        supportButton.translatesAutoresizingMaskIntoConstraints = false

        let buttonStack = NSStackView(views: [logsButton, supportButton])
        buttonStack.orientation = .horizontal
        buttonStack.spacing = Spacing.md
        buttonStack.translatesAutoresizingMaskIntoConstraints = false

        contentView.addSubview(title)
        contentView.addSubview(versionLabel)
        contentView.addSubview(subtitle)
        contentView.addSubview(buttonStack)

        NSLayoutConstraint.activate([
            title.topAnchor.constraint(equalTo: contentView.topAnchor, constant: Spacing.xxl),
            title.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),

            versionLabel.topAnchor.constraint(equalTo: title.bottomAnchor, constant: Spacing.sm),
            versionLabel.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),

            subtitle.topAnchor.constraint(equalTo: versionLabel.bottomAnchor, constant: Spacing.md),
            subtitle.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),

            buttonStack.topAnchor.constraint(equalTo: subtitle.bottomAnchor, constant: Spacing.xxl),
            buttonStack.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),
            buttonStack.bottomAnchor.constraint(lessThanOrEqualTo: contentView.bottomAnchor, constant: -Spacing.xxl),
        ])

        let wc = NSWindowController(window: window)
        aboutWindowController = wc
        wc.showWindow(nil)
        window.makeKeyAndOrderFront(nil)
    }

    @objc private func openLogsDirectory() {
        NSWorkspace.shared.open(URL(fileURLWithPath: Self.logsDir, isDirectory: true))
    }

    @objc private func openAppSupportDirectory() {
        NSWorkspace.shared.open(URL(fileURLWithPath: Self.appSupportDir, isDirectory: true))
    }

    @objc private func showCompanionWindow() {
        if companionWindowController == nil {
            companionWindowController = CompanionWindowController()
        }
        companionWindowController?.showWindow(nil)
        companionWindowController?.window?.makeKeyAndOrderFront(nil)
    }

    @objc private func showNetworkDebugWindow() {
        if window == nil {
            setupNetworkDebugWindow()
            setupUI()
            checkModelOnLaunch()
        }
        window.makeKeyAndOrderFront(nil)
    }

    // MARK: - Window setup

    /// Launch the Claude Companion window as the primary window on startup.
    private func setupCompanionWindow() {
        companionWindowController = CompanionWindowController()
        companionWindowController?.showWindow(nil)
        companionWindowController?.window?.makeKeyAndOrderFront(nil)
    }

    /// Create the network debug window (on-demand via Cmd+2).
    private func setupNetworkDebugWindow() {
        let frame = NSRect(x: 200, y: 200, width: 720, height: 580)
        let style: NSWindow.StyleMask = [.titled, .closable, .miniaturizable, .resizable]
        window = NSWindow(contentRect: frame, styleMask: style, backing: .buffered, defer: false)
        window.title = "Glass Slipper — Network Debug"
        window.minSize = NSSize(width: 480, height: 360)
        window.center()
    }

    // MARK: - UI setup

    private func setupUI() {
        guard let contentView = window.contentView else { return }
        contentView.wantsLayer = true
        contentView.layer?.backgroundColor = NSColor.surfacePrimary.cgColor

        // Top bar: URL field + Diagnose button
        urlField = NSTextField()
        urlField.translatesAutoresizingMaskIntoConstraints = false
        urlField.placeholderString = "http://localhost:14094"
        urlField.bezelStyle = .roundedBezel
        urlField.font = .promptInput
        contentView.addSubview(urlField)

        diagnoseButton = NSButton(title: "Diagnose", target: self, action: #selector(diagnoseClicked))
        diagnoseButton.translatesAutoresizingMaskIntoConstraints = false
        diagnoseButton.bezelStyle = .rounded
        diagnoseButton.controlSize = .regular
        diagnoseButton.isEnabled = false
        contentView.addSubview(diagnoseButton)

        // Status bar
        statusBar = StatusBarView()
        contentView.addSubview(statusBar)

        // Spine view controller
        spineVC = SpineViewController()
        spineVC.view.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(spineVC.view)

        NSLayoutConstraint.activate([
            // URL field
            urlField.topAnchor.constraint(equalTo: contentView.topAnchor, constant: Spacing.lg),
            urlField.leadingAnchor.constraint(equalTo: contentView.leadingAnchor, constant: Spacing.lg),
            urlField.trailingAnchor.constraint(equalTo: diagnoseButton.leadingAnchor, constant: -Spacing.md),

            // Diagnose button
            diagnoseButton.topAnchor.constraint(equalTo: contentView.topAnchor, constant: Spacing.lg),
            diagnoseButton.trailingAnchor.constraint(equalTo: contentView.trailingAnchor, constant: -Spacing.lg),
            diagnoseButton.widthAnchor.constraint(greaterThanOrEqualToConstant: 100),

            // Status bar — below URL field
            statusBar.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            statusBar.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            statusBar.topAnchor.constraint(equalTo: urlField.bottomAnchor, constant: Spacing.sm),

            // Spine — below status bar
            spineVC.view.topAnchor.constraint(equalTo: statusBar.bottomAnchor),
            spineVC.view.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            spineVC.view.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            spineVC.view.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])
    }

    // MARK: - Actions

    @objc private func diagnoseClicked() {
        if isRunning {
            stopDiagnosis()
        } else {
            startDiagnosis()
        }
    }

    // MARK: - Start diagnosis

    private func startDiagnosis() {
        var url = urlField.stringValue.trimmingCharacters(in: .whitespaces)
        if url.isEmpty {
            url = "http://localhost:14094"
            urlField.stringValue = url
        }

        // Clear previous results
        spineVC = SpineViewController()
        spineVC.view.translatesAutoresizingMaskIntoConstraints = false
        if let contentView = window.contentView {
            // Remove old spine
            for sub in contentView.subviews where sub !== urlField && sub !== diagnoseButton && sub !== statusBar {
                sub.removeFromSuperview()
            }
            contentView.addSubview(spineVC.view)
            NSLayoutConstraint.activate([
                spineVC.view.topAnchor.constraint(equalTo: statusBar.bottomAnchor),
                spineVC.view.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
                spineVC.view.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
                spineVC.view.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
            ])
        }

        lineBuffer = Data()
        stderrBuffer = Data()
        worstStatus = .ok

        // Immediate feedback
        spineVC.append(.connecting)

        // Find glass-slipper binary
        guard let glassSlipperPath = findGlassSlipper() else {
            let alert = NSAlert()
            alert.messageText = "glass-slipper not found"
            alert.informativeText = "Build glass-slipper first: cargo build --release\nThen ensure it's in your PATH or next to this app."
            alert.alertStyle = .warning
            alert.runModal()
            return
        }

        let prompt = "Diagnose this URL: \(url)"
        let modelPath = modelFilePath()

        // Set status bar model name from filename
        let modelFileName = (modelPath as NSString).lastPathComponent
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

        // Build arguments
        var args = [".", "-p", prompt, "--playbook", "network-debug", "--format", "json", "--model", modelPath]
        if let llamaPath = findLlamaServer() {
            args.append(contentsOf: ["--llama-server", llamaPath])
        }

        // Launch process
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: glassSlipperPath)
        proc.arguments = args

        let stdout = Pipe()
        let stderr = Pipe()
        proc.standardOutput = stdout
        proc.standardError = stderr
        self.stdoutPipe = stdout
        self.stderrPipe = stderr

        // Read stdout
        let stdoutHandle = stdout.fileHandleForReading
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(stdoutDataAvailable),
            name: .NSFileHandleDataAvailable,
            object: stdoutHandle
        )
        stdoutHandle.waitForDataInBackgroundAndNotify()

        // Read stderr
        let stderrHandle = stderr.fileHandleForReading
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(stderrDataAvailable),
            name: .NSFileHandleDataAvailable,
            object: stderrHandle
        )
        stderrHandle.waitForDataInBackgroundAndNotify()

        // Termination handler
        proc.terminationHandler = { [weak self] task in
            DispatchQueue.main.async {
                self?.taskDidTerminate(task)
            }
        }

        do {
            try proc.run()
        } catch {
            let alert = NSAlert()
            alert.messageText = "Failed to launch glass-slipper"
            alert.informativeText = error.localizedDescription
            alert.alertStyle = .warning
            alert.runModal()
            return
        }

        self.process = proc
        isRunning = true
        diagnoseButton.title = "Stop"

        // Open JSONL log file in persistent logs directory
        let logPath = Self.logsDir + "/glass-slipper.jsonl"
        FileManager.default.createFile(atPath: logPath, contents: nil)
        logFileHandle = FileHandle(forWritingAtPath: logPath)
        logFileHandle?.seekToEndOfFile()
        NSLog("Glass Slipper log: %@", logPath)

        AppLogger.log("diagnosis_start", ["url": url])
    }

    // MARK: - Stop diagnosis

    private func stopDiagnosis() {
        guard let proc = process, proc.isRunning else { return }
        proc.terminate()
        // SIGKILL fallback if SIGTERM doesn't work within 3 seconds
        let pid = proc.processIdentifier
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.0) { [weak self] in
            if let p = self?.process, p.isRunning {
                NSLog("Glass Slipper: SIGTERM ignored, sending SIGKILL to pid %d", pid)
                kill(pid, SIGKILL)
            }
        }
    }

    // MARK: - Find binaries

    private func findGlassSlipper() -> String? {
        let bundled = Bundle.main.bundlePath + "/Contents/MacOS/glass-slipper-agent"
        if FileManager.default.isExecutableFile(atPath: bundled) {
            return bundled
        }
        return nil
    }

    private func findLlamaServer() -> String? {
        let bundled = Bundle.main.bundlePath + "/Contents/MacOS/llama-server"
        if FileManager.default.isExecutableFile(atPath: bundled) {
            return bundled
        }
        return nil
    }

    // MARK: - Kill llama-server on quit

    /// Find and kill any llama-server process listening on port 8787.
    @discardableResult
    private func killLlamaServer() -> Bool {
        let lsof = Process()
        lsof.executableURL = URL(fileURLWithPath: "/usr/sbin/lsof")
        lsof.arguments = ["-ti", ":\(LlamaServerManager.port)"]
        let pipe = Pipe()
        lsof.standardOutput = pipe
        lsof.standardError = Pipe()
        do {
            try lsof.run()
            lsof.waitUntilExit()
        } catch { return false }

        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        guard let output = String(data: data, encoding: .utf8) else { return false }

        var killed = false
        for pidStr in output.split(separator: "\n") {
            if let pid = Int32(pidStr.trimmingCharacters(in: .whitespaces)) {
                // Safety: verify the process is actually llama-server before killing
                let ps = Process()
                ps.executableURL = URL(fileURLWithPath: "/bin/ps")
                ps.arguments = ["-p", "\(pid)", "-o", "comm="]
                let psPipe = Pipe()
                ps.standardOutput = psPipe
                ps.standardError = Pipe()
                if let _ = try? ps.run() {
                    ps.waitUntilExit()
                    let psData = psPipe.fileHandleForReading.readDataToEndOfFile()
                    let comm = String(data: psData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                    guard comm.contains("llama") else {
                        NSLog("Glass Slipper: pid %d on port 8787 is '%@', not llama-server — skipping", pid, comm)
                        continue
                    }
                }
                NSLog("Glass Slipper: sending SIGTERM to llama-server pid %d", pid)
                kill(pid, SIGTERM)
                killed = true
            }
        }
        return killed
    }

    // MARK: - Model path

    private func modelFilePath() -> String {
        LlamaServerManager.modelFilePath()
    }

    // MARK: - Pipe reading

    @objc private func stdoutDataAvailable(_ notification: Notification) {
        guard let handle = notification.object as? FileHandle else { return }
        let data = handle.availableData

        if data.isEmpty { return } // EOF

        lineBuffer.append(data)
        processLineBuffer()
        handle.waitForDataInBackgroundAndNotify()
    }

    @objc private func stderrDataAvailable(_ notification: Notification) {
        guard let handle = notification.object as? FileHandle else { return }
        let data = handle.availableData
        if !data.isEmpty {
            stderrBuffer.append(data)
            handle.waitForDataInBackgroundAndNotify()
        }
    }

    private func processLineBuffer() {
        guard let bufStr = String(data: lineBuffer, encoding: .utf8) else {
            NSLog("Glass Slipper: UTF-8 decode failed (%lu bytes), clearing buffer", lineBuffer.count)
            lineBuffer = Data()
            return
        }

        let lines = bufStr.components(separatedBy: "\n")
        if lines.count <= 1 { return } // No complete line yet

        // Process all complete lines (everything except the last fragment)
        for i in 0 ..< lines.count - 1 {
            let line = lines[i]
            if !line.isEmpty {
                processJSONLine(line)
            }
        }

        // Keep the last (incomplete) fragment in the buffer
        let remainder = lines.last ?? ""
        lineBuffer = remainder.data(using: .utf8) ?? Data()
    }

    // MARK: - JSON parsing

    private func processJSONLine(_ line: String) {
        // Tee to log file for tail -f
        if let handle = logFileHandle, let data = (line + "\n").data(using: .utf8) {
            handle.write(data)
            handle.synchronizeFile()
        }

        guard let jsonData = line.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any],
              let eventType = json["event"] as? String else {
            NSLog("Glass Slipper: invalid JSON line: %@", line)
            return
        }

        // NSFileHandle notifications are delivered on the thread that called
        // waitForDataInBackgroundAndNotify, which is main. No async hop needed.
        handleEvent(json, type: eventType)
    }

    // MARK: - Event dispatch

    private func handleEvent(_ event: [String: Any], type eventType: String) {
        switch eventType {
        case "hw_info":
            let chip = event["chip"] as? String ?? "Unknown"
            let ramUsed = event["ram_used_gb"] as? Double ?? 0
            let ramTotal = event["ram_total_gb"] as? Double ?? 0
            let gpu = event["gpu_layers"] as? String ?? "—"
            spineVC.append(.hwInfo(chip: chip, ramUsed: ramUsed, ramTotal: ramTotal, gpu: gpu))
            statusBar.setHealthState(.normal)

        case "user_prompt":
            let text = event["text"] as? String ?? ""
            spineVC.append(.userPrompt(text: text))

        case "plan":
            let items = event["items"] as? [String] ?? []
            spineVC.append(.plan(items: items))

        case "step_complete":
            let name = event["step"] as? String ?? "unknown"
            let statusStr = event["status"] as? String ?? "pass"
            let summary = event["summary"] as? String ?? ""
            let detail = event["detail"] as? String ?? summary
            let status: EventStatus
            switch statusStr {
            case "pass": status = .ok
            case "fail": status = .err
            case "warn": status = .warn
            default: status = .info
            }
            // Track worst status (skip synthesis — it carries the aggregate from Rust)
            if name != "synthesis" {
                if status == .err { worstStatus = .err }
                else if status == .warn && worstStatus != .err { worstStatus = .warn }
            }
            let title = stepDisplayTitle(name)
            spineVC.append(.check(name: title, status: status, detail: detail))

        case "thinking":
            let content = event["content"] as? String ?? ""
            if !content.trimmingCharacters(in: .whitespaces).isEmpty {
                spineVC.append(.thought(text: content))
            }

        case "diagnosis":
            let text = event["text"] as? String ?? ""
            spineVC.append(.diagnosis(text: text, status: worstStatus))

        case "done":
            isRunning = false
            diagnoseButton.title = "Diagnose"

        case "token_rate":
            let rate = event["tok_per_sec"] as? Double ?? 0
            statusBar.setTokPerSec(rate)

        case "memory_warning":
            if let until = warningDismissedUntil, Date() < until { break }
            let pageoutRate = event["pageout_rate"] as? UInt64
                ?? (event["pageout_rate"] as? Int).map(UInt64.init) ?? 0
            let swapUsedMB = event["swap_used_mb"] as? Double ?? 0
            let tokPerSec = event["tok_per_sec"] as? Double
            statusBar.setHealthState(.warning)
            spineVC.append(.memoryWarning(pageoutRate: pageoutRate, swapUsedMB: swapUsedMB, tokPerSec: tokPerSec))
            if let banner = spineVC.lastAddedView as? WarningBannerView {
                banner.delegate = self
            }
            logAppEvent("warning_shown", details: ["pageout_rate": pageoutRate, "swap_used_mb": swapUsedMB])

        case "model_swap":
            let fromModel = event["from_model"] as? String ?? ""
            let toModel = event["to_model"] as? String ?? ""
            let reason = event["reason"] as? String ?? ""
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
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) { [weak self] in
                self?.statusBar.setHealthState(.normal)
            }
            logAppEvent("model_swap", details: ["from": fromModel, "to": toModel, "reason": reason])

        case "promotion_available":
            if let until = promotionDismissedUntil, Date() < until { break }
            let toModel = event["to_model"] as? String ?? ""
            spineVC.append(.promotionAvailable(toModel: toModel))
            if let banner = spineVC.lastAddedView as? PromotionBannerView {
                banner.delegate = self
            }
            logAppEvent("promotion_shown", details: ["to_model": toModel])

        case "warning":
            NSLog("Glass Slipper warning: %@", event["message"] as? String ?? "")

        default:
            break // step_start, text, tool_start, tool_done — ignore
        }
    }

    /// Map step IDs to display titles (matches Rust step_display_title).
    private func stepDisplayTitle(_ step: String) -> String {
        switch step {
        case "parse_target": return "Parse Target"
        case "dns": return "DNS Resolution"
        case "connectivity": return "Connectivity Check"
        case "route_analysis": return "Route Analysis"
        case "port_check": return "Port Check"
        case "service_check": return "Service Check"
        case "synthesis": return "Diagnosis"
        default: return step
        }
    }

    private func logAppEvent(_ eventName: String, details: [String: Any]) {
        AppLogger.log(eventName, details)
    }

    // MARK: - Termination

    private func taskDidTerminate(_ task: Process) {
        // NOTE: Potential race — if this handler fires before the final stdout
        // notification, removeObserver drops the last chunk. In practice NSFileHandle
        // notifications are delivered before the termination handler on the main queue.
        NotificationCenter.default.removeObserver(self, name: .NSFileHandleDataAvailable, object: nil)

        isRunning = false
        diagnoseButton.title = "Diagnose"

        if task.terminationStatus != 0 {
            let stderrText = (String(data: stderrBuffer, encoding: .utf8) ?? "Unknown error")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let errorMsg = stderrText.isEmpty
                ? "glass-slipper exited with code \(task.terminationStatus)"
                : stderrText
            spineVC.append(.check(name: "Error", status: .err, detail: errorMsg))
        }

        logFileHandle?.closeFile()
        logFileHandle = nil

        AppLogger.log("diagnosis_end", ["exit_code": task.terminationStatus])
    }
}

// MARK: - ModelDownloadManagerDelegate

extension AppDelegate: ModelDownloadManagerDelegate {
    func downloadDidUpdateProgress(downloaded: Int64, total: Int64) {
        downloadRowView?.showProgress(downloaded: downloaded, total: total)
    }

    func downloadDidBeginVerification() {
        downloadRowView?.showVerifying()
    }

    func downloadDidFinish() {
        downloadRowView?.removeFromSuperview()
        diagnoseButton.isEnabled = true
        spineVC.view.isHidden = false
    }

    func downloadDidFail(error: String) {
        downloadRowView?.showError(error)
        downloadRowView?.onAction = { [weak self] in
            self?.handleDownloadAction()
        }
    }
}

// MARK: - Click-to-copy extension for SpineViewController

extension SpineViewController {
    /// Add click gesture to a row view for copy-to-clipboard.
    func addClickToCopy(to rowView: NSView, text: String) {
        let click = NSClickGestureRecognizer(target: self, action: #selector(rowClicked(_:)))
        rowView.addGestureRecognizer(click)
        copyTextByView[ObjectIdentifier(rowView)] = text
    }

    @objc func rowClicked(_ gesture: NSClickGestureRecognizer) {
        guard let view = gesture.view,
              let text = copyTextByView[ObjectIdentifier(view)],
              !text.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}

// MARK: - MemoryBannerDelegate

extension AppDelegate: MemoryBannerDelegate {
    func warningBannerDidRequestSwap() {
        logAppEvent("warning_swap_requested", details: [:])
        NSLog("Glass Slipper: user requested model swap from warning banner")
    }

    func warningBannerDidDismiss() {
        warningDismissedUntil = Date().addingTimeInterval(5 * 60)
        statusBar.setHealthState(.normal)
        logAppEvent("warning_dismissed", details: [:])
    }

    func promotionBannerDidAccept() {
        logAppEvent("promotion_accepted", details: [:])
        NSLog("Glass Slipper: user accepted promotion")
    }

    func promotionBannerDidDismiss() {
        promotionDismissedUntil = Date().addingTimeInterval(15 * 60)
        logAppEvent("promotion_dismissed", details: [:])
    }
}
