import BridgethingSchema
import Foundation

/// Orchestrates the voice-NLU pipeline on the companion. Given a
/// transcript + the list of currently-installed/active webapps, returns
/// an `NluResolvedIntent` ready to ship over the `voice.dispatch` wire
/// surface.
///
/// Stage chain:
///   1. `NluFastPath` regex match - bare PLAY/STOP/NEXT/VOLUME_*/PRESET.
///   2. LLM (`NluOpenRouterClient` against Gemma + json_schema grammar).
///   3. `NluIntentValidator.snapPrediction` - snap intent + filler-strip
///      raw_query.
///   4. SpotifyResolver decoration (Spotify Search -> uri). Wired
///      separately; resolver is opt-in.
///
/// Mirrors `nlu/scripts/pipeline.py` and `nlu/scripts/eval_reference_set.py`
/// - any logic change here should land there too so the reference-set
/// evaluator stays representative.
public actor VoiceController {
    public struct Config: Sendable {
        public let model: String
        public let grammarSchema: [String: Any]?
        public let useFastPath: Bool
        public let useSnap: Bool

        public init(
            model: String = "google/gemma-4-26b-a4b-it",
            grammarSchema: [String: Any]? = nil,
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

    /// Run the pipeline against a transcript with the currently-active
    /// webapps (system prompt context). On success the resolved intent
    /// is ready to send via the `voice.dispatch` wire surface.
    public func resolve(transcript: String, activeWebapps: [NluSystemPrompt.ActiveWebapp] = []) async throws -> Resolution {
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
                systemPrompt: NluSystemPrompt.build(activeWebapps: activeWebapps),
                utterance: trimmed,
                responseFormat: config.grammarSchema.map { schema in
                    [
                        "type": "json_schema",
                        "json_schema": [
                            "name": "intent_output",
                            "strict": true,
                            "schema": schema,
                        ],
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

/// Static helpers split out to keep the actor's `self` surface clean.
public enum NluController {
    /// Parse the LLM's JSON response into an `NluPrediction`. Tolerates
    /// the chat-completion model occasionally wrapping the JSON in
    /// markdown fences or stray text before/after.
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
            slots.rawQuery = raw["raw_query"] as? String
            slots.webappId = raw["webapp_id"] as? String
            slots.webappName = raw["webapp_name"] as? String
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
            rawQuery: raw["raw_query"] as? String,
            webappId: raw["webapp_id"] as? String,
            webappName: raw["webapp_name"] as? String,
            preset: raw["preset"] as? String,
            amount: raw["amount"] as? String,
            level: raw["level"] as? UInt32,
            uri: raw["uri"] as? String
        )
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
