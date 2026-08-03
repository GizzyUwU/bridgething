import CoreML
import Foundation
import Testing

@testable import BridgethingNluKit

@Suite("coreml compile cache")
struct CoreMlCompileCacheTests {
    private static func stagedBundle() throws -> URL? {
        guard let package = ProcessInfo.processInfo.environment["BRIDGETHING_NLU_MLPACKAGE"] else {
            print("BRIDGETHING_NLU_MLPACKAGE unset; skipping")
            return nil
        }
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("nlu-compile-cache-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try FileManager.default.copyItem(
            at: URL(fileURLWithPath: package),
            to: dir.appendingPathComponent("model.mlpackage")
        )
        return dir
    }

    @Test("the compiled model is kept in the bundle and reused")
    func compilesOnceAndCaches() throws {
        guard let bundle = try Self.stagedBundle() else { return }
        defer { try? FileManager.default.removeItem(at: bundle) }
        let cached = bundle.appendingPathComponent("model.mlmodelc", isDirectory: true)

        let first = try CoreMlNluModel.compiled(bundleDir: bundle)
        #expect(first.standardizedFileURL == cached.standardizedFileURL)
        #expect(FileManager.default.fileExists(atPath: cached.path))

        let stamp = try FileManager.default.attributesOfItem(atPath: cached.path)[.modificationDate] as? Date
        let second = try CoreMlNluModel.compiled(bundleDir: bundle)

        #expect(second.standardizedFileURL == cached.standardizedFileURL)
        let after = try FileManager.default.attributesOfItem(atPath: cached.path)[.modificationDate] as? Date
        #expect(stamp == after)
    }

    @Test("a cached compile still loads and predicts")
    func cachedModelLoads() throws {
        guard let bundle = try Self.stagedBundle() else { return }
        defer { try? FileManager.default.removeItem(at: bundle) }

        _ = try CoreMlNluModel(bundleDir: bundle, closedHeadCount: 0)
        let reloaded = try CoreMlNluModel(bundleDir: bundle, closedHeadCount: 0)
        let out = try reloaded.predict(
            inputIds: Array(repeating: Int32(1), count: 64),
            attentionMask: Array(repeating: Int32(1), count: 64)
        )

        #expect(!out.intentLogits.isEmpty)
    }
}
