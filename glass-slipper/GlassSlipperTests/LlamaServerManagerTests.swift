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
}
