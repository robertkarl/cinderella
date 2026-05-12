# LlamaServerManager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire up the Companion "Start Server" button to actually launch llama-server, auto-start on launch if model is downloaded, and make the logic testable.

**Architecture:** First, fix the Xcode build phase to embed all three binaries (glass-slipper-agent, glass-slipper-mcp, llama-server) into the app bundle so dev and release builds both find them at the same path. Delete all homebrew/PATH/cargo-output fallbacks. Then extract a `LlamaServerManager` class that owns the llama-server `Process` lifecycle — find binary, find model, spawn, poll health, report state. CompanionWindowController calls it from `handleServerStart()` and on init. AppDelegate uses it for quit cleanup too.

**Tech Stack:** Swift, Foundation (Process, URLSession), XCTest

---

### Task 0: Embed all binaries in Xcode build phase, delete dev fallbacks

**Files:**
- Modify: `glass-slipper/GlassSlipper.xcodeproj/project.pbxproj` (the "Copy Glass Slipper" shell script)
- Modify: `glass-slipper/AppDelegate.swift` (delete `findGlassSlipper` dev fallbacks, delete `findLlamaServer` dev fallbacks, delete `isReleaseBuild`)

The "Copy Glass Slipper" build phase currently symlinks glass-slipper adjacent to the .app and copies glass-slipper-mcp into the bundle. It does NOT copy glass-slipper-agent or llama-server into the bundle. Fix it to copy all three into `Contents/MacOS/`.

- [ ] **Step 1: Create the build step shell script**

Create `glass-slipper/copy-helpers.sh`:

```bash
#!/usr/bin/env bash
#
# copy-helpers.sh — Copy Rust helpers and llama-server into the app bundle.
# Called by the "Copy Glass Slipper" Xcode build phase.
#
set -euo pipefail

MACOS_DEST="${BUILT_PRODUCTS_DIR}/${PRODUCT_NAME}.app/Contents/MacOS"

# glass-slipper-agent (the Rust CLI, renamed to avoid APFS collision)
AGENT="${SRCROOT}/../target/release/glass-slipper"
if [ -x "$AGENT" ]; then
    cp -f "$AGENT" "$MACOS_DEST/glass-slipper-agent"
    echo "Copied glass-slipper-agent into app bundle"
else
    echo "warning: glass-slipper not found at $AGENT — run: cargo build --release"
fi

# glass-slipper-mcp
MCP="${SRCROOT}/../target/release/glass-slipper-mcp"
if [ -x "$MCP" ]; then
    cp -f "$MCP" "$MACOS_DEST/glass-slipper-mcp"
    echo "Copied glass-slipper-mcp into app bundle"
else
    echo "warning: glass-slipper-mcp not found at $MCP — run: cargo build --release"
fi

# llama-server (pre-built arm64 binary from build-llama.sh)
LLAMA="${SRCROOT}/../build/llama-server"
if [ -x "$LLAMA" ]; then
    cp -f "$LLAMA" "$MACOS_DEST/llama-server"
    echo "Copied llama-server into app bundle"
else
    echo "warning: llama-server not found at $LLAMA — run: scripts/build-llama.sh"
fi
```

Make it executable: `chmod +x glass-slipper/copy-helpers.sh`

- [ ] **Step 2: Update the Xcode build phase to call the script**

In `project.pbxproj`, replace the `shellScript` for the "Copy Glass Slipper" phase (object A3000003, line 200) with:

```
echo "Running copy-helpers step"\n"${SRCROOT}/copy-helpers.sh"
```

- [ ] **Step 2: Delete dev fallbacks from findGlassSlipper()**

In `AppDelegate.swift`, replace `findGlassSlipper()` (currently ~60 lines with adjacent, cargo debug, cargo release, and `which` fallbacks) with:

```swift
    private func findGlassSlipper() -> String? {
        let bundled = Bundle.main.bundlePath + "/Contents/MacOS/glass-slipper-agent"
        if FileManager.default.isExecutableFile(atPath: bundled) {
            return bundled
        }
        return nil
    }
```

- [ ] **Step 3: Delete dev fallbacks from findLlamaServer()**

Replace `findLlamaServer()` with:

```swift
    private func findLlamaServer() -> String? {
        let bundled = Bundle.main.bundlePath + "/Contents/MacOS/llama-server"
        if FileManager.default.isExecutableFile(atPath: bundled) {
            return bundled
        }
        return nil
    }
```

- [ ] **Step 4: Delete `isReleaseBuild` property**

Remove the `isReleaseBuild` computed property entirely — it's no longer used since there are no dev/release branches.

Also delete any remaining references to `isReleaseBuild` in `modelFilePath()` — simplify it to always use the App Support path:

```swift
    private func modelFilePath() -> String {
        let home = NSHomeDirectory()
        let appSupportPath = home + "/Library/Application Support/Glass Slipper/Models/Qwen3.5-9B-Q5_K_M.gguf"
        if FileManager.default.fileExists(atPath: appSupportPath) {
            return appSupportPath
        }
        // Legacy path for development machines with models in ~/models/
        let legacyPath = home + "/models/Qwen3.5-9B-Q5_K_M.gguf"
        if FileManager.default.fileExists(atPath: legacyPath) {
            return legacyPath
        }
        return appSupportPath
    }
```

- [ ] **Step 5: Build and verify all three binaries are in the bundle**

Run: `xcodebuild build -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper 2>&1 | grep -E "(error:|BUILD |Copied|warning:.*not found)"`

Then verify: `ls -la build/DerivedData/Build/Products/Release/GlassSlipper.app/Contents/MacOS/`

Expected: `GlassSlipper`, `glass-slipper-agent`, `glass-slipper-mcp`, `llama-server` all present.

- [ ] **Step 6: Run tests**

Run: `xcodebuild test -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper 2>&1 | grep -E "(Test Case|test result)"`

Expected: All 9 existing tests PASS.

- [ ] **Step 7: Commit**

```bash
git add glass-slipper/copy-helpers.sh glass-slipper/GlassSlipper.xcodeproj/project.pbxproj glass-slipper/AppDelegate.swift
git commit -m "fix: embed all binaries in Xcode build, delete homebrew/PATH fallbacks"
```

---

### Task 1: Create LlamaServerManager with binary/model resolution and argument building

**Files:**
- Create: `glass-slipper/LlamaServerManager.swift`
- Create: `glass-slipper/GlassSlipperTests/LlamaServerManagerTests.swift`

This task builds the testable core — path resolution and argument construction — without spawning anything.

- [ ] **Step 1: Write failing tests for binary resolution and argument building**

In `glass-slipper/GlassSlipperTests/LlamaServerManagerTests.swift`:

```swift
import XCTest

class LlamaServerManagerTests: XCTestCase {

    func testBuildArgumentsIncludesContextSize() {
        let args = LlamaServerManager.buildArguments(modelPath: "/tmp/model.gguf", port: 8787)
        guard let idx = args.firstIndex(of: "--ctx-size") else {
            XCTFail("--ctx-size not found in arguments")
            return
        }
        XCTAssertEqual(args[idx + 1], "32768")
    }

    func testFindModelPathReturnsAppSupportLocation() {
        let path = LlamaServerManager.modelFilePath()
        XCTAssertTrue(path.contains("Library/Application Support/Glass Slipper/Models"))
    }

    func testStateStartsAsNotRunning() {
        let manager = LlamaServerManager()
        XCTAssertEqual(manager.state, .notRunning)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild test -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper 2>&1 | grep -E "(Test Case|test result|error:)"`

Expected: Compilation error — `LlamaServerManager` not defined.

- [ ] **Step 3: Write LlamaServerManager with resolution and args**

In `glass-slipper/LlamaServerManager.swift`:

```swift
//
//  LlamaServerManager.swift
//  Glass Slipper — manages the llama-server lifecycle
//

import Foundation

enum LlamaServerState: Equatable {
    case notRunning
    case starting
    case running
    case failed(String)
}

protocol LlamaServerManagerDelegate: AnyObject {
    func serverStateDidChange(_ state: LlamaServerState)
}

final class LlamaServerManager {

    weak var delegate: LlamaServerManagerDelegate?
    private(set) var state: LlamaServerState = .notRunning

    private var process: Process?
    private var healthTimer: Timer?

    static let binaryName = "llama-server"
    static let port: UInt16 = 8787
    static let healthURL = URL(string: "http://127.0.0.1:8787/health")!

    // MARK: - Binary resolution (static for testability)

    static func findLlamaServer(bundle: Bundle = .main) -> String? {
        let bundled = bundle.bundlePath + "/Contents/MacOS/\(binaryName)"
        if FileManager.default.isExecutableFile(atPath: bundled) {
            return bundled
        }
        return nil
    }

    static func modelFilePath() -> String {
        let home = NSHomeDirectory()
        let appSupportPath = home + "/Library/Application Support/Glass Slipper/Models/Qwen3.5-9B-Q5_K_M.gguf"
        if FileManager.default.fileExists(atPath: appSupportPath) {
            return appSupportPath
        }
        let legacyPath = home + "/models/Qwen3.5-9B-Q5_K_M.gguf"
        if FileManager.default.fileExists(atPath: legacyPath) {
            return legacyPath
        }
        return appSupportPath
    }

    static func buildArguments(modelPath: String, port: UInt16 = 8787) -> [String] {
        return [
            "--model", modelPath,
            "--port", "\(port)",
            "--host", "127.0.0.1",
            "--ctx-size", "32768",
            "--n-gpu-layers", "-1",
            "--jinja",
        ]
    }
}
```

- [ ] **Step 4: Add LlamaServerManager.swift to the Xcode project**

Add the new file to the GlassSlipper target in `project.pbxproj`. Also add the test file to the GlassSlipperTests target.

- [ ] **Step 5: Run tests to verify they pass**

Run: `xcodebuild test -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper 2>&1 | grep -E "(Test Case|test result)"`

Expected: All 3 new tests PASS, all 9 existing tests PASS.

- [ ] **Step 6: Commit**

```bash
git add glass-slipper/LlamaServerManager.swift glass-slipper/GlassSlipperTests/LlamaServerManagerTests.swift glass-slipper/GlassSlipper.xcodeproj/project.pbxproj
git commit -m "feat: add LlamaServerManager with binary/model resolution and arg building"
```

---

### Task 2: Add start/stop/health-poll to LlamaServerManager

**Files:**
- Modify: `glass-slipper/LlamaServerManager.swift`
- Modify: `glass-slipper/GlassSlipperTests/LlamaServerManagerTests.swift`

This task adds the Process lifecycle — start, health polling, stop, and state transitions.

- [ ] **Step 1: Write failing tests for start and state transitions**

Append to `LlamaServerManagerTests.swift`:

```swift
    func testStartWithMissingBinaryTransitionsToFailed() {
        let manager = LlamaServerManager()
        manager.start(binaryPath: "/nonexistent/llama-server", modelPath: "/tmp/model.gguf")
        XCTAssertEqual(manager.state, .failed("llama-server not found"))
    }

    func testStartWithMissingModelTransitionsToFailed() {
        let manager = LlamaServerManager()
        manager.start(binaryPath: nil, modelPath: "/nonexistent/model.gguf")
        XCTAssertEqual(manager.state, .failed("llama-server not found"))
    }

    func testStopFromNotRunningIsNoOp() {
        let manager = LlamaServerManager()
        manager.stop()
        XCTAssertEqual(manager.state, .notRunning)
    }

    func testDelegateCalledOnStateChange() {
        let manager = LlamaServerManager()
        let spy = StateSpy()
        manager.delegate = spy
        manager.start(binaryPath: "/nonexistent/llama-server", modelPath: "/tmp/model.gguf")
        XCTAssertEqual(spy.states.last, .failed("llama-server not found"))
    }
}

// MARK: - Test helpers

class StateSpy: LlamaServerManagerDelegate {
    var states: [LlamaServerState] = []
    func serverStateDidChange(_ state: LlamaServerState) {
        states.append(state)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Expected: Compilation error — `start(binaryPath:modelPath:)` and `stop()` not defined.

- [ ] **Step 3: Implement start/stop/health-poll**

Add to `LlamaServerManager`:

```swift
    // MARK: - Lifecycle

    /// Start llama-server. Pass explicit paths or nil to auto-resolve.
    func start(binaryPath: String? = nil, modelPath: String? = nil) {
        let binary = binaryPath ?? Self.findLlamaServer()
        guard let binary = binary, FileManager.default.isExecutableFile(atPath: binary) else {
            setState(.failed("llama-server not found"))
            return
        }

        let model = modelPath ?? Self.modelFilePath()
        guard FileManager.default.fileExists(atPath: model) else {
            setState(.failed("Model not found"))
            return
        }

        setState(.starting)

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: binary)
        proc.arguments = Self.buildArguments(modelPath: model)
        proc.standardOutput = FileHandle.nullDevice
        proc.standardError = FileHandle.nullDevice

        proc.terminationHandler = { [weak self] task in
            DispatchQueue.main.async {
                guard let self = self, self.process === task else { return }
                self.healthTimer?.invalidate()
                self.healthTimer = nil
                self.process = nil
                if self.state != .notRunning {
                    self.setState(.failed("llama-server exited with code \(task.terminationStatus)"))
                }
            }
        }

        do {
            try proc.run()
        } catch {
            setState(.failed("Failed to launch: \(error.localizedDescription)"))
            return
        }

        self.process = proc
        NSLog("Glass Slipper: launched llama-server pid %d", proc.processIdentifier)
        startHealthPolling()
    }

    func stop() {
        healthTimer?.invalidate()
        healthTimer = nil
        guard let proc = process, proc.isRunning else { return }
        proc.terminate()
        process = nil
        setState(.notRunning)
    }

    // MARK: - Health polling

    private func startHealthPolling() {
        healthTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            self?.checkHealth()
        }
    }

    private func checkHealth() {
        var request = URLRequest(url: Self.healthURL)
        request.timeoutInterval = 2
        URLSession.shared.dataTask(with: request) { [weak self] _, response, _ in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if let http = response as? HTTPURLResponse, http.statusCode == 200 {
                    self.healthTimer?.invalidate()
                    self.healthTimer = nil
                    self.setState(.running)
                }
            }
        }.resume()
    }

    private func setState(_ newState: LlamaServerState) {
        state = newState
        delegate?.serverStateDidChange(newState)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild test -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper 2>&1 | grep -E "(Test Case|test result)"`

Expected: All 7 new tests PASS, all 9 existing tests PASS.

- [ ] **Step 5: Commit**

```bash
git add glass-slipper/LlamaServerManager.swift glass-slipper/GlassSlipperTests/LlamaServerManagerTests.swift
git commit -m "feat: add start/stop/health-poll to LlamaServerManager"
```

---

### Task 3: Wire CompanionWindowController to LlamaServerManager

**Files:**
- Modify: `glass-slipper/CompanionWindowController.swift`

Wire up `handleServerStart()` and auto-start on init. Replace the inline `isServerRunning` health check with the manager's state.

- [ ] **Step 1: Add LlamaServerManager property and delegate conformance**

In `CompanionWindowController.swift`, add a property and conform to the delegate:

```swift
final class CompanionWindowController: NSWindowController, MCPActivityLogDelegate, LlamaServerManagerDelegate {

    private let activityLog = MCPActivityLog()
    private let serverManager = LlamaServerManager()
```

- [ ] **Step 2: Replace `isServerRunning` computed property**

Remove the existing `isServerRunning` computed property (lines 36-48) and replace with:

```swift
    private var isServerRunning: Bool {
        serverManager.state == .running
    }
```

- [ ] **Step 3: Wire handleServerStart() to the manager**

Replace the stub `handleServerStart()` (lines 211-213) with:

```swift
    private func handleServerStart() {
        serverManager.start()
    }
```

- [ ] **Step 4: Add delegate method to update UI on state change**

```swift
    // MARK: - LlamaServerManagerDelegate

    func serverStateDidChange(_ state: LlamaServerState) {
        switch state {
        case .starting:
            serverRow.updateDetail("Starting…")
        case .running:
            serverRow.setComplete(true)
            serverRow.updateDetail("llama-server · Port 8787")
            refreshState()
        case .failed(let msg):
            serverRow.setComplete(false)
            serverRow.updateDetail("Failed: \(msg)")
        case .notRunning:
            serverRow.setComplete(false)
            serverRow.updateDetail("llama-server · Port 8787")
        }
    }
```

- [ ] **Step 5: Set delegate and auto-start in init**

In the `convenience init()`, after `activityLog.startPolling()`, add:

```swift
        serverManager.delegate = self
        // Auto-start if model is already downloaded
        if isModelDownloaded {
            serverManager.start()
        }
```

- [ ] **Step 6: Expose serverManager for AppDelegate quit cleanup**

Add a public accessor so AppDelegate can stop the server on quit:

```swift
    var llamaServerManager: LlamaServerManager { serverManager }
```

- [ ] **Step 7: Run tests to verify nothing broke**

Run: `xcodebuild test -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper 2>&1 | grep -E "(Test Case|test result)"`

Expected: All tests PASS.

- [ ] **Step 8: Commit**

```bash
git add glass-slipper/CompanionWindowController.swift
git commit -m "feat: wire Start Server button and auto-start to LlamaServerManager"
```

---

### Task 4: Wire AppDelegate quit to LlamaServerManager

**Files:**
- Modify: `glass-slipper/AppDelegate.swift`

Use the managed process for clean shutdown, keep lsof fallback for externally-started servers.

- [ ] **Step 1: Stop managed server in applicationShouldTerminate**

In `applicationShouldTerminate`, add a call to stop the managed server before the lsof fallback:

```swift
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
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) {
                NSApp.reply(toApplicationShouldTerminate: true)
            }
            return .terminateLater
        }

        return .terminateNow
    }
```

- [ ] **Step 2: Run tests**

Run: `xcodebuild test -project glass-slipper/GlassSlipper.xcodeproj -scheme GlassSlipper 2>&1 | grep -E "(Test Case|test result)"`

Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add glass-slipper/AppDelegate.swift
git commit -m "feat: wire AppDelegate quit to LlamaServerManager"
```
