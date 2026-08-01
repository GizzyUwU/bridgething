import BridgethingGateway
import BridgethingSchema
import Foundation

public protocol VoiceCapturing: Sendable {
    func start(gateway: BridgethingGateway) async
    func stop() async
}

@available(macOS 26.0, iOS 26.0, *)
public actor VoiceDispatcher: VoiceCapturing {
    private struct Capture {
        let format: VoiceFormat
        var frames: [UInt32: Data] = [:]
    }

    private let recognizer: NluSpeechRecognizer
    private let controller: VoiceController
    private let resolver: SpotifyResolver?
    private var captures: [UUID: Capture] = [:]
    private var tasks: [Task<Void, Never>] = []

    public init(recognizer: NluSpeechRecognizer, controller: VoiceController, resolver: SpotifyResolver? = nil) {
        self.recognizer = recognizer
        self.controller = controller
        self.resolver = resolver
    }

    public func start(gateway: BridgethingGateway) async {
        tasks.append(Task { [recognizer] in
            do {
                try await recognizer.prepare()
            } catch {
                print("voice: speech recognizer unavailable: \(error)")
            }
        })
        tasks.append(Task { [weak self] in
            for await (deviceId, msg) in gateway.voice.streamOpen {
                await self?.open(deviceId: deviceId, msg: msg)
            }
        })
        tasks.append(Task { [weak self] in
            for await (_, msg) in gateway.voice.frame {
                await self?.append(msg)
            }
        })
        tasks.append(Task { [weak self] in
            for await (deviceId, msg) in gateway.voice.streamClose {
                await self?.close(deviceId: deviceId, msg: msg, gateway: gateway)
            }
        })
    }

    public func stop() async {
        tasks.forEach { $0.cancel() }
        tasks.removeAll()
        captures.removeAll()
    }

    private func open(deviceId: String, msg: VoiceStreamOpen) {
        captures[msg.streamId] = Capture(format: msg.format)
    }

    private func append(_ msg: VoiceFrame) {
        captures[msg.streamId]?.frames[msg.seq] = msg.pcm
    }

    private func close(deviceId: String, msg: VoiceStreamClose, gateway: BridgethingGateway) async {
        guard let capture = captures.removeValue(forKey: msg.streamId) else { return }
        guard msg.reason == .endOfSpeech else { return }

        let pcm = capture.frames.sorted { $0.key < $1.key }.reduce(into: Data()) { $0.append($1.value) }
        guard !pcm.isEmpty else { return }

        do {
            let url = try Self.writeWav(pcm: pcm, format: capture.format)
            defer { try? FileManager.default.removeItem(at: url) }
            let transcription = try await recognizer.transcribe(fileAt: url)
            try await resolveAndDispatch(transcript: transcription.text, deviceId: deviceId, gateway: gateway)
        } catch {
            print("voice: capture \(msg.streamId) failed: \(error)")
        }
    }

    func resolveAndDispatch(transcript: String, deviceId: String, gateway: BridgethingGateway) async throws {
        let resolution = try await controller.resolve(transcript: transcript)
        var prediction = NluPrediction.fromWire(resolution.resolved)

        if let resolver {
            do {
                prediction = try await resolver.decorate(prediction)
            } catch {
                print("voice: catalog resolution failed, dispatching without a uri: \(error)")
            }
        }

        try await gateway.device(deviceId).voice.dispatch(
            VoiceDispatch(resolved: prediction.toWire(), stage: resolution.stage.wire)
        )
    }

    // MARK: - wav

    static func writeWav(pcm: Data, format: VoiceFormat) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgething-voice-\(UUID().uuidString).wav")
        var out = Data()
        let channels = UInt16(format.channels)
        let bits = UInt16(format.bitsPerSample)
        let rate = UInt32(format.sampleRateHz)
        let blockAlign = channels * bits / 8
        let byteRate = rate * UInt32(blockAlign)

        func append32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
        func append16(_ v: UInt16) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }

        out.append(contentsOf: Array("RIFF".utf8))
        append32(UInt32(36 + pcm.count))
        out.append(contentsOf: Array("WAVEfmt ".utf8))
        append32(16)
        append16(1)
        append16(channels)
        append32(rate)
        append32(byteRate)
        append16(blockAlign)
        append16(bits)
        out.append(contentsOf: Array("data".utf8))
        append32(UInt32(pcm.count))
        out.append(pcm)

        try out.write(to: url)
        return url
    }
}
