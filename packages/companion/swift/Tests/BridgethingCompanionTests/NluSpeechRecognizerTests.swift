import AVFoundation
import Foundation
import Testing

@testable import BridgethingCompanion

@Suite("nlu speech recognizer", .enabled(if: ProcessInfo.processInfo.environment["BRIDGETHING_ASR_TEST"] != nil))
struct NluSpeechRecognizerTests {
    static func synthesise(_ phrase: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("asr-\(UUID().uuidString).aiff")
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/say")
        process.arguments = ["-o", url.path, phrase]
        try process.run()
        process.waitUntilExit()
        return url
    }

    @Test("transcribes a spoken command")
    func transcribesCommand() async throws {
        guard #available(macOS 26.0, iOS 26.0, *) else { return }
        let recognizer = NluSpeechRecognizer()
        try await recognizer.prepare()

        let audio = try Self.synthesise("play some jazz")
        defer { try? FileManager.default.removeItem(at: audio) }

        let result = try await recognizer.transcribe(fileAt: audio)
        let lowered = result.text.lowercased()
        #expect(lowered.contains("jazz"), "got: \(result.text)")
        #expect(lowered.contains("play"), "got: \(result.text)")
    }

    @Test("feeds the nlu pipeline end to end")
    func drivesFastPath() async throws {
        guard #available(macOS 26.0, iOS 26.0, *) else { return }
        let recognizer = NluSpeechRecognizer()
        try await recognizer.prepare()

        let audio = try Self.synthesise("pause")
        defer { try? FileManager.default.removeItem(at: audio) }

        let result = try await recognizer.transcribe(fileAt: audio)
        #expect(NluFastPath.match(result.text)?.intent == "PAUSE", "transcript was: \(result.text)")
    }
}
