import BridgethingCompanion
import Foundation
import Nlu
import Testing

@testable import BridgethingNluKit

@Suite("nlu bundle inference")
struct NluBundleInferenceTests {
    struct FakeModel: NluModelRunning {
        let info: ManifestInfo
        let intentHot: Int
        let oodLogit: Float

        func predict(inputIds: [Int32], attentionMask: [Int32]) throws -> NluModelOutputs {
            let n = info.intentNames.count
            var intent = [Float](repeating: 0, count: n)
            intent[intentHot] = 8
            return NluModelOutputs(
                intentLogits: intent,
                oodLogit: oodLogit,
                bioLogits: [Float](repeating: 0, count: inputIds.count * Int(info.bioTagCount)),
                closedLogits: info.closedHeadSizes.map { size in
                    var row = [Float](repeating: 0, count: Int(size))
                    row[0] = 8
                    return row
                }
            )
        }
    }

    func decoder() -> NluDecoder? {
        guard let dir = ProcessInfo.processInfo.environment["BRIDGETHING_NLU_BUNDLE"] else {
            print("BRIDGETHING_NLU_BUNDLE unset; skipping")
            return nil
        }
        return try? NluDecoder.load(bundleDir: dir)
    }

    @Test("the ood head is negated into the in-domain logit")
    func oodNegation() async throws {
        guard let decoder = decoder() else { return }
        let info = decoder.info()
        let pauseIndex = info.intentNames.firstIndex(of: "PAUSE")!
        let inference = try NluBundleInference(
            decoder: decoder,
            model: FakeModel(info: info, intentHot: pauseIndex, oodLogit: 3)
        )
        let out = try await inference.infer(transcript: "pause")
        #expect(out.inDomainLogit == -3)
        #expect(out.intentLogits.count == info.intentNames.count)
        #expect(out.intentLogits[pauseIndex] == 8)
    }

    @Test("the bundle's calibrated rejection pair surfaces on the conformer")
    func bundleRejection() throws {
        guard let decoder = decoder() else { return }
        let info = decoder.info()
        let inference = try NluBundleInference(
            decoder: decoder,
            model: FakeModel(info: info, intentHot: 0, oodLogit: 0)
        )
        if let rejection = info.rejection {
            #expect(inference.rejection?.inDomainThreshold == rejection.inDomainThreshold)
            #expect(inference.rejection?.clarifyMargin == rejection.clarifyMargin)
        } else {
            #expect(inference.rejection == nil)
        }
    }

    @Test("rejection flows through VoiceController end to end")
    func controllerIntegration() async throws {
        guard let decoder = decoder() else { return }
        let info = decoder.info()
        let searchIndex = info.intentNames.firstIndex(of: "SEARCH")!
        let inference = try NluBundleInference(
            decoder: decoder,
            model: FakeModel(info: info, intentHot: searchIndex, oodLogit: -6)
        )
        let controller = VoiceController(
            client: inference,
            config: .init(rejection: inference.rejection ?? .init())
        )
        let resolution = try await controller.resolve(transcript: "find me some nineties shoegaze")
        #expect(resolution.stage == .model)
        #expect(resolution.resolved.intent == "SEARCH")
    }
}
