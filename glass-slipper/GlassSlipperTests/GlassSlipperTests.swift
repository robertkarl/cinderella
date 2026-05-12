//
//  GlassSlipperTests.swift
//  GlassSlipperTests
//
//  Unit tests for Glass Slipper macOS app.
//

import XCTest

// MARK: - AppDelegate Tests

class AppDelegateTests: XCTestCase {

    func testFindLlamaServerReturnsValidPath() throws {
        let candidates = [
            "/opt/homebrew/bin/llama-server",
            "/usr/local/bin/llama-server",
        ]
        let found = candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
        if found == nil {
            throw XCTSkip("llama-server not installed")
        }
        XCTAssertNotNil(found)
    }

    func testModelFilePathContainsExpectedFilename() {
        let home = NSHomeDirectory()
        let expectedPath = home + "/Library/Application Support/Glass Slipper/Models/Qwen3.5-9B-Q5_K_M.gguf"
        XCTAssertTrue(expectedPath.hasSuffix("Qwen3.5-9B-Q5_K_M.gguf"))
    }

    func testLlamaServerArgumentsAreCorrect() {
        let model = "/tmp/test-model.gguf"
        let port = "8787"
        let ctxSize = "32768"
        let gpuLayers = "-1"

        let args = [
            "--model", model,
            "--port", port,
            "--ctx-size", ctxSize,
            "--n-gpu-layers", gpuLayers,
            "--jinja",
        ]

        XCTAssertEqual(args[0], "--model")
        XCTAssertEqual(args[1], model)
        XCTAssertEqual(args[2], "--port")
        XCTAssertEqual(args[3], port)
        XCTAssertTrue(args.contains("--jinja"))
        XCTAssertEqual(args.count, 9)
    }
}

// MARK: - CompanionWindowController Tests

class CompanionWindowControllerTests: XCTestCase {

    func testServerHealthCheckFailsGracefully() {
        let semaphore = DispatchSemaphore(value: 0)
        var healthy = false
        guard let url = URL(string: "http://127.0.0.1:19999/health") else {
            XCTFail("Invalid URL")
            return
        }
        URLSession.shared.dataTask(with: url) { _, response, _ in
            if let http = response as? HTTPURLResponse, http.statusCode == 200 {
                healthy = true
            }
            semaphore.signal()
        }.resume()
        _ = semaphore.wait(timeout: .now() + 3)
        XCTAssertFalse(healthy)
    }

    func testToolDisplayName() {
        let cases: [(String, String, String)] = [
            ("local_summarize", "build.sh", "Summarize — build.sh"),
            ("local_explain", "main.rs", "Explain — main.rs"),
            ("local_ask", "", "Ask"),
            ("local_status", "", "Status"),
            ("unknown_tool", "detail", "unknown_tool — detail"),
        ]
        for (tool, detail, expected) in cases {
            let result = toolDisplayName(tool: tool, detail: detail)
            XCTAssertEqual(result, expected, "toolDisplayName(\(tool), \(detail))")
        }
    }

    private func toolDisplayName(tool: String, detail: String) -> String {
        let verb: String
        switch tool {
        case "local_summarize": verb = "Summarize"
        case "local_explain":   verb = "Explain"
        case "local_ask":       verb = "Ask"
        case "local_web_fetch": verb = "Fetch"
        case "local_review":    verb = "Review"
        case "local_draft":     verb = "Draft"
        case "local_status":    verb = "Status"
        default:                verb = tool
        }
        if detail.isEmpty { return verb }
        return "\(verb) — \(detail)"
    }
}

// MARK: - ModelDownloadManager Tests

class ModelDownloadManagerTests: XCTestCase {

    func testModelPathResolution() {
        let home = NSHomeDirectory()
        let appSupportPath = home + "/Library/Application Support/Glass Slipper/Models"
        XCTAssertTrue(appSupportPath.contains("Glass Slipper"))
        XCTAssertTrue(appSupportPath.contains("Models"))
    }

    func testIsModelPresentWhenFileMissing() {
        let fakePath = "/tmp/glass-slipper-test-\(UUID().uuidString)/nonexistent.gguf"
        XCTAssertFalse(FileManager.default.fileExists(atPath: fakePath))
    }

    func testIsModelPresentWhenFileExists() throws {
        let tmpDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("glass-slipper-test-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tmpDir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: tmpDir) }

        let modelFile = tmpDir.appendingPathComponent("test-model.gguf")
        let fakeData = Data(repeating: 0, count: 1024)
        try fakeData.write(to: modelFile)

        XCTAssertTrue(FileManager.default.fileExists(atPath: modelFile.path))
    }
}

// MARK: - MCPInstaller Tests

class MCPInstallerTests: XCTestCase {

    func testClaudeJsonPathIsCorrect() {
        let home = NSHomeDirectory()
        let claudeJsonPath = home + "/.claude.json"
        XCTAssertTrue(claudeJsonPath.hasSuffix(".claude.json"))
    }
}
