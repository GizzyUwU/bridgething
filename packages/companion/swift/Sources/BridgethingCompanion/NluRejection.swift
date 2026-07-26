import Foundation

public struct NluRejectionPolicy: Sendable, Equatable {
    public var inDomainThreshold: Double
    public var clarifyMargin: Double
    public var maxAlternates: Int

    public init(inDomainThreshold: Double = 0.5, clarifyMargin: Double = 0.15, maxAlternates: Int = 2) {
        self.inDomainThreshold = inDomainThreshold
        self.clarifyMargin = clarifyMargin
        self.maxAlternates = maxAlternates
    }
}

public enum NluRejectionOutcome: Sendable, Equatable {
    case accept(intent: String)
    case noIntent
    case clarify(alternates: [String])
}

public enum NluRejection {
    public enum RejectionError: Error, CustomStringConvertible {
        case headMismatch(logits: Int, catalog: Int)

        public var description: String {
            switch self {
            case let .headMismatch(logits, catalog):
                return "intent head emits \(logits) logits but the catalog has \(catalog) names"
            }
        }
    }

    public static func evaluate(
        _ output: NluInferenceOutput,
        policy: NluRejectionPolicy = .init()
    ) throws -> NluRejectionOutcome {
        let names = NluIntentCatalog.surfaceNames
        guard output.intentLogits.count == names.count else {
            throw RejectionError.headMismatch(logits: output.intentLogits.count, catalog: names.count)
        }

        guard sigmoid(output.inDomainLogit) >= policy.inDomainThreshold else { return .noIntent }

        let probabilities = softmax(output.intentLogits)
        let ranked = probabilities.enumerated()
            .sorted { $0.element > $1.element }
            .map { (name: names[$0.offset], probability: $0.element) }

        guard let top = ranked.first else { return .noIntent }
        guard let runnerUp = ranked.dropFirst().first else { return .accept(intent: top.name) }

        if top.probability - runnerUp.probability < policy.clarifyMargin {
            let alternates = ranked.prefix(max(policy.maxAlternates, 0)).map(\.name)
            return .clarify(alternates: Array(alternates))
        }
        return .accept(intent: top.name)
    }

    static func sigmoid(_ x: Double) -> Double {
        1 / (1 + exp(-x))
    }

    static func softmax(_ logits: [Double]) -> [Double] {
        guard let peak = logits.max() else { return [] }
        let exps = logits.map { exp($0 - peak) }
        let total = exps.reduce(0, +)
        guard total > 0 else { return Array(repeating: 0, count: logits.count) }
        return exps.map { $0 / total }
    }
}
