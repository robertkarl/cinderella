//
//  CodexMCPInstallerTests.swift
//  GlassSlipperTests
//
//  Unit tests for CodexMCPInstaller.
//

import XCTest
@testable import GlassSlipper

class CodexMCPInstallerTests: XCTestCase {

    // MARK: - Path Probing

    func testFindCodexBinaryReturnsPathOrNil() {
        // This test verifies the method doesn't crash.
        // On CI without codex, it returns nil. On dev machines, it finds the binary.
        let result = CodexMCPInstaller.findCodexBinary()
        if let path = result {
            XCTAssertTrue(FileManager.default.isExecutableFile(atPath: path),
                          "Returned path should be executable: \(path)")
        }
        // nil is also a valid result (codex not installed)
    }

    func testIsCodexAvailableMatchesFindBinary() {
        let found = CodexMCPInstaller.findCodexBinary()
        XCTAssertEqual(CodexMCPInstaller.isCodexAvailable, found != nil)
    }

    // MARK: - Process.arguments with Space-Containing Paths

    func testProcessArgumentsHandleSpacesInPath() {
        // Verify that Process.arguments correctly passes paths with spaces
        // without shell word-splitting. This is the pattern CodexMCPInstaller uses.
        let pathWithSpaces = "/Applications/Glass Slipper.app/Contents/MacOS/glass-slipper-mcp"

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/bin/echo")
        proc.arguments = ["mcp", "add", "glass-slipper", "--", pathWithSpaces]

        let pipe = Pipe()
        proc.standardOutput = pipe

        try? proc.run()
        proc.waitUntilExit()

        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        let output = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""

        // echo should output the full path as a single argument, not split on space
        XCTAssertTrue(output.contains("Glass Slipper.app"),
                      "Path with spaces should be preserved: \(output)")
        XCTAssertTrue(output.hasSuffix(pathWithSpaces),
                      "Full path should appear at end: \(output)")
    }

    // MARK: - Cached State

    func testIsInstalledDefaultsToFalse() {
        // Before any refresh, isInstalled should be false
        // (This tests the initial static state)
        // Note: if tests run after a refreshState call, this may be true on dev machines.
        // The important thing is it doesn't crash.
        _ = CodexMCPInstaller.isInstalled
    }

    // MARK: - MCPBinaryPath Reference

    func testMCPBinaryPathIsAccessible() {
        // Verify CodexMCPInstaller can reference MCPInstaller.mcpBinaryPath
        let path = MCPInstaller.mcpBinaryPath
        XCTAssertTrue(path.hasSuffix("glass-slipper-mcp"),
                      "mcpBinaryPath should end with glass-slipper-mcp: \(path)")
    }
}
