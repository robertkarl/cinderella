//
//  ModelDownloadManager.swift
//  Cinderella — first-run model download
//
//  Downloads the Qwen GGUF to Application Support on first launch.
//  Resumable via URLSession resume data. SHA-256 verified after download.
//

import AppKit
import CryptoKit

// MARK: - Manifest parsing

struct ModelManifest: Codable {
    let version: Int
    let models: [ModelDefinition]
    let default_model: String
}

struct ModelDefinition: Codable {
    let id: String
    let name: String
    let filename: String
    let quant: String
    let size_bytes: Int64
    let sha256: String
    let url: String
    let min_ram_gb: Int
    let app_support_subdir: String
}

// MARK: - Delegate

protocol ModelDownloadManagerDelegate: AnyObject {
    func downloadDidUpdateProgress(downloaded: Int64, total: Int64)
    func downloadDidBeginVerification()
    func downloadDidFinish()
    func downloadDidFail(error: String)
}

// MARK: - Manager

final class ModelDownloadManager: NSObject, URLSessionDownloadDelegate {

    let model: ModelDefinition
    weak var delegate: ModelDownloadManagerDelegate?

    private var session: URLSession!
    private var downloadTask: URLSessionDownloadTask?

    init(model: ModelDefinition) {
        self.model = model
        super.init()
        let config = URLSessionConfiguration.default
        self.session = URLSession(configuration: config, delegate: self, delegateQueue: .main)
    }

    // MARK: - Paths

    var modelDir: URL {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let dir = home.appendingPathComponent("Library/Application Support/\(model.app_support_subdir)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    var modelPath: URL { modelDir.appendingPathComponent(model.filename) }
    var partPath: URL { modelDir.appendingPathComponent(model.filename + ".part") }
    var resumeDataPath: URL { modelDir.appendingPathComponent(".download-resume-data") }

    // MARK: - Check

    var isModelPresent: Bool {
        guard FileManager.default.fileExists(atPath: modelPath.path) else { return false }
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: modelPath.path),
              let size = attrs[.size] as? Int64 else { return false }
        return size == model.size_bytes
    }

    // MARK: - Download

    func startDownload() {
        guard model.url != "TODO_FILL_MODEL_URL" else {
            delegate?.downloadDidFail(error: "Model download URL not configured yet.")
            return
        }
        guard let url = URL(string: model.url) else {
            delegate?.downloadDidFail(error: "Invalid model URL: \(model.url)")
            return
        }

        // Check available disk space (model size + 1 GB margin for .part + final)
        let requiredBytes = model.size_bytes + 1_073_741_824
        if let attrs = try? FileManager.default.attributesOfFileSystem(forPath: modelDir.path),
           let freeBytes = attrs[.systemFreeSize] as? Int64,
           freeBytes < requiredBytes {
            let freeGB = String(format: "%.1f", Double(freeBytes) / 1_073_741_824)
            let needGB = String(format: "%.1f", Double(requiredBytes) / 1_073_741_824)
            delegate?.downloadDidFail(error: "Not enough disk space. Need \(needGB) GB free, have \(freeGB) GB.")
            return
        }

        // Resume from saved data if available
        if let data = try? Data(contentsOf: resumeDataPath) {
            downloadTask = session.downloadTask(withResumeData: data)
        } else {
            downloadTask = session.downloadTask(with: url)
        }
        downloadTask?.resume()
    }

    func cancelDownload() {
        downloadTask?.cancel(byProducingResumeData: { [weak self] data in
            guard let self, let data else { return }
            try? data.write(to: self.resumeDataPath)
        })
        downloadTask = nil
    }

    // MARK: - URLSessionDownloadDelegate

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                    didWriteData bytesWritten: Int64, totalBytesWritten: Int64,
                    totalBytesExpectedToWrite: Int64) {
        let total = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : model.size_bytes
        delegate?.downloadDidUpdateProgress(downloaded: totalBytesWritten, total: total)
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                    didFinishDownloadingTo location: URL) {
        // Move to .part first, then verify
        try? FileManager.default.removeItem(at: partPath)
        do {
            try FileManager.default.moveItem(at: location, to: partPath)
        } catch {
            delegate?.downloadDidFail(error: "Failed to save download: \(error.localizedDescription)")
            return
        }

        // Clean up resume data
        try? FileManager.default.removeItem(at: resumeDataPath)

        // Notify UI that verification is starting
        delegate?.downloadDidBeginVerification()

        // Verify SHA-256 on background thread
        let expectedHash = model.sha256
        let part = partPath
        let final_ = modelPath
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let ok = Self.verifySHA256(file: part, expected: expectedHash)
            DispatchQueue.main.async {
                guard let self else { return }
                if ok {
                    // Atomic rename
                    try? FileManager.default.removeItem(at: final_)
                    do {
                        try FileManager.default.moveItem(at: part, to: final_)
                        self.delegate?.downloadDidFinish()
                    } catch {
                        self.delegate?.downloadDidFail(error: "Failed to finalize model: \(error.localizedDescription)")
                    }
                } else {
                    try? FileManager.default.removeItem(at: part)
                    self.delegate?.downloadDidFail(error: "SHA-256 verification failed. The download may be corrupt.")
                }
            }
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?) {
        guard let error else { return } // success handled in didFinishDownloadingTo
        let nsError = error as NSError

        // Save resume data if available
        if let resumeData = nsError.userInfo[NSURLSessionDownloadTaskResumeData] as? Data {
            try? resumeData.write(to: resumeDataPath)
        }

        if nsError.code == NSURLErrorCancelled { return } // user-initiated cancel
        delegate?.downloadDidFail(error: error.localizedDescription)
    }

    // MARK: - SHA-256

    private static func verifySHA256(file: URL, expected: String) -> Bool {
        #if DEBUG
        if expected.hasPrefix("TODO") { return true }
        #endif
        guard let handle = try? FileHandle(forReadingFrom: file) else { return false }
        defer { handle.closeFile() }

        var hasher = SHA256()
        while autoreleasepool(invoking: {
            let chunk = handle.readData(ofLength: 8 * 1024 * 1024)
            if chunk.isEmpty { return false }
            hasher.update(data: chunk)
            return true
        }) {}

        let digest = hasher.finalize()
        let hash = digest.map { String(format: "%02x", $0) }.joined()
        return hash == expected
    }

    // MARK: - Load manifest

    static func loadManifest() -> ModelManifest? {
        // Release: bundled in app
        if let url = Bundle.main.url(forResource: "model-manifest", withExtension: "json"),
           let data = try? Data(contentsOf: url),
           let manifest = try? JSONDecoder().decode(ModelManifest.self, from: data) {
            return manifest
        }

        // Dev: walk up from the app bundle until we find the repo root (has Cargo.toml)
        var dir = Bundle.main.bundlePath as NSString
        for _ in 0..<10 {
            dir = dir.deletingLastPathComponent as NSString
            let candidate = dir.appendingPathComponent("model-manifest.json")
            let cargoToml = dir.appendingPathComponent("Cargo.toml")
            if FileManager.default.fileExists(atPath: cargoToml),
               let data = try? Data(contentsOf: URL(fileURLWithPath: candidate)),
               let manifest = try? JSONDecoder().decode(ModelManifest.self, from: data) {
                return manifest
            }
        }

        return nil
    }
}
