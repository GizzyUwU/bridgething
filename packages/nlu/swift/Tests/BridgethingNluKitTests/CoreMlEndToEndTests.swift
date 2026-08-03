import BridgethingCompanion
import CoreML
import Foundation
import Nlu
import Testing

@testable import BridgethingNluKit

@Suite("coreml end to end")
struct CoreMlEndToEndTests {
    static func inference() throws -> NluBundleInference? {
        let env = ProcessInfo.processInfo.environment
        guard let bundle = env["BRIDGETHING_NLU_BUNDLE"], let package = env["BRIDGETHING_NLU_MLPACKAGE"] else {
            print("BRIDGETHING_NLU_BUNDLE / BRIDGETHING_NLU_MLPACKAGE unset; skipping")
            return nil
        }
        let decoder = try NluDecoder.load(bundleDir: bundle)
        let compiled = try MLModel.compileModel(at: URL(fileURLWithPath: package))
        let model = CoreMlNluModel(
            model: try MLModel(contentsOf: compiled),
            closedHeadCount: decoder.info().closedHeadSizes.count
        )
        return try NluBundleInference(decoder: decoder, model: model)
    }

    @Test("a catalog play resolves with target and type")
    func catalogPlay() async throws {
        guard let inference = try Self.inference() else { return }
        let controller = VoiceController(
            client: inference,
            config: .init(rejection: inference.rejection ?? .init())
        )
        let resolution = try await controller.resolve(transcript: "play the album 1989 by taylor swift")
        #expect(resolution.stage == .model)
        #expect(resolution.resolved.intent == "PLAY")
        #expect(resolution.resolved.slots.target?.lowercased().contains("1989") == true)
        #expect(resolution.resolved.slots.targetType == .album)
    }

    @Test("out-of-domain speech rejects instead of acting")
    func outOfDomain() async throws {
        guard let inference = try Self.inference() else { return }
        let controller = VoiceController(
            client: inference,
            config: .init(rejection: inference.rejection ?? .init())
        )
        let resolution = try await controller.resolve(transcript: "what is the capital of peru")
        #expect(resolution.stage == .rejectedNoIntent)
        #expect(resolution.resolved.intent == "NO_INTENT")
    }
}
