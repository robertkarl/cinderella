//
//  CodexMCPInstaller.swift
//  Glass Slipper -- Codex CLI MCP configuration
//
//  Shells out to `codex mcp add/remove/get` for install, uninstall,
//  and state checking. All Process calls run on a background queue
//  with a timeout to avoid blocking the UI.
//

import Foundation

enum CodexMCPInstaller {

    /// Cached install state for synchronous UI reads.
    /// Updated asynchronously by `refreshState()`.
    private(set) static var isInstalled: Bool = false

    /// Timeout for all codex CLI calls (seconds).
    private static let processTimeout: TimeInterval = 10

    // MARK: - Binary Detection

    /// Probe common paths for the codex binary. macOS GUI apps get a
    /// minimal PATH, so `which` alone fails for Dock-launched apps.
    static func findCodexBinary() -> String? {
        let candidates = [
            "/usr/local/bin/codex",
            NSHomeDirectory() + "/.local/bin/codex",
            "/opt/homebrew/bin/codex",
        ]
        for path in candidates {
            if FileManager.default.isExecutableFile(atPath: path) {
                return path
            }
        }
        // Last resort: `which codex` (works if launched from terminal)
        let (output, exitCode) = runProcessSync(
            executablePath: "/usr/bin/which",
            arguments: ["codex"],
            timeout: 5
        )
        if exitCode == 0, let path = output?.trimmingCharacters(in: .whitespacesAndNewlines),
           !path.isEmpty, FileManager.default.isExecutableFile(atPath: path) {
            return path
        }
        return nil
    }

    /// Whether the codex binary can be found on this machine.
    static var isCodexAvailable: Bool {
        findCodexBinary() != nil
    }

    // MARK: - Install

    /// Install glass-slipper MCP into Codex CLI config.
    /// Calls completion on the main queue with nil on success or an error message.
    static func install(completion: @escaping (String?) -> Void) {
        guard let codexPath = findCodexBinary() else {
            DispatchQueue.main.async {
                completion("Codex CLI not found. Install it or use the Terminal fallback.")
            }
            return
        }

        DispatchQueue.global(qos: .userInitiated).async {
            let (_, exitCode) = runProcessSync(
                executablePath: codexPath,
                arguments: ["mcp", "add", "glass-slipper", "--", MCPInstaller.mcpBinaryPath],
                timeout: processTimeout
            )

            if exitCode != 0 {
                DispatchQueue.main.async {
                    completion("codex mcp add failed (exit \(exitCode))")
                }
                return
            }

            // Post-install verify: confirm binary path matches
            let (getOutput, getExit) = runProcessSync(
                executablePath: codexPath,
                arguments: ["mcp", "get", "glass-slipper"],
                timeout: processTimeout
            )

            if getExit != 0 {
                DispatchQueue.main.async {
                    completion("Install appeared to succeed but verification failed (exit \(getExit))")
                }
                return
            }

            // Check that the output references our binary path
            if let output = getOutput, !output.contains("glass-slipper-mcp") {
                DispatchQueue.main.async {
                    completion("Install verification: binary path mismatch in codex config")
                }
                return
            }

            DispatchQueue.main.async {
                isInstalled = true
                completion(nil)
            }
        }
    }

    // MARK: - Uninstall

    /// Remove glass-slipper MCP from Codex CLI config.
    /// Calls completion on the main queue with nil on success or an error message.
    static func uninstall(completion: @escaping (String?) -> Void) {
        guard let codexPath = findCodexBinary() else {
            DispatchQueue.main.async {
                completion("Codex CLI not found")
            }
            return
        }

        DispatchQueue.global(qos: .userInitiated).async {
            let (_, exitCode) = runProcessSync(
                executablePath: codexPath,
                arguments: ["mcp", "remove", "glass-slipper"],
                timeout: processTimeout
            )

            DispatchQueue.main.async {
                if exitCode == 0 {
                    isInstalled = false
                    completion(nil)
                } else {
                    completion("codex mcp remove failed (exit \(exitCode))")
                }
            }
        }
    }

    // MARK: - State Refresh

    /// Async check of actual install state via `codex mcp get`.
    /// Updates `isInstalled` on the main queue when done.
    static func refreshState() {
        guard let codexPath = findCodexBinary() else {
            DispatchQueue.main.async { isInstalled = false }
            return
        }

        DispatchQueue.global(qos: .utility).async {
            let (_, exitCode) = runProcessSync(
                executablePath: codexPath,
                arguments: ["mcp", "get", "glass-slipper"],
                timeout: processTimeout
            )

            DispatchQueue.main.async {
                isInstalled = (exitCode == 0)
            }
        }
    }

    // MARK: - Terminal Fallback

    /// Open Terminal.app with the `codex mcp add` command pre-filled.
    /// Used when codex can't be found via path probing.
    static func openTerminalFallback() {
        let binaryPath = MCPInstaller.mcpBinaryPath
        let command = "codex mcp add glass-slipper -- '\(binaryPath)'"
        let script = "tell application \"Terminal\" to do script \"\(command)\""

        if let appleScript = NSAppleScript(source: script) {
            var error: NSDictionary?
            appleScript.executeAndReturnError(&error)
        }
    }

    // MARK: - Private

    /// Run a Process synchronously with a timeout. Returns (stdout, exitCode).
    /// exitCode is -1 on timeout.
    private static func runProcessSync(
        executablePath: String,
        arguments: [String],
        timeout: TimeInterval
    ) -> (String?, Int32) {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: executablePath)
        proc.arguments = arguments

        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = FileHandle.nullDevice

        do {
            try proc.run()
        } catch {
            return (nil, -1)
        }

        // Wait with timeout
        let deadline = Date().addingTimeInterval(timeout)
        while proc.isRunning && Date() < deadline {
            Thread.sleep(forTimeInterval: 0.1)
        }

        if proc.isRunning {
            proc.terminate()
            return (nil, -1)
        }

        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        let output = String(data: data, encoding: .utf8)
        return (output, proc.terminationStatus)
    }
}
