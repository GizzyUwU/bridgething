import Foundation

@testable import BridgethingCompanion

struct FakeNluInference: NluInferring {
    enum FakeError: Error { case shouldNotHaveBeenCalled }

    var logits: [String: Double] = [:]
    var inDomainLogit: Double = 8
    var slots: NluMutableSlots = .init()
    var failWithCall: Bool = false
    var logitCountOverride: Int?

    func infer(transcript: String) async throws -> NluInferenceOutput {
        if failWithCall { throw FakeError.shouldNotHaveBeenCalled }
        let count = logitCountOverride ?? NluIntentCatalog.surfaceNames.count
        var vector = Array(repeating: 0.0, count: count)
        for (name, logit) in logits {
            guard let index = NluIntentCatalog.surfaceNames.firstIndex(of: name), index < count else { continue }
            vector[index] = logit
        }
        return NluInferenceOutput(intentLogits: vector, inDomainLogit: inDomainLogit, slots: slots)
    }
}
