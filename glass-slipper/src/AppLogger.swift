//
//  AppLogger.swift
//  Glass Slipper — centralized app-side JSONL logger
//
//  Writes structured events to ~/Library/Logs/Glass Slipper/glass-slipper-app.jsonl.
//  Safe to call from any thread (serialized via a dispatch queue).
//

import Foundation

enum AppLogger {

    private static let queue = DispatchQueue(label: "net.robertkarl.glass-slipper.logger")
    private static var handle: FileHandle?
    private static let formatter: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        return f
    }()

    /// Open (or create) the log file. Call once at launch.
    static func start() {
        queue.sync {
            let dir = NSHomeDirectory() + "/Library/Logs/Glass Slipper"
            try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
            let path = dir + "/glass-slipper-app.jsonl"
            if !FileManager.default.fileExists(atPath: path) {
                FileManager.default.createFile(atPath: path, contents: nil)
            }
            handle = FileHandle(forWritingAtPath: path)
            handle?.seekToEndOfFile()
        }
    }

    /// Log a structured event.
    static func log(_ event: String, _ details: [String: Any] = [:]) {
        queue.async {
            guard let handle else { return }
            var entry: [String: Any] = [
                "ts": formatter.string(from: Date()),
                "event": event,
            ]
            for (k, v) in details { entry[k] = v }
            guard let data = try? JSONSerialization.data(withJSONObject: entry),
                  let line = String(data: data, encoding: .utf8) else { return }
            if let lineData = (line + "\n").data(using: .utf8) {
                handle.write(lineData)
                handle.synchronizeFile()
            }
        }
    }
}
