import BridgethingCompanion
import BridgethingSchema
import CoreML
import Foundation
import Nlu

    public enum BundleError: Error, CustomStringConvertible {
        case catalogMismatch(bundle: [String], catalog: [String])

        public var description: String {
            switch self {
            case let .catalogMismatch(bundle, catalog):
                return "bundle intents \(bundle) do not match the companion catalog \(catalog)"
            }
        }
    }

    private let decoder: NluDecoder
    private let model: any NluModelRunning

    public let rejection: NluRejectionPolicy?

    public init(decoder: NluDecoder, model: any NluModelRunning) throws {
        let info = decoder.info()
        guard info.intentNames == NluIntentCatalog.surfaceNames else {
            throw BundleError.catalogMismatch(bundle: info.intentNames, catalog: NluIntentCatalog.surfaceNames)
        }
        self.decoder = decoder
        self.model = model
        self.rejection = info.rejection.map {
            NluRejectionPolicy(inDomainThreshold: $0.inDomainThreshold, clarifyMargin: $0.clarifyMargin)
        }
    }

    public convenience init(bundleDir: URL, model: any NluModelRunning) throws {
        try self.init(decoder: try NluDecoder.load(bundleDir: bundleDir.path), model: model)
    }

    public static func load(
        bundleDir: URL,
        computeUnits: MLComputeUnits = .cpuOnly,
        deferModel: Bool = false
    ) throws -> NluBundleInference {
        let decoder = try NluDecoder.load(bundleDir: bundleDir.path)
        let heads = decoder.info().closedHeadSizes.count
        let model: any NluModelRunning = deferModel
            ? LazyCoreMlModel(bundleDir: bundleDir, closedHeadCount: heads, computeUnits: computeUnits)
            : try CoreMlNluModel(bundleDir: bundleDir, closedHeadCount: heads, computeUnits: computeUnits)
        return try NluBundleInference(decoder: decoder, model: model)
    }

    public func infer(transcript: String) async throws -> NluInferenceOutput {
        let tokens = try decoder.tokenize(transcript: transcript)
        let out = try model.predict(inputIds: tokens.inputIds, attentionMask: tokens.attentionMask)
        let frame = try decoder.decode(
            transcript: transcript,
            tokens: tokens,
            intentLogits: out.intentLogits,
            bioLogits: out.bioLogits,
            closedLogits: out.closedLogits
        )
        return NluInferenceOutput(
            intentLogits: out.intentLogits.map(Double.init),
            inDomainLogit: -Double(out.oodLogit),
            slots: NluSlotMapping.apply(frame.slots)
        )
    }
}

extension NluBundleInference: NluPrewarmable {
    public func prewarm() async {
        guard let prewarming = model as? any NluModelPrewarming else { return }
        await Task.detached(priority: .userInitiated) { try? prewarming.prewarm() }.value
    }
}
