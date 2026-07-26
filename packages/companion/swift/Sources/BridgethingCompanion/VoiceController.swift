import BridgethingSchema
import Foundation

public actor VoiceController {
    public struct Config: Sendable {
        public let model: String
        public let grammarSchema: Data?
        public let useFastPath: Bool
        public let useSnap: Bool

        public init(
            model: String = "google/gemma-4-26b-a4b-it",
            grammarSchema: Data? = nil,
            useFastPath: Bool = true,
            useSnap: Bool = true
        ) {
            self.model = model
            self.grammarSchema = grammarSchema
            self.useFastPath = useFastPath
            self.useSnap = useSnap
        }
    }

    public enum Stage: String, Sendable {
        case fastPath
        case llm
        case llmSnapped
        case llmSnapDropped
        case llmFail
    }

    public struct Resolution: Sendable {
        public let resolved: NluResolvedIntent
        public let stage: Stage
        public let snapReason: NluIntentValidator.SnapReason?

        public init(resolved: NluResolvedIntent, stage: Stage, snapReason: NluIntentValidator.SnapReason? = nil) {
            self.resolved = resolved
            self.stage = stage
            self.snapReason = snapReason
        }
    }

    public enum ControllerError: Error, CustomStringConvertible {
        case llmFailed(Error)
        case llmEmpty
        case snapDropped(NluIntentValidator.SnapReason)

        public var description: String {
            switch self {
            case let .llmFailed(err): return "llm call failed: \(err)"
            case .llmEmpty: return "llm returned no parseable JSON"
            case let .snapDropped(reason): return "intent snap dropped prediction: \(reason)"
            }
        }
    }

    private let client: NluOpenRouterClient
    private let config: Config

    public init(client: NluOpenRouterClient, config: Config = Config()) {
        self.client = client
        self.config = config
    }

    public func resolve(transcript: String) async throws -> Resolution {
        let trimmed = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return Resolution(
                resolved: NluPrediction(intent: "NO_INTENT", transcript: transcript).toWire(),
                stage: .fastPath
            )
        }

        if config.useFastPath, let hit = NluFastPath.match(trimmed) {
            let pred = NluPrediction(intent: hit.intent, slots: hit.slots, transcript: transcript)
            return Resolution(resolved: pred.toWire(), stage: .fastPath)
        }

        let llmText: String
        do {
            let completion = try await client.chat(
                model: config.model,
                systemPrompt: NluSystemPrompt.build(),
                utterance: trimmed,
                responseFormat: config.grammarSchema.flatMap { data -> [String: Any]? in
                    guard let schema = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                        return nil
                    }
                    return [
                        "type": "json_schema",
                        "json_schema": ["name": "intent_output", "strict": true, "schema": schema],
                    ]
                }
            )
            llmText = completion.text
        } catch {
            throw ControllerError.llmFailed(error)
        }

        guard var pred = NluController.parsePrediction(text: llmText, transcript: transcript) else {
            throw ControllerError.llmEmpty
        }

        if config.useSnap {
            let (snapped, reason) = NluIntentValidator.snapPrediction(pred)
            guard let snapped else {
                throw ControllerError.snapDropped(reason)
            }
            pred = snapped
            return Resolution(
                resolved: pred.toWire(),
                stage: reason == .exact ? .llm : .llmSnapped,
                snapReason: reason
            )
        }
        return Resolution(resolved: pred.toWire(), stage: .llm)
    }
}

public enum NluController {
    public static func parsePrediction(text: String, transcript: String) -> NluPrediction? {
        let cleaned = stripJsonFences(text)
        guard let data = cleaned.data(using: .utf8) else { return nil }
        guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        guard let intent = obj["intent"] as? String else { return nil }

        var slots = NluMutableSlots()
        if let raw = obj["slots"] as? [String: Any] {
            slots.artist = raw["artist"] as? String
            slots.track = raw["track"] as? String
            slots.album = raw["album"] as? String
            slots.playlist = raw["playlist"] as? String
            slots.podcast = raw["podcast"] as? String
            slots.episode = raw["episode"] as? String
            slots.mood = raw["mood"] as? String
            slots.genre = raw["genre"] as? String
            slots.era = raw["era"] as? String
            slots.popularityFilter = raw["popularity_filter"] as? String
            slots.entityType = raw["entity_type"] as? String
            slots.query = raw["query"] as? String
            slots.webappName = raw["webapp_name"] as? String
            slots.enabled = raw["enabled"] as? Bool
            slots.repeatMode = (raw["repeat_mode"] as? String).flatMap(NluRepeatMode.init(rawValue:))
            slots.seconds = raw["seconds"].flatMap { v -> Int32? in
                if let n = v as? Int { return Int32(n) }
                if let s = v as? String, let n = Int32(s) { return n }
                return nil
            }
            slots.speed = raw["speed"].flatMap { v -> NluPlaybackSpeed? in
                if let s = v as? String { return NluPlaybackSpeed(rawValue: s) }
                if let n = v as? Double { return NluPlaybackSpeed(rawValue: n == n.rounded() ? String(Int(n)) : String(n)) }
                return nil
            }
            slots.direction = (raw["direction"] as? String).flatMap(NluDirection.init(rawValue:))
            slots.brightnessMode = (raw["brightness_mode"] as? String).flatMap(NluBrightnessMode.init(rawValue:))
            slots.view = (raw["view"] as? String).flatMap { NluView(rawValue: camelCased($0)) }
            slots.phoneAction = (raw["phone_action"] as? String).flatMap(NluPhoneAction.init(rawValue:))
            slots.systemAction = (raw["system_action"] as? String).flatMap { NluSystemAction(rawValue: camelCased($0)) }
            slots.preset = raw["preset"].flatMap { v -> String? in
                if let s = v as? String { return s }
                if let n = v as? Int { return String(n) }
                return nil
            }
            slots.amount = raw["amount"].flatMap { v -> String? in
                if let s = v as? String { return s }
                if let n = v as? Int { return String(n) }
                return nil
            }
            slots.level = raw["level"].flatMap { v -> UInt32? in
                if let n = v as? Int { return UInt32(n) }
                if let s = v as? String, let n = UInt32(s) { return n }
                return nil
            }
            slots.uri = raw["uri"] as? String
        }

        var confidence: NluConfidence? = nil
        if let c = obj["confidence"] as? [String: Any], let i = c["intent"] as? String {
            confidence = NluConfidence(intent: i, slots: c["slots"] as? String)
        }

        var alternates: [NluAlternate]? = nil
        if let arr = obj["ambiguous_alternates"] as? [[String: Any]] {
            alternates = arr.compactMap { entry -> NluAlternate? in
                guard let i = entry["intent"] as? String else { return nil }
                let s = entry["slots"] as? [String: Any]
                return NluAlternate(intent: i, slots: s.map(parseSlotsWire))
            }
        }

        return NluPrediction(
            intent: intent,
            slots: slots,
            transcript: transcript,
            confidence: confidence,
            alternates: alternates
        )
    }

    static func parseSlotsWire(_ raw: [String: Any]) -> NluSlots {
        NluSlots(
            artist: raw["artist"] as? String,
            track: raw["track"] as? String,
            album: raw["album"] as? String,
            playlist: raw["playlist"] as? String,
            podcast: raw["podcast"] as? String,
            episode: raw["episode"] as? String,
            mood: raw["mood"] as? String,
            genre: raw["genre"] as? String,
            era: raw["era"] as? String,
            popularityFilter: raw["popularity_filter"] as? String,
            entityType: raw["entity_type"] as? String,
            query: raw["query"] as? String,
            webappName: raw["webapp_name"] as? String,
            preset: raw["preset"] as? String,
            enabled: raw["enabled"] as? Bool,
            repeatMode: (raw["repeat_mode"] as? String).flatMap(NluRepeatMode.init(rawValue:)),
            seconds: (raw["seconds"] as? Int).map(Int32.init),
            speed: (raw["speed"] as? String).flatMap(NluPlaybackSpeed.init(rawValue:)),
            direction: (raw["direction"] as? String).flatMap(NluDirection.init(rawValue:)),
            amount: raw["amount"] as? String,
            level: raw["level"] as? UInt32,
            brightnessMode: (raw["brightness_mode"] as? String).flatMap(NluBrightnessMode.init(rawValue:)),
            view: (raw["view"] as? String).flatMap { NluView(rawValue: camelCased($0)) },
            phoneAction: (raw["phone_action"] as? String).flatMap(NluPhoneAction.init(rawValue:)),
            systemAction: (raw["system_action"] as? String).flatMap { NluSystemAction(rawValue: camelCased($0)) },
            uri: raw["uri"] as? String
        )
    }

    static func camelCased(_ raw: String) -> String {
        let parts = raw.split(separator: "_")
        guard let first = parts.first else { return raw }
        return ([String(first)] + parts.dropFirst().map { $0.capitalized }).joined()
    }

    static func stripJsonFences(_ text: String) -> String {
        var s = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.hasPrefix("```") {
            if let firstNewline = s.firstIndex(of: "\n") {
                s = String(s[s.index(after: firstNewline)...])
            }
            if s.hasSuffix("```") {
                s = String(s.dropLast(3)).trimmingCharacters(in: .whitespacesAndNewlines)
            }
        }
        return s
    }
}
