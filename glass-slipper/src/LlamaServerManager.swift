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
    private var healthPollStart: Date?

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
        NSHomeDirectory() + "/Library/Application Support/Glass Slipper/Models/Qwen3.5-9B-Q5_K_M.gguf"
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

    // MARK: - Lifecycle

    /// Start llama-server. Pass explicit paths or nil to auto-resolve.
    func start(binaryPath: String? = nil, modelPath: String? = nil) {
        if case .starting = state { return }
        if case .running = state { return }

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

    private static let healthTimeoutSeconds: TimeInterval = 60

    private func startHealthPolling() {
        healthPollStart = Date()
        healthTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            self?.checkHealth()
        }
    }

    private func checkHealth() {
        if let start = healthPollStart, Date().timeIntervalSince(start) > Self.healthTimeoutSeconds {
            healthTimer?.invalidate()
            healthTimer = nil
            healthPollStart = nil
            stop()
            setState(.failed("Health check timed out after \(Int(Self.healthTimeoutSeconds))s"))
            return
        }

        var request = URLRequest(url: Self.healthURL)
        request.timeoutInterval = 2
        URLSession.shared.dataTask(with: request) { [weak self] _, response, _ in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if let http = response as? HTTPURLResponse, http.statusCode == 200 {
                    self.healthTimer?.invalidate()
                    self.healthTimer = nil
                    self.healthPollStart = nil
                    self.setState(.running)
                }
            }
        }.resume()
    }

    private func setState(_ newState: LlamaServerState) {
        state = newState
        delegate?.serverStateDidChange(newState)
    }
}
