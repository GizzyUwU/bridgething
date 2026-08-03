import CoreML
import Foundation

public final class CoreMlNluModel: NluModelRunning, @unchecked Sendable {
    public enum ModelError: Error, CustomStringConvertible {
        case missingOutput(String)

        public var description: String {
            switch self {
            case let .missingOutput(name): return "model output \(name) missing"
            }
        }
    }

    private let model: MLModel
    private let closedHeadCount: Int

    public init(model: MLModel, closedHeadCount: Int) {
        self.model = model
        self.closedHeadCount = closedHeadCount
    }

    public convenience init(bundleDir: URL, closedHeadCount: Int, computeUnits: MLComputeUnits = .cpuOnly) throws {
        let config = MLModelConfiguration()
        config.computeUnits = computeUnits
        self.init(
            model: try MLModel(contentsOf: try Self.compiled(bundleDir: bundleDir), configuration: config),
            closedHeadCount: closedHeadCount
        )
    }

    static func compiled(bundleDir: URL) throws -> URL {
        let cached = bundleDir.appendingPathComponent("model.mlmodelc", isDirectory: true)
        if FileManager.default.fileExists(atPath: cached.path) { return cached }
        let fresh = try MLModel.compileModel(at: bundleDir.appendingPathComponent("model.mlpackage", isDirectory: true))
        do {
            try FileManager.default.moveItem(at: fresh, to: cached)
            return cached
        } catch {
            return FileManager.default.fileExists(atPath: cached.path) ? cached : fresh
        }
    }

    public func predict(inputIds: [Int32], attentionMask: [Int32]) throws -> NluModelOutputs {
        let ids = try multiArray(inputIds)
        let mask = try multiArray(attentionMask)
        let out = try model.prediction(from: MLDictionaryFeatureProvider(dictionary: [
            "input_ids": MLFeatureValue(multiArray: ids),
            "attention_mask": MLFeatureValue(multiArray: mask),
        ]))

        return NluModelOutputs(
            intentLogits: try floats(out, "intent"),
            oodLogit: try floats(out, "ood").first ?? 0,
            bioLogits: try floats(out, "bio"),
            closedLogits: try (0..<closedHeadCount).map { try floats(out, "closed_\($0)") }
        )
    }

    private func multiArray(_ values: [Int32]) throws -> MLMultiArray {
        let array = try MLMultiArray(shape: [1, NSNumber(value: values.count)], dataType: .int32)
        for (i, v) in values.enumerated() {
            array[i] = NSNumber(value: v)
        }
        return array
    }

    private func floats(_ provider: MLFeatureProvider, _ name: String) throws -> [Float] {
        guard let value = provider.featureValue(for: name)?.multiArrayValue else {
            throw ModelError.missingOutput(name)
        }
        if value.dataType == .float32 {
            return value.withUnsafeBufferPointer(ofType: Float.self) { Array($0) }
        }
        var out = [Float](repeating: 0, count: value.count)
        for i in 0..<value.count {
            out[i] = value[i].floatValue
        }
        return out
    }
}
