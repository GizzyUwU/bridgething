#if canImport(CoreML)

    import BridgethingCompanionCore
    import CoreML
    import Foundation

    final class CoreMlNluModel: @unchecked Sendable {
        enum ModelError: Error, CustomStringConvertible {
            case missingOutput(String)
            case raggedClosedHeads([String])

            var description: String {
                switch self {
                case let .missingOutput(name): return "model output \(name) missing"
                case let .raggedClosedHeads(names): return "closed outputs \(names) are not a contiguous closed_N run"
                }
            }
        }

        private let model: MLModel
        private let closedHeadCount: Int

        init(model: MLModel) throws {
            self.model = model
            let names = model.modelDescription.outputDescriptionsByName.keys
            var count = 0
            while names.contains("closed_\(count)") { count += 1 }
            let claimed = names.filter { $0.hasPrefix("closed_") }
            guard claimed.count == count else {
                throw ModelError.raggedClosedHeads(claimed.sorted())
            }
            closedHeadCount = count
        }

        convenience init(bundleDir: URL, computeUnits: MLComputeUnits = .cpuOnly) throws {
            let config = MLModelConfiguration()
            config.computeUnits = computeUnits
            try self.init(model: MLModel(contentsOf: Self.compiled(bundleDir: bundleDir), configuration: config))
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

        func predict(inputIds: [Int32], attentionMask: [Int32]) throws -> NluModelOutputs {
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
                closedLogits: try (0 ..< closedHeadCount).map { try floats(out, "closed_\($0)") }
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
            for i in 0 ..< value.count {
                out[i] = value[i].floatValue
            }
            return out
        }
    }

    public final class CoreMlNluRunner: NluModelRunner, @unchecked Sendable {
        private let bundleDir: @Sendable () -> String?
        private let lock = NSLock()
        private var loaded: (dir: String, model: CoreMlNluModel)?

        public init(bundleDir: @escaping @Sendable () -> String?) {
            self.bundleDir = bundleDir
        }

        public func prewarm() {
            _ = try? resolved()
        }

        public func predict(inputIds: [Int32], attentionMask: [Int32]) throws -> NluModelOutputs {
            guard let model = try resolved() else {
                throw NluRunnerError.NotLoaded
            }
            do {
                return try model.predict(inputIds: inputIds, attentionMask: attentionMask)
            } catch {
                throw NluRunnerError.Failed(reason: String(describing: error))
            }
        }

        private func resolved() throws -> CoreMlNluModel? {
            lock.lock()
            defer { lock.unlock() }
            guard let dir = bundleDir() else { return nil }
            if let loaded, loaded.dir == dir { return loaded.model }
            let model: CoreMlNluModel
            do {
                model = try CoreMlNluModel(bundleDir: URL(fileURLWithPath: dir, isDirectory: true))
            } catch {
                throw NluRunnerError.Failed(reason: String(describing: error))
            }
            loaded = (dir, model)
            return model
        }
    }

#endif
