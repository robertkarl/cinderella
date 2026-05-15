//
//  MCPInstaller.swift
//  Glass Slipper — one-click MCP config for Claude Code
//
//  Reads ~/.claude.json, adds/removes the glass-slipper MCP entry,
//  writes it back. Creates the file if it doesn't exist.
//

import Foundation

enum MCPInstaller {

    /// Path to the MCP binary inside the app bundle.
    static var mcpBinaryPath: String {
        Bundle.main.bundlePath + "/Contents/MacOS/glass-slipper-mcp"
    }

    /// Path to Claude Code config.
    static var claudeConfigPath: String {
        NSHomeDirectory() + "/.claude.json"
    }

    /// Check if MCP is already configured.
    static var isInstalled: Bool {
        guard let config = readConfig() else { return false }
        guard let servers = config["mcpServers"] as? [String: Any] else { return false }
        return servers["glass-slipper"] != nil
    }

    /// Install the MCP entry into ~/.claude.json.
    /// Returns nil on success, error message on failure.
    static func install() -> String? {
        var config = readConfig() ?? [String: Any]()

        var servers = config["mcpServers"] as? [String: Any] ?? [:]
        servers["glass-slipper"] = [
            "command": mcpBinaryPath
        ]
        config["mcpServers"] = servers

        return writeConfig(config)
    }

    /// Remove the MCP entry from ~/.claude.json.
    /// Returns nil on success, error message on failure.
    static func uninstall() -> String? {
        guard var config = readConfig() else { return nil }
        guard var servers = config["mcpServers"] as? [String: Any] else { return nil }
        servers.removeValue(forKey: "glass-slipper")
        config["mcpServers"] = servers
        return writeConfig(config)
    }

    // MARK: - Private

    private static func readConfig() -> [String: Any]? {
        let path = claudeConfigPath
        guard FileManager.default.fileExists(atPath: path),
              let data = FileManager.default.contents(atPath: path),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        return json
    }

    private static func writeConfig(_ config: [String: Any]) -> String? {
        do {
            let data = try JSONSerialization.data(
                withJSONObject: config,
                options: [.prettyPrinted, .sortedKeys]
            )
            let path = claudeConfigPath
            try data.write(to: URL(fileURLWithPath: path))
            return nil
        } catch {
            return "Failed to write \(claudeConfigPath): \(error.localizedDescription)"
        }
    }
}
