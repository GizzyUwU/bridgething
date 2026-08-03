import Foundation
import Testing

@testable import BridgethingCompanion

@Suite("voice controller")
struct VoiceControllerTests {
    @Test("fast path short-circuits before the model runs")
    func fastPathShortCircuits() async throws {
        let controller = VoiceController(client: FakeNluInference(failWithCall: true))
        let resolution = try await controller.resolve(transcript: "Pause.")
        #expect(resolution.stage == .fastPath)
        #expect(resolution.resolved.intent == "PAUSE")
    }

    @Test("an empty transcript is NO_INTENT without touching the model")
    func emptyTranscript() async throws {
        let controller = VoiceController(client: FakeNluInference(failWithCall: true))
        let resolution = try await controller.resolve(transcript: "   ")
        #expect(resolution.stage == .rejectedNoIntent)
        #expect(resolution.resolved.intent == "NO_INTENT")
    }

    @Test("with no model configured the fast path still resolves")
    func noModelKeepsFastPath() async throws {
        let resolution = try await VoiceController().resolve(transcript: "turn it up")
        #expect(resolution.stage == .fastPath)
        #expect(resolution.resolved.intent == "SET_VOLUME")
    }

    @Test("with no model configured a fast-path miss says so rather than guessing")
    func noModelRejectsTheTail() async throws {
        let resolution = try await VoiceController().resolve(transcript: "play the new mitski album")
        #expect(resolution.stage == .noModel)
        #expect(resolution.resolved.intent == "NO_INTENT")
        #expect(resolution.resolved.transcript == "play the new mitski album")
    }

    @Test("an accepted intent carries the decoded slots through to the wire")
    func acceptedIntentKeepsSlots() async throws {
        let client = FakeNluInference(
            logits: ["PLAY": 9],
            slots: .init(target: "you stupid bitch by girl in red", targetType: .track)
        )
        let resolution = try await VoiceController(client: client).resolve(transcript: "play you stupid bitch by girl in red")
        #expect(resolution.stage == .model)
        #expect(resolution.resolved.intent == "PLAY")
        #expect(resolution.resolved.slots.target == "you stupid bitch by girl in red")
        #expect(resolution.resolved.slots.targetType == .track)
    }

    @Test("prewarm reaches a client that can be warmed")
    func prewarmForwardsToClient() async throws {
        let client = PrewarmableNluInference()
        await VoiceController(client: client).prewarm()
        #expect(client.warmed.count == 1)
    }

    @Test("prewarm is a no-op for a client with nothing to warm")
    func prewarmIgnoresPlainClient() async throws {
        await VoiceController(client: FakeNluInference(failWithCall: true)).prewarm()
        await VoiceController().prewarm()
    }

    @Test("an out-of-domain utterance resolves to NO_INTENT")
    func outOfDomain() async throws {
        let client = FakeNluInference(logits: ["SEARCH": 5], inDomainLogit: -6)
        let resolution = try await VoiceController(client: client).resolve(transcript: "what is the capital of peru")
        #expect(resolution.stage == .rejectedNoIntent)
        #expect(resolution.resolved.intent == "NO_INTENT")
    }

    @Test("an ambiguous utterance resolves to CLARIFY with alternates and no slots")
    func ambiguous() async throws {
        let client = FakeNluInference(
            logits: ["PLAY": 4.0, "SEARCH": 3.95],
            slots: .init(target: "pink")
        )
        let resolution = try await VoiceController(client: client).resolve(transcript: "pink")
        #expect(resolution.stage == .rejectedClarify)
        #expect(resolution.resolved.intent == "CLARIFY")
        #expect(Set(resolution.resolved.alternates?.map(\.intent) ?? []) == ["PLAY", "SEARCH"])
        #expect(resolution.resolved.alternates?.allSatisfy { $0.slots == nil } == true)
    }

    @Test("the transcript rides along on every outcome")
    func transcriptPreserved() async throws {
        let client = FakeNluInference(logits: ["SEARCH": 9])
        let resolution = try await VoiceController(client: client).resolve(transcript: "search for 90s shoegaze")
        #expect(resolution.resolved.transcript == "search for 90s shoegaze")
    }

    @Test("an inference failure surfaces as a controller error")
    func inferenceFailureWraps() async throws {
        let controller = VoiceController(client: FakeNluInference(failWithCall: true))
        await #expect(throws: VoiceController.ControllerError.self) {
            try await controller.resolve(transcript: "play some jazz by miles davis")
        }
    }

    @Test("disabling the fast path routes bare transport through the model")
    func fastPathDisabled() async throws {
        let client = FakeNluInference(logits: ["PAUSE": 9])
        let controller = VoiceController(client: client, config: .init(useFastPath: false))
        let resolution = try await controller.resolve(transcript: "pause")
        #expect(resolution.stage == .model)
        #expect(resolution.resolved.intent == "PAUSE")
    }
}
