import XCTest
@testable import GlassSlipper

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

    func testStartWithMissingBinaryTransitionsToFailed() {
        let manager = LlamaServerManager()
        manager.start(binaryPath: "/nonexistent/llama-server", modelPath: "/tmp/model.gguf")
        XCTAssertEqual(manager.state, .failed("llama-server not found"))
    }

    func testStartWithMissingModelTransitionsToFailed() {
        let manager = LlamaServerManager()
        manager.start(binaryPath: "/bin/ls", modelPath: "/nonexistent/model.gguf")
        XCTAssertEqual(manager.state, .failed("Model not found"))
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
