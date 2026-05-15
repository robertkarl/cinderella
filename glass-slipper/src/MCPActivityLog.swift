//
//  MCPActivityLog.swift
//  Glass Slipper — reads mcp-activity.jsonl for the companion window
//
//  Polls the JSONL file for new lines. Parses entries and notifies
//  the delegate of new activity. Computes running totals.
//

import Foundation

/// One parsed entry from mcp-activity.jsonl.
struct MCPActivityEntry {
    let timestamp: String
    let tool: String
    let detail: String
    let inputTokens: Int
    let outputTokens: Int
    let contextTokens: Int
    let latencyMs: Int
    let estimatedCloudCostUSD: Double
    let cacheHit: Bool
    let model: String
}

/// Running totals for the savings dashboard.
struct MCPSavingsSummary {
    var totalCostSaved: Double = 0
    var totalTasksDelegated: Int = 0
    var totalTokensSaved: Int = 0
}

protocol MCPActivityLogDelegate: AnyObject {
    func activityLogDidUpdate(entries: [MCPActivityEntry], summary: MCPSavingsSummary)
}

final class MCPActivityLog {

    weak var delegate: MCPActivityLogDelegate?

    private var entries: [MCPActivityEntry] = []
    private var summary = MCPSavingsSummary()
    private var lastReadOffset: UInt64 = 0
    private var timer: Timer?

    private var logPath: String {
        NSHomeDirectory() + "/Library/Application Support/Glass Slipper/mcp-activity.jsonl"
    }

    /// Start polling for new entries every 2 seconds.
    func startPolling() {
        readNewEntries()
        timer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { [weak self] _ in
            self?.readNewEntries()
        }
    }

    func stopPolling() {
        timer?.invalidate()
        timer = nil
    }

    private func readNewEntries() {
        guard FileManager.default.fileExists(atPath: logPath),
              let handle = FileHandle(forReadingAtPath: logPath) else { return }

        handle.seek(toFileOffset: lastReadOffset)
        let data = handle.readDataToEndOfFile()
        lastReadOffset = handle.offsetInFile
        handle.closeFile()

        guard !data.isEmpty,
              let text = String(data: data, encoding: .utf8) else { return }

        var newEntries = [MCPActivityEntry]()

        for line in text.components(separatedBy: "\n") where !line.isEmpty {
            guard let jsonData = line.data(using: .utf8),
                  let json = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any]
            else { continue }

            let entry = MCPActivityEntry(
                timestamp: json["ts"] as? String ?? "",
                tool: json["tool"] as? String ?? "",
                detail: json["detail"] as? String ?? "",
                inputTokens: json["input_tokens"] as? Int ?? 0,
                outputTokens: json["output_tokens"] as? Int ?? 0,
                contextTokens: json["context_tokens"] as? Int ?? 0,
                latencyMs: json["latency_ms"] as? Int ?? 0,
                estimatedCloudCostUSD: json["estimated_cloud_cost_usd"] as? Double ?? 0,
                cacheHit: json["cache_hit"] as? Bool ?? false,
                model: json["model"] as? String ?? ""
            )

            newEntries.append(entry)
            summary.totalCostSaved += entry.estimatedCloudCostUSD
            summary.totalTasksDelegated += 1
            summary.totalTokensSaved += entry.contextTokens
        }

        if !newEntries.isEmpty {
            entries.append(contentsOf: newEntries)
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.delegate?.activityLogDidUpdate(entries: self.entries, summary: self.summary)
            }
        }
    }
}
