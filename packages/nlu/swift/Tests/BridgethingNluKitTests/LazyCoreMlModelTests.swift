import Foundation
import Testing

@testable import BridgethingNluKit

private final class Counter: @unchecked Sendable {
    private let lock = NSLock()
    private var value = 0

    func bump() {
        lock.lock()
        defer { lock.unlock() }
        value += 1
    }

    var count: Int {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}

private struct StubModel: NluModelRunning {
    func predict(inputIds: [Int32], attentionMask _: [Int32]) throws -> NluModelOutputs {
        NluModelOutputs(
            intentLogits: [Float(inputIds.count)],
            oodLogit: 0,
            bioLogits: [],
            closedLogits: []
        )
    }
}

@Suite("lazy coreml model")
struct LazyCoreMlModelTests {
    @Test("nothing is built until the first prediction")
    func deferredUntilUse() throws {
        let builds = Counter()
        let model = LazyCoreMlModel {
            builds.bump()
            return StubModel()
        }

        #expect(builds.count == 0)

        _ = try model.predict(inputIds: [1, 2, 3], attentionMask: [1, 1, 1])

        #expect(builds.count == 1)
    }

    @Test("later predictions reuse the model built by the first")
    func buildsOnce() throws {
        let builds = Counter()
        let model = LazyCoreMlModel {
            builds.bump()
            return StubModel()
        }

        for _ in 0 ..< 5 {
            _ = try model.predict(inputIds: [1], attentionMask: [1])
        }

        #expect(builds.count == 1)
    }

    @Test("concurrent first use builds exactly one model")
    func singleFlight() async throws {
        let builds = Counter()
        let model = LazyCoreMlModel {
            builds.bump()
            Thread.sleep(forTimeInterval: 0.05)
            return StubModel()
        }

        await withTaskGroup(of: Void.self) { group in
            for _ in 0 ..< 8 {
                group.addTask { _ = try? model.predict(inputIds: [1], attentionMask: [1]) }
            }
        }

        #expect(builds.count == 1)
    }

    @Test("prewarm pays the build cost before the first prediction")
    func prewarmBuilds() throws {
        let builds = Counter()
        let model = LazyCoreMlModel {
            builds.bump()
            return StubModel()
        }

        try model.prewarm()
        #expect(builds.count == 1)

        _ = try model.predict(inputIds: [1], attentionMask: [1])
        #expect(builds.count == 1)
    }

    @Test("a failed build is retried rather than cached")
    func failedBuildRetries() throws {
        struct BuildFailure: Error {}
        let builds = Counter()
        let model = LazyCoreMlModel {
            builds.bump()
            if builds.count == 1 { throw BuildFailure() }
            return StubModel()
        }

        #expect(throws: BuildFailure.self) { try model.prewarm() }
        _ = try model.predict(inputIds: [1], attentionMask: [1])

        #expect(builds.count == 2)
    }
}
