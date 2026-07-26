import AVFoundation
import Foundation
import Testing

@testable import BridgethingCompanion

@Suite("device voice end to end",
       .enabled(if: ProcessInfo.processInfo.environment["BRIDGETHING_DEVICE_TEST"] != nil))
struct DeviceVoiceEndToEndTests {
    static var captureSeconds: Int {
        Int(ProcessInfo.processInfo.environment["BRIDGETHING_CAPTURE_SECONDS"] ?? "") ?? 5
    }

    enum CaptureError: Error { case deviceUnreachable(Int32) }

    static func captureFromDevice(seconds: Int) throws -> URL {
        let script = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Documents/carthing/yocto-superbird/scripts/superbird-ssh")
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("device-\(UUID().uuidString).wav")
        FileManager.default.createFile(atPath: url.path, contents: nil)

        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }

        let process = Process()
        process.executableURL = script
        process.arguments = ["arecord -D hw:0,0 -f S32_LE -r 16000 -c 1 -d \(seconds) -t wav - 2>/dev/null"]
        process.standardOutput = handle
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw CaptureError.deviceUnreachable(process.terminationStatus)
        }
        return url
    }

    @Test("speak to the device and get a resolved intent")
    func speakAndResolve() async throws {
        guard #available(macOS 26.0, iOS 26.0, *) else { return }

        let seconds = Self.captureSeconds
        print("\n>>> SPEAK NOW into the Car Thing (\(seconds)s) <<<\n")
        let audio = try Self.captureFromDevice(seconds: seconds)
        defer { try? FileManager.default.removeItem(at: audio) }

        let recognizer = NluSpeechRecognizer()
        try await recognizer.prepare()
        let transcription = try await recognizer.transcribe(fileAt: audio)
        print("transcript   : \(transcription.text)")
        print("alternatives : \(transcription.alternatives)")
        print("confidence   : \(String(describing: transcription.confidence))")

        let controller = VoiceController(client: NluOpenRouterClient(), config: .init(grammarSchema: nil))
        let resolution = try await controller.resolve(transcript: transcription.text)
        print("stage        : \(resolution.stage.rawValue)")
        print("intent       : \(resolution.resolved.intent)")
        print("slots        : \(String(describing: resolution.resolved.slots))")

        #expect(!transcription.text.isEmpty, "nothing was transcribed")
        #expect(resolution.resolved.intent != "NO_INTENT", "resolved to NO_INTENT")
    }
}
