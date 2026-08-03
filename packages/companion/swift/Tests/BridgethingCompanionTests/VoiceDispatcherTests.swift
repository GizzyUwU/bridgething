import BridgethingGateway
import BridgethingSchema
import BridgethingTestKit
import Foundation
import Testing

@testable import BridgethingCompanion

private struct FakeCatalogResolver: VoiceCatalogResolving {
    enum Failure: Error { case unreachable }

    let uri: String?
    let contextUri: String?
    let failure: Failure?

    init(uri: String? = nil, contextUri: String? = nil, failure: Failure? = nil) {
        self.uri = uri
        self.contextUri = contextUri
        self.failure = failure
    }

    func decorate(_ prediction: NluPrediction) async throws -> NluPrediction {
        if let failure { throw failure }
        var decorated = prediction
        decorated.slots.uri = uri
        decorated.slots.contextUri = contextUri
        return decorated
    }
}

@available(macOS 26.0, iOS 26.0, *)
private struct FakeRecognizer: NluSpeechRecognizing {
    enum Failure: Error { case analyzerDied }

    var transcript = "play hounds of love"
    var failure: Failure?

    func prepare() async throws {}

    func transcribe(fileAt _: URL) async throws -> NluSpeechRecognizer.Transcription {
        if let failure { throw failure }
        return NluSpeechRecognizer.Transcription(text: transcript)
    }
}

private final class RecordingDecoder: VoicePacketDecoding, @unchecked Sendable {
    enum Failure: Error { case codecUnavailable }

    private let lock = NSLock()
    private var turns: [[Data]] = []
    private let failure: Failure?

    init(failure: Failure? = nil) {
        self.failure = failure
    }

    func decode(_ packets: [Data], format _: VoiceFormat) throws -> Data {
        lock.lock()
        turns.append(packets)
        lock.unlock()
        if let failure { throw failure }
        return Data(repeating: 0, count: 640)
    }

    var recorded: [[Data]] {
        lock.lock()
        defer { lock.unlock() }
        return turns
    }
}

@Suite("voice dispatcher")
struct VoiceDispatcherTests {
    @available(macOS 26.0, iOS 26.0, *)
    private struct Harness {
        let dispatcher: VoiceDispatcher
        let gateway: BridgethingGateway
        let driver: WireDriver
    }

    @available(macOS 26.0, iOS 26.0, *)
    private func boot(resolver: (any VoiceCatalogResolving)?) async throws -> Harness {
        let adapter = InMemoryAdapter()
        let gateway = BridgethingGateway(adapter: adapter)
        try await gateway.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()

        let dispatcher = VoiceDispatcher(
            recognizer: NluSpeechRecognizer(),
            controller: VoiceController(
                client: FakeNluInference(logits: ["PLAY": 9], slots: NluMutableSlots(target: "hounds of love")),
                config: .init(useFastPath: false)
            )
        )
        if let resolver {
            await dispatcher.setCatalogResolver { resolver }
        }
        return Harness(dispatcher: dispatcher, gateway: gateway, driver: driver)
    }

    @available(macOS 26.0, iOS 26.0, *)
    private func dispatchedIntent(_ h: Harness) async throws -> NluResolvedIntent {
        try await h.dispatcher.resolveAndDispatch(
            transcript: "play hounds of love", deviceId: h.driver.deviceId, gateway: h.gateway
        )
        let frame = try await h.driver.waitOutbound(timeout: .seconds(5)) {
            if case .voice(.dispatch) = $0.data { return true }
            return false
        }
        guard case let .voice(.dispatch(d)) = frame.data else { throw WireDriverError.decodeFailed }
        return d.resolved
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("the resolver decorates the turn before it goes on the wire")
    func resolverDecoratesBeforeDispatch() async throws {
        let h = try await boot(resolver: FakeCatalogResolver(uri: "spotify:track:7", contextUri: "spotify:album:2"))
        let resolved = try await dispatchedIntent(h)
        #expect(resolved.slots.target == "hounds of love")
        #expect(resolved.slots.uri == "spotify:track:7")
        #expect(resolved.slots.contextUri == "spotify:album:2")
        await h.driver.stop()
        await h.gateway.stop()
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("no resolver still dispatches, with the slots left unresolved")
    func absentResolverStillDispatches() async throws {
        let h = try await boot(resolver: nil)
        let resolved = try await dispatchedIntent(h)
        #expect(resolved.intent == "PLAY")
        #expect(resolved.slots.uri == nil)
        await h.driver.stop()
        await h.gateway.stop()
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("a failing resolver still dispatches so the daemon answers the turn")
    func failingResolverStillDispatches() async throws {
        let h = try await boot(resolver: FakeCatalogResolver(failure: .unreachable))
        let resolved = try await dispatchedIntent(h)
        #expect(resolved.intent == "PLAY")
        #expect(resolved.slots.target == "hounds of love")
        #expect(resolved.slots.uri == nil, "a failed resolution must not invent a uri")
        await h.driver.stop()
        await h.gateway.stop()
    }

    // MARK: - capture path

    @available(macOS 26.0, iOS 26.0, *)
    private struct CaptureHarness {
        let dispatcher: VoiceDispatcher
        let gateway: BridgethingGateway
        let driver: WireDriver
        let adapter: InMemoryAdapter
        let decoder: RecordingDecoder
        let codec = Codec()

        func shutdown() async {
            await dispatcher.stop()
            await driver.stop()
            await gateway.stop()
        }

        func nextDispatch(timeout: Duration = .seconds(5)) async throws -> VoiceDispatch {
            let frame = try await driver.waitOutbound(timeout: timeout) {
                if case .voice(.dispatch) = $0.data { return true }
                return false
            }
            guard case let .voice(.dispatch(d)) = frame.data else { throw WireDriverError.decodeFailed }
            return d
        }

        func turn(_ streamId: UUID, packets: [Data], reason: VoiceCloseReason = .endOfSpeech) async throws {
            let format = VoiceOpusFixture.format
            try await driver.send(.voice(.streamOpen(VoiceStreamOpen(streamId: streamId, format: format))), meta: .event)
            for (seq, packet) in packets.enumerated() {
                try await driver.send(
                    .voice(.frame(VoiceFrame(streamId: streamId, seq: UInt32(seq), packet: packet))), meta: .event
                )
            }
            try await driver.send(.voice(.streamClose(VoiceStreamClose(streamId: streamId, reason: reason))), meta: .event)
        }

        func burst(_ surfaces: [BridgeToGatewayVoiceMsg]) async throws {
            var bytes = Data()
            for surface in surfaces {
                bytes.append(try codec.encode(BridgeToGatewayMsg(id: UUID(), meta: .event, data: .voice(surface))))
            }
            adapter.feed(deviceId: await driver.deviceId, bytes)
        }
    }

    @available(macOS 26.0, iOS 26.0, *)
    private func bootCapture(
        recognizer: FakeRecognizer = FakeRecognizer(),
        decoder: RecordingDecoder = RecordingDecoder(),
        inference: FakeNluInference = FakeNluInference(logits: ["PLAY": 9], slots: NluMutableSlots(target: "hounds of love"))
    ) async throws -> CaptureHarness {
        let adapter = InMemoryAdapter()
        let gateway = BridgethingGateway(adapter: adapter)
        try await gateway.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()

        let dispatcher = VoiceDispatcher(
            recognizer: recognizer,
            decoder: decoder,
            controller: VoiceController(client: inference, config: .init(useFastPath: false))
        )
        await dispatcher.start(gateway: gateway)
        return CaptureHarness(
            dispatcher: dispatcher, gateway: gateway, driver: driver, adapter: adapter, decoder: decoder
        )
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("a failed decode still answers the turn")
    func failedDecodeStillDispatches() async throws {
        let h = try await bootCapture(decoder: RecordingDecoder(failure: .codecUnavailable))
        try await h.turn(UUID(), packets: [Data([1, 2, 3])])

        let dispatched = try await h.nextDispatch()
        #expect(dispatched.resolved.intent == NluIntentCatalog.noIntent)
        #expect(dispatched.stage == .rejectedNoIntent)
        await h.shutdown()
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("a throwing recognizer still answers the turn")
    func failedRecognizerStillDispatches() async throws {
        let h = try await bootCapture(recognizer: FakeRecognizer(failure: .analyzerDied))
        try await h.turn(UUID(), packets: [Data([1, 2, 3])])

        let dispatched = try await h.nextDispatch()
        #expect(dispatched.resolved.intent == NluIntentCatalog.noIntent)
        #expect(dispatched.stage == .rejectedNoIntent)
        await h.shutdown()
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("a capture that carried no packets answers no intent")
    func emptyCaptureStillDispatches() async throws {
        let h = try await bootCapture()
        try await h.turn(UUID(), packets: [])

        let dispatched = try await h.nextDispatch()
        #expect(dispatched.resolved.intent == NluIntentCatalog.noIntent)
        #expect(dispatched.stage == .rejectedNoIntent)
        #expect(dispatched.resolved.transcript.isEmpty)
        await h.shutdown()
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("a failed model still answers the turn")
    func failedModelStillDispatches() async throws {
        let h = try await bootCapture(inference: FakeNluInference(failWithCall: true))
        try await h.turn(UUID(), packets: [Data([1, 2, 3])])

        let dispatched = try await h.nextDispatch()
        #expect(dispatched.resolved.intent == NluIntentCatalog.noIntent)
        #expect(dispatched.stage == .noModel)
        #expect(dispatched.resolved.transcript == "play hounds of love", "the transcript survives a model that failed")
        await h.shutdown()
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("a cancelled capture drops without answering")
    func cancelledCaptureNeverDispatches() async throws {
        let h = try await bootCapture()
        try await h.turn(UUID(), packets: [Data([1, 2, 3])], reason: .cancelled)

        await #expect(throws: WireDriverError.self) { try await h.nextDispatch(timeout: .milliseconds(400)) }
        await h.shutdown()
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("two turns interleaved on the wire both dispatch, with every packet")
    func interleavedTurnsKeepEveryPacket() async throws {
        let h = try await bootCapture()
        let first = UUID()
        let second = UUID()
        let count = 200
        func packets(_ tag: UInt8) -> [Data] {
            (0..<count).map { Data([tag, UInt8($0 & 0xFF), UInt8($0 >> 8)]) }
        }
        let firstPackets = packets(0xA0)
        let secondPackets = packets(0xB0)

        var surfaces: [BridgeToGatewayVoiceMsg] = [
            .streamOpen(VoiceStreamOpen(streamId: first, format: VoiceOpusFixture.format)),
            .streamOpen(VoiceStreamOpen(streamId: second, format: VoiceOpusFixture.format)),
        ]
        for seq in 0..<count {
            surfaces.append(.frame(VoiceFrame(streamId: first, seq: UInt32(seq), packet: firstPackets[seq])))
            surfaces.append(.frame(VoiceFrame(streamId: second, seq: UInt32(seq), packet: secondPackets[seq])))
        }
        surfaces.append(.streamClose(VoiceStreamClose(streamId: first, reason: .endOfSpeech)))
        surfaces.append(.streamClose(VoiceStreamClose(streamId: second, reason: .endOfSpeech)))
        try await h.burst(surfaces)

        _ = try await h.nextDispatch()
        _ = try await h.nextDispatch()

        let recorded = h.decoder.recorded
        #expect(recorded.count == 2, "both turns have to reach the decoder")
        #expect(recorded.contains(firstPackets), "the first turn lost packets: \(recorded.map(\.count))")
        #expect(recorded.contains(secondPackets), "the second turn lost packets: \(recorded.map(\.count))")
        await h.shutdown()
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("a capture already in flight when the dispatcher starts keeps every packet")
    func captureAtStartupKeepsEveryPacket() async throws {
        let adapter = InMemoryAdapter()
        let gateway = BridgethingGateway(adapter: adapter)
        try await gateway.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        let deviceId = await driver.deviceId

        let decoder = RecordingDecoder()
        let dispatcher = VoiceDispatcher(
            recognizer: FakeRecognizer(),
            decoder: decoder,
            controller: VoiceController(client: FakeNluInference(logits: ["PLAY": 9]), config: .init(useFastPath: false))
        )

        let streamId = UUID()
        let packets = (0..<400).map { Data([0xC0, UInt8($0 & 0xFF), UInt8($0 >> 8)]) }
        var surfaces: [BridgeToGatewayVoiceMsg] = [
            .streamOpen(VoiceStreamOpen(streamId: streamId, format: VoiceOpusFixture.format)),
        ]
        for (seq, packet) in packets.enumerated() {
            surfaces.append(.frame(VoiceFrame(streamId: streamId, seq: UInt32(seq), packet: packet)))
        }
        surfaces.append(.streamClose(VoiceStreamClose(streamId: streamId, reason: .endOfSpeech)))

        let codec = Codec()
        var bytes = Data()
        for surface in surfaces {
            bytes.append(try codec.encode(BridgeToGatewayMsg(id: UUID(), meta: .event, data: .voice(surface))))
        }

        await dispatcher.start(gateway: gateway)
        adapter.feed(deviceId: deviceId, bytes)

        _ = try await driver.waitOutbound(timeout: .seconds(5)) {
            if case .voice(.dispatch) = $0.data { return true }
            return false
        }
        #expect(decoder.recorded == [packets], "the turn lost packets: \(decoder.recorded.map(\.count))")
        await dispatcher.stop()
        await driver.stop()
        await gateway.stop()
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("wav header describes the pcm the decoder produced")
    func wavHeader() throws {
        let format = VoiceOpusFixture.format
        let pcm = Data(repeating: 0xAB, count: 640)
        let url = try VoiceDispatcher.writeWav(pcm: pcm, format: format)
        defer { try? FileManager.default.removeItem(at: url) }

        let bytes = try Data(contentsOf: url)
        #expect(bytes.count == 44 + pcm.count)

        func u32(_ at: Int) -> UInt32 {
            bytes[at..<(at + 4)].reduce(into: UInt32(0)) { acc, b in acc = acc >> 8 | UInt32(b) << 24 }
        }
        func u16(_ at: Int) -> UInt16 {
            UInt16(bytes[at]) | UInt16(bytes[at + 1]) << 8
        }

        #expect(String(decoding: bytes[0..<4], as: UTF8.self) == "RIFF")
        #expect(String(decoding: bytes[8..<12], as: UTF8.self) == "WAVE")
        #expect(u16(22) == 1, "channel count")
        #expect(u32(24) == 16000, "sample rate")
        #expect(u16(34) == 16, "bits per sample")
        #expect(u32(28) == 16000 * 2, "byte rate")
        #expect(u16(32) == 2, "block align")
        #expect(u32(40) == UInt32(pcm.count), "data chunk size")
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("a multichannel format widens block align rather than assuming mono")
    func wavHeaderMultichannel() throws {
        let format = VoiceFormat(codec: .opus, sampleRateHz: 48000, channels: 2)
        let url = try VoiceDispatcher.writeWav(pcm: Data(repeating: 0, count: 64), format: format)
        defer { try? FileManager.default.removeItem(at: url) }
        let bytes = try Data(contentsOf: url)
        let blockAlign = UInt16(bytes[32]) | UInt16(bytes[33]) << 8
        #expect(blockAlign == 4, "2ch x 16bit is 4 bytes per frame")
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("opus packets decode to a wav the recognizer can open")
    func opusPacketsDecodeToWav() throws {
        let packets = VoiceOpusFixture.packets
        let decoded = try SystemVoicePacketDecoder().decode(packets, format: VoiceOpusFixture.format)
        let url = try VoiceDispatcher.writeWav(pcm: decoded, format: VoiceOpusFixture.format)
        defer { try? FileManager.default.removeItem(at: url) }

        let bytes = try Data(contentsOf: url)
        let pcm = VoiceOpusFixture.samples(inWav: bytes)
        let encoded = packets.reduce(0) { $0 + $1.count }

        #expect(bytes.count == 44 + pcm.count * 2, "the header has to describe the decoded body")
        let expected = packets.count * VoiceOpusFixture.samplesPerPacket
        #expect(pcm.count > expected - 200 && pcm.count <= expected, "decoded \(pcm.count) of \(expected)")
        #expect(
            pcm.count * 2 > encoded * 8,
            "opus has to be a real saving over the pcm it replaces: \(encoded) bytes for \(pcm.count) samples"
        )
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("the decoded waveform is the tone that was encoded, not noise")
    func opusDecodePreservesTheTone() throws {
        let pcm = try VoicePacketDecoder(format: VoiceOpusFixture.format).decode(VoiceOpusFixture.packets)
        let samples = pcm.withUnsafeBytes { Array($0.bindMemory(to: Int16.self)) }

        let tone = VoiceOpusFixture.energy(of: samples, atHz: VoiceOpusFixture.toneHz)
        let elsewhere = VoiceOpusFixture.energy(of: samples, atHz: 1500)
        #expect(tone > elsewhere * 100, "440 hz carried \(tone), 1500 hz carried \(elsewhere)")
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("packets are decoded in sequence order however they arrived")
    func opusPacketsAreOrderedBySequence() throws {
        let ordered = try VoicePacketDecoder(format: VoiceOpusFixture.format).decode(VoiceOpusFixture.packets)

        var shuffled: [UInt32: Data] = [:]
        for (seq, packet) in VoiceOpusFixture.packets.enumerated() { shuffled[UInt32(seq)] = packet }
        let resorted = shuffled.sorted { $0.key < $1.key }.map(\.value)
        let replayed = try VoicePacketDecoder(format: VoiceOpusFixture.format).decode(resorted)

        #expect(replayed == ordered, "reassembly by seq has to reproduce the capture order exactly")
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("a turn with no packets never reaches the decoder")
    func emptyTurnDecodesToNothing() throws {
        let pcm = try VoicePacketDecoder(format: VoiceOpusFixture.format).decode([])
        #expect(pcm.isEmpty)
    }
}
