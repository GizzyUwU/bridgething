import Foundation
import Testing

@testable import BridgethingCompanion

@Suite("voice controller", .enabled(if: ProcessInfo.processInfo.environment["BRIDGETHING_LLM_TEST"] != nil))
struct VoiceControllerTests {
    static func controller(useFastPath: Bool = true) -> VoiceController {
        VoiceController(
            client: NluOpenRouterClient(),
            config: .init(grammarSchema: nil, useFastPath: useFastPath)
        )
    }

    @Test("fast path short-circuits before any network call")
    func fastPathShortCircuits() async throws {
        let resolution = try await Self.controller().resolve(transcript: "Pause.")
        #expect(resolution.stage == .fastPath)
        #expect(resolution.resolved.intent == "PAUSE")
    }

    @Test("content-carrying utterance reaches the llm and extracts slots")
    func llmExtractsSlots() async throws {
        let resolution = try await Self.controller().resolve(transcript: "play some jazz by miles davis")
        #expect(resolution.stage != .fastPath)
        #expect(resolution.resolved.intent.hasPrefix("PLAY"), "intent was \(resolution.resolved.intent)")

        let artist = resolution.resolved.slots?.artist?.lowercased()
        #expect(artist?.contains("miles davis") == true, "artist slot was \(String(describing: artist))")
    }

    @Test("opening a webapp by name resolves to OPEN_WEBAPP")
    func openWebapp() async throws {
        let resolution = try await Self.controller().resolve(transcript: "switch over to the browser")
        #expect(resolution.resolved.intent == "OPEN_WEBAPP", "intent was \(resolution.resolved.intent)")
        #expect(resolution.resolved.slots?.webappName?.lowercased().contains("browser") == true,
                "webappName was \(String(describing: resolution.resolved.slots?.webappName))")
    }
}
