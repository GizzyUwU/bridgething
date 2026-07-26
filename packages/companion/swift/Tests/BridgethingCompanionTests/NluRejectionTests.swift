import Foundation
import Testing

@testable import BridgethingCompanion

@Suite("nlu rejection layer")
struct NluRejectionTests {
    static func output(
        _ logits: [String: Double],
        inDomain: Double = 8,
        count: Int? = nil
    ) -> NluInferenceOutput {
        let size = count ?? NluIntentCatalog.surfaceNames.count
        var vector = Array(repeating: 0.0, count: size)
        for (name, logit) in logits {
            guard let index = NluIntentCatalog.surfaceNames.firstIndex(of: name), index < size else { continue }
            vector[index] = logit
        }
        return NluInferenceOutput(intentLogits: vector, inDomainLogit: inDomain, slots: .init())
    }

    @Test("a clear winner in domain is accepted")
    func acceptsClearWinner() throws {
        let outcome = try NluRejection.evaluate(Self.output(["PAUSE": 9]))
        #expect(outcome == .accept(intent: "PAUSE"))
    }

    @Test("in-domain head below threshold yields NO_INTENT")
    func rejectsOutOfDomain() throws {
        let outcome = try NluRejection.evaluate(Self.output(["PAUSE": 9], inDomain: -6))
        #expect(outcome == .noIntent)
    }

    @Test("out of domain outranks an ambiguous distribution")
    func outOfDomainBeatsAmbiguity() throws {
        let outcome = try NluRejection.evaluate(Self.output(["PLAY": 4, "SEARCH": 4], inDomain: -6))
        #expect(outcome == .noIntent)
    }

    @Test("a narrow top-2 margin yields CLARIFY carrying the candidates")
    func clarifiesOnNarrowMargin() throws {
        let outcome = try NluRejection.evaluate(Self.output(["PLAY": 4.0, "SEARCH": 3.95]))
        guard case let .clarify(alternates) = outcome else {
            Issue.record("expected clarify, got \(outcome)")
            return
        }
        #expect(Set(alternates) == ["PLAY", "SEARCH"])
    }

    @Test("maxAlternates caps the candidate list")
    func capsAlternates() throws {
        let policy = NluRejectionPolicy(clarifyMargin: 0.5, maxAlternates: 3)
        let outcome = try NluRejection.evaluate(
            Self.output(["PLAY": 4, "SEARCH": 4, "NEXT": 4, "PAUSE": 4]),
            policy: policy
        )
        guard case let .clarify(alternates) = outcome else {
            Issue.record("expected clarify, got \(outcome)")
            return
        }
        #expect(alternates.count == 3)
    }

    @Test("a widened margin turns an accepted intent into CLARIFY")
    func marginIsLoadBearing() throws {
        let logits = Self.output(["PLAY": 4.0, "SEARCH": 3.0])
        #expect(try NluRejection.evaluate(logits, policy: .init(clarifyMargin: 0.01)) == .accept(intent: "PLAY"))
        guard case .clarify = try NluRejection.evaluate(logits, policy: .init(clarifyMargin: 0.9)) else {
            Issue.record("a 0.9 margin should not accept a 1.0-logit gap")
            return
        }
    }

    @Test("a head that disagrees with the catalog throws rather than guessing")
    func headMismatchThrows() {
        #expect(throws: NluRejection.RejectionError.self) {
            try NluRejection.evaluate(Self.output(["PAUSE": 9], count: 12))
        }
    }

    @Test("softmax is stable on large logits and sums to one")
    func softmaxStability() {
        let probabilities = NluRejection.softmax([900, 901, 899])
        #expect(probabilities.allSatisfy { $0.isFinite }, "softmax overflowed: \(probabilities)")
        #expect(abs(probabilities.reduce(0, +) - 1) < 1e-9)
    }
}
