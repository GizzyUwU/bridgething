import BridgethingSchema
import Foundation

public actor VoiceController {
    public struct Config: Sendable {
        public let useFastPath: Bool
        public let rejection: NluRejectionPolicy

        public init(useFastPath: Bool = true, rejection: NluRejectionPolicy = .init()) {
            self.useFastPath = useFastPath
            self.rejection = rejection
        }
    }

    public enum Stage: String, Sendable {
        case fastPath
        case model
        case rejectedNoIntent
        case rejectedClarify
        case noModel

        var wire: NluStage {
            switch self {
            case .fastPath: return .fastPath
            case .model: return .model
            case .rejectedNoIntent: return .rejectedNoIntent
            case .rejectedClarify: return .rejectedClarify
            case .noModel: return .noModel
            }
        }
    }

    public struct Resolution: Sendable {
        public let resolved: NluResolvedIntent
        public let stage: Stage

        public init(resolved: NluResolvedIntent, stage: Stage) {
            self.resolved = resolved
            self.stage = stage
        }
    }

    public enum ControllerError: Error, CustomStringConvertible {
        case inferenceFailed(Error)

        public var description: String {
            switch self {
            case let .inferenceFailed(err): return "nlu inference failed: \(err)"
            }
        }
    }

    private let client: (any NluInferring)?
    private let config: Config

    public init(client: (any NluInferring)? = nil, config: Config = Config()) {
        self.client = client
        self.config = config
    }

    public func resolve(transcript: String) async throws -> Resolution {
        let trimmed = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return Resolution(
                resolved: NluPrediction(intent: NluIntentCatalog.noIntent, transcript: transcript).toWire(),
                stage: .rejectedNoIntent
            )
        }

        if config.useFastPath, let hit = NluFastPath.match(trimmed) {
            let pred = NluPrediction(intent: hit.intent, slots: hit.slots, transcript: transcript)
            return Resolution(resolved: pred.toWire(), stage: .fastPath)
        }

        guard let client else {
            return Resolution(
                resolved: NluPrediction(intent: NluIntentCatalog.noIntent, transcript: transcript).toWire(),
                stage: .noModel
            )
        }

        let output: NluInferenceOutput
        do {
            output = try await client.infer(transcript: trimmed)
        } catch {
            throw ControllerError.inferenceFailed(error)
        }

        switch try NluRejection.evaluate(output, policy: config.rejection) {
        case .noIntent:
            return Resolution(
                resolved: NluPrediction(intent: NluIntentCatalog.noIntent, transcript: transcript).toWire(),
                stage: .rejectedNoIntent
            )

        case let .clarify(alternates):
            let pred = NluPrediction(
                intent: NluIntentCatalog.clarify,
                transcript: transcript,
                alternates: alternates.map { NluAlternate(intent: $0, slots: nil) }
            )
            return Resolution(resolved: pred.toWire(), stage: .rejectedClarify)

        case let .accept(intent):
            let pred = NluPrediction(intent: intent, slots: output.slots, transcript: transcript)
            return Resolution(resolved: pred.toWire(), stage: .model)
        }
    }
}
