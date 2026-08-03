import BridgethingGateway
import BridgethingSchema
import Foundation

public protocol VoiceCapturing: Sendable {
    func start(gateway: BridgethingGateway) async
    func stop() async
    func setCatalogResolver(_ provider: @escaping @Sendable () async -> (any VoiceCatalogResolving)?) async
}

@available(macOS 26.0, iOS 26.0, *)
protocol NluSpeechRecognizing: Sendable {
    func prepare() async throws
    func transcribe(fileAt url: URL) async throws -> NluSpeechRecognizer.Transcription
}

@available(macOS 26.0, iOS 26.0, *)
extension NluSpeechRecognizer: NluSpeechRecognizing {}

protocol VoicePacketDecoding: Sendable {
    func decode(_ packets: [Data], format: VoiceFormat) throws -> Data
}

struct SystemVoicePacketDecoder: VoicePacketDecoding {
    func decode(_ packets: [Data], format: VoiceFormat) throws -> Data {
        try VoicePacketDecoder(format: format).decode(packets)
    }
}

@available(macOS 26.0, iOS 26.0, *)
public actor VoiceDispatcher: VoiceCapturing {
    private struct Capture {
        let format: VoiceFormat
        var packets: [UInt32: Data] = [:]
    }

    private struct Turn: Sendable {
        let streamId: UUID
        let format: VoiceFormat
        let packets: [Data]
    }

    private let recognizer: any NluSpeechRecognizing
    private let decoder: any VoicePacketDecoding
    private let controller: VoiceController
    private var resolverProvider: (@Sendable () async -> (any VoiceCatalogResolving)?)?
    private var captures: [UUID: Capture] = [:]
    private var tasks: [Task<Void, Never>] = []
    private var prewarmRequested = false

    public init(recognizer: NluSpeechRecognizer, controller: VoiceController) {
        self.init(recognizer: recognizer, decoder: SystemVoicePacketDecoder(), controller: controller)
    }

    init(
        recognizer: any NluSpeechRecognizing,
        decoder: any VoicePacketDecoding,
        controller: VoiceController
    ) {
        self.recognizer = recognizer
        self.decoder = decoder
        self.controller = controller
    }

    public func setCatalogResolver(_ provider: @escaping @Sendable () async -> (any VoiceCatalogResolving)?) {
        resolverProvider = provider
    }

    public func start(gateway: BridgethingGateway) async {
        tasks.append(Task { [recognizer] in
            do {
                try await recognizer.prepare()
            } catch {
                print("voice: speech recognizer unavailable: \(error)")
            }
        })
        let events = gateway.events
        tasks.append(Task { [weak self] in
            await withDiscardingTaskGroup { group in
                for await event in events {
                    guard case let .message(deviceId, message) = event,
                          case let .voice(surface) = message.data
                    else { continue }
                    guard let self else { return }
                    guard let turn = await self.accept(surface) else { continue }
                    group.addTask { await self.dispatch(turn, deviceId: deviceId, gateway: gateway) }
                }
            }
        })
    }

    public func stop() async {
        tasks.forEach { $0.cancel() }
        tasks.removeAll()
        captures.removeAll()
        prewarmRequested = false
    }

    private func accept(_ msg: BridgeToGatewayVoiceMsg) -> Turn? {
        switch msg {
        case let .streamOpen(open):
            self.open(open)
        case let .frame(frame):
            append(frame)
        case let .streamClose(close):
            return closeCapture(close)
        default:
            break
        }
        return nil
    }

    private func open(_ msg: VoiceStreamOpen) {
        captures[msg.streamId] = Capture(format: msg.format)
        guard !prewarmRequested else { return }
        prewarmRequested = true
        tasks.append(Task { [controller] in await controller.prewarm() })
    }

    private func append(_ msg: VoiceFrame) {
        captures[msg.streamId]?.packets[msg.seq] = msg.packet
    }

    private func closeCapture(_ msg: VoiceStreamClose) -> Turn? {
        guard let capture = captures.removeValue(forKey: msg.streamId) else { return nil }
        guard msg.reason == .endOfSpeech else { return nil }
        return Turn(
            streamId: msg.streamId,
            format: capture.format,
            packets: capture.packets.sorted { $0.key < $1.key }.map(\.value)
        )
    }

    private nonisolated func dispatch(_ turn: Turn, deviceId: String, gateway: BridgethingGateway) async {
        do {
            try await resolveAndDispatch(transcript: await transcribe(turn), deviceId: deviceId, gateway: gateway)
        } catch {
            print("voice: dispatching turn \(turn.streamId) failed: \(error)")
        }
    }

    private nonisolated func transcribe(_ turn: Turn) async -> String {
        guard !turn.packets.isEmpty else { return "" }
        do {
            let pcm = try decoder.decode(turn.packets, format: turn.format)
            guard !pcm.isEmpty else { return "" }
            let url = try Self.writeWav(pcm: pcm, format: turn.format)
            defer { try? FileManager.default.removeItem(at: url) }
            return try await recognizer.transcribe(fileAt: url).text
        } catch {
            print("voice: capture \(turn.streamId) failed: \(error)")
            return ""
        }
    }

    nonisolated func resolveAndDispatch(transcript: String, deviceId: String, gateway: BridgethingGateway) async throws {
        let resolution: VoiceController.Resolution
        do {
            resolution = try await controller.resolve(transcript: transcript)
        } catch {
            print("voice: nlu failed: \(error)")
            resolution = VoiceController.Resolution(
                resolved: NluPrediction(intent: NluIntentCatalog.noIntent, transcript: transcript).toWire(),
                stage: .noModel
            )
        }
        var prediction = NluPrediction.fromWire(resolution.resolved)

        if let resolver = await currentResolver() {
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

    private func currentResolver() async -> (any VoiceCatalogResolving)? {
        guard let resolverProvider else { return nil }
        return await resolverProvider()
    }

    // MARK: - wav

    static let wavBitsPerSample: UInt16 = 16

    static func writeWav(pcm: Data, format: VoiceFormat) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgething-voice-\(UUID().uuidString).wav")
        var out = Data()
        let channels = UInt16(format.channels)
        let bits = Self.wavBitsPerSample
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
