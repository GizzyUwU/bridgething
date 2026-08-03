import CoreML
import Foundation

public protocol NluModelPrewarming: Sendable {
    func prewarm() throws
}

public final class LazyCoreMlModel: NluModelRunning, NluModelPrewarming, @unchecked Sendable {
    private let build: @Sendable () throws -> any NluModelRunning
    private let lock = NSLock()
    private var model: (any NluModelRunning)?

    public init(build: @escaping @Sendable () throws -> any NluModelRunning) {
        self.build = build
    }

    public convenience init(bundleDir: URL, closedHeadCount: Int, computeUnits: MLComputeUnits = .cpuOnly) {
        self.init {
            try CoreMlNluModel(bundleDir: bundleDir, closedHeadCount: closedHeadCount, computeUnits: computeUnits)
        }
    }

    public func prewarm() throws {
        _ = try resolved()
    }

    public func predict(inputIds: [Int32], attentionMask: [Int32]) throws -> NluModelOutputs {
        try resolved().predict(inputIds: inputIds, attentionMask: attentionMask)
    }

    private func resolved() throws -> any NluModelRunning {
        lock.lock()
        defer { lock.unlock() }
        if let model { return model }
        let built = try build()
        model = built
        return built
    }
}
