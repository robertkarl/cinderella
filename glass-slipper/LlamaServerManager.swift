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
