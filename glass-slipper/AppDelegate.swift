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

    // MARK: - Application lifecycle

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupMenuBar()
        setupWindow()
        setupUI()
        checkModelOnLaunch()
        window.makeKeyAndOrderFront(nil)
        if #available(macOS 14.0, *) {
            NSApp.activate()
        } else {
            NSApp.activate(ignoringOtherApps: true)
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationWillTerminate(_ notification: Notification) {
        if let proc = process, proc.isRunning {
            proc.terminate()
        }
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

        NSApp.mainMenu = menubar
    }

    // MARK: - Window setup

    private func setupWindow() {
        let frame = NSRect(x: 200, y: 200, width: 720, height: 580)
        let style: NSWindow.StyleMask = [.titled, .closable, .miniaturizable, .resizable]
        window = NSWindow(contentRect: frame, styleMask: style, backing: .buffered, defer: false)
        window.title = "Glass Slipper"
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

            // Spine — fills remaining space below top bar
            spineVC.view.topAnchor.constraint(equalTo: urlField.bottomAnchor, constant: Spacing.lg),
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
            for sub in contentView.subviews where sub !== urlField && sub !== diagnoseButton {
                sub.removeFromSuperview()
            }
            contentView.addSubview(spineVC.view)
            NSLayoutConstraint.activate([
                spineVC.view.topAnchor.constraint(equalTo: urlField.bottomAnchor, constant: Spacing.lg),
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

        // Open JSONL log file for tail -f debugging (after proc.run succeeds)
        let logPath = NSTemporaryDirectory() + "glass-slipper.jsonl"
        FileManager.default.createFile(atPath: logPath, contents: nil)
        logFileHandle = FileHandle(forWritingAtPath: logPath)
        logFileHandle?.truncateFile(atOffset: 0)
        NSLog("Glass Slipper log: %@", logPath)
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

    /// Whether we are running from inside an app bundle (release mode).
    private var isReleaseBuild: Bool {
        // In a proper app bundle, the executable is at .app/Contents/MacOS/Glass Slipper
        let exe = Bundle.main.executablePath ?? ""
        return exe.contains(".app/Contents/MacOS/")
    }

    private func findGlassSlipper() -> String? {
        // Primary: bundled helper inside app bundle (named glass-slipper-agent to avoid
        // case-insensitive collision with the Swift "Glass Slipper" executable)
        let bundled = Bundle.main.bundlePath + "/Contents/MacOS/glass-slipper-agent"
        if FileManager.default.isExecutableFile(atPath: bundled) {
            return bundled
        }

        // Release mode: fail closed — don't fall back to PATH or Cargo outputs
        if isReleaseBuild {
            return nil
        }

        // --- Development-only fallbacks ---
        let appDir = Bundle.main.bundlePath
        let parentDir = (appDir as NSString).deletingLastPathComponent

        // Adjacent to app
        let adjacent = (parentDir as NSString).appendingPathComponent("glass-slipper")
        if FileManager.default.isExecutableFile(atPath: adjacent) {
            return adjacent
        }

        // Cargo debug build
        let cargoDebug = ((parentDir as NSString).appendingPathComponent("../target/debug/glass-slipper") as NSString).standardizingPath
        if FileManager.default.isExecutableFile(atPath: cargoDebug) {
            return cargoDebug
        }

        // Cargo release build
        let cargoRelease = ((parentDir as NSString).appendingPathComponent("../target/release/glass-slipper") as NSString).standardizingPath
        if FileManager.default.isExecutableFile(atPath: cargoRelease) {
            return cargoRelease
        }

        // Check PATH via which
        let which = Process()
        which.executableURL = URL(fileURLWithPath: "/usr/bin/which")
        which.arguments = ["glass-slipper"]
        let whichPipe = Pipe()
        which.standardOutput = whichPipe
        which.standardError = Pipe()
        do {
            try which.run()
            which.waitUntilExit()
            let data = whichPipe.fileHandleForReading.readDataToEndOfFile()
            let path = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if !path.isEmpty && FileManager.default.isExecutableFile(atPath: path) {
                return path
            }
        } catch {}

        return nil
    }

    private func findLlamaServer() -> String? {
        // Primary: bundled helper inside app bundle
        let bundled = Bundle.main.bundlePath + "/Contents/MacOS/llama-server"
        if FileManager.default.isExecutableFile(atPath: bundled) {
            return bundled
        }

        // Release mode: fail closed — no Homebrew fallback
        if isReleaseBuild {
            return nil
        }

        // --- Development-only fallbacks ---
        let candidates = [
            "/opt/homebrew/bin/llama-server",
            "/usr/local/bin/llama-server",
        ]
        for path in candidates {
            if FileManager.default.isExecutableFile(atPath: path) {
                return path
            }
        }
        return nil
    }

    // MARK: - Model path

    /// Model file path: ~/Library/Application Support/Glass Slipper/Models/Qwen3.5-9B-Q5_K_M.gguf
    /// In development, falls back to ~/models/ if Application Support copy doesn't exist.
    private func modelFilePath() -> String {
        let home = NSHomeDirectory()
        let appSupportPath = home + "/Library/Application Support/Glass Slipper/Models/Qwen3.5-9B-Q5_K_M.gguf"
        if FileManager.default.fileExists(atPath: appSupportPath) {
            return appSupportPath
        }
        // Development fallback (not used in release builds)
        if !isReleaseBuild {
            let legacyPath = home + "/models/Qwen3.5-9B-Q5_K_M.gguf"
            if FileManager.default.fileExists(atPath: legacyPath) {
                return legacyPath
            }
        }
        // Return the canonical path even if missing — the Rust helper will report the error
        return appSupportPath
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
