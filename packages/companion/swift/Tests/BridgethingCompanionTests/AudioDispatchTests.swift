import BridgethingGateway
import BridgethingSchema
import BridgethingTestKit
import Foundation
import XCTest

@testable import BridgethingCompanion

final class AudioDispatchTests: XCTestCase {
    private func boot(_ backend: FakeAudioBackend) async throws -> (BridgethingCompanion, WireDriver) {
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "audio-test", appVersion: "0.0.1", osName: "macOS"),
            audioBackend: backend
        )
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        return (companion, driver)
    }

    func testSetVolumeRoutesToBackend() async throws {
        let backend = FakeAudioBackend()
        let (companion, driver) = try await boot(backend)
        try await driver.send(.audio(.setVolume(SetVolume(level: 0.42))))
        try await eventually { await backend.setVolumeCalls == [0.42] }
        await companion.stop()
    }

    func testSetMuteAndVolumeStepRouteToBackend() async throws {
        let backend = FakeAudioBackend()
        let (companion, driver) = try await boot(backend)
        try await driver.send(.audio(.setMute(SetMute(muted: true))))
        try await driver.send(.audio(.volumeUp))
        try await driver.send(.audio(.muteToggle))
        try await eventually {
            let mute = await backend.setMuteCalls
            let up = await backend.volumeUpCount
            let toggle = await backend.muteToggleCount
            return mute == [true] && up == 1 && toggle == 1
        }
        await companion.stop()
    }

    func testTtsEmitsStartedAndEndedCompleted() async throws {
        let backend = FakeAudioBackend()
        let (companion, driver) = try await boot(backend)
        let id = UUID()
        try await driver.send(.audio(.tts(Tts(id: id, text: "hello", voice: nil))))

        let started = try await driver.waitOutbound(timeout: .seconds(5)) { msg in
            if case let .audio(.ttsStarted(s)) = msg.data { return s.id == id }
            return false
        }
        guard case .audio(.ttsStarted) = started.data else {
            await companion.stop(); return XCTFail("expected ttsStarted")
        }

        let ended = try await driver.waitOutbound(timeout: .seconds(5)) { msg in
            if case let .audio(.ttsEnded(e)) = msg.data { return e.id == id }
            return false
        }
        guard case let .audio(.ttsEnded(e)) = ended.data else {
            await companion.stop(); return XCTFail("expected ttsEnded")
        }
        XCTAssertTrue(e.completed, "uncancelled speech should end completed")
        await companion.stop()
    }

    func testTtsCancelEndsIncomplete() async throws {
        let backend = FakeAudioBackend(blockUntilCancel: true)
        let (companion, driver) = try await boot(backend)
        let id = UUID()
        try await driver.send(.audio(.tts(Tts(id: id, text: "long sentence", voice: nil))))
        _ = try await driver.waitOutbound(timeout: .seconds(5)) { msg in
            if case let .audio(.ttsStarted(s)) = msg.data { return s.id == id }
            return false
        }

        try await driver.send(.audio(.ttsCancel(TtsCancel(id: id))))
        let ended = try await driver.waitOutbound(timeout: .seconds(5)) { msg in
            if case let .audio(.ttsEnded(e)) = msg.data { return e.id == id }
            return false
        }
        guard case let .audio(.ttsEnded(e)) = ended.data else {
            await companion.stop(); return XCTFail("expected ttsEnded")
        }
        XCTAssertFalse(e.completed, "cancelled speech should end not-completed")
        await companion.stop()
    }

    func testEarconRoutesToBackend() async throws {
        let backend = FakeAudioBackend()
        let (companion, driver) = try await boot(backend)
        try await driver.send(.audio(.earcon(Earcon(name: "confirm"))))
        try await eventually { await backend.earconNames == ["confirm"] }
        await companion.stop()
    }
}

actor FakeAudioBackend: AudioBackend {
    private(set) var setVolumeCalls: [Float] = []
    private(set) var setMuteCalls: [Bool] = []
    private(set) var volumeUpCount = 0
    private(set) var volumeDownCount = 0
    private(set) var muteToggleCount = 0
    private(set) var earconNames: [String] = []

    private let blockUntilCancel: Bool
    private var pending: [UUID: CheckedContinuation<Bool, Never>] = [:]

    init(blockUntilCancel: Bool = false) {
        self.blockUntilCancel = blockUntilCancel
    }

    func setVolume(_ level: Float) async { setVolumeCalls.append(level) }
    func setMute(_ muted: Bool) async { setMuteCalls.append(muted) }
    func volumeUp() async { volumeUpCount += 1 }
    func volumeDown() async { volumeDownCount += 1 }
    func muteToggle() async { muteToggleCount += 1 }

    func speak(id: UUID, text: String, voice: String?, onStart: @escaping @Sendable () -> Void) async -> Bool {
        onStart()
        if blockUntilCancel {
            return await withCheckedContinuation { (cont: CheckedContinuation<Bool, Never>) in
                pending[id] = cont
            }
        }
        return true
    }

    func cancel(id: UUID) async { pending.removeValue(forKey: id)?.resume(returning: false) }
    func cancelAll() async {
        for (_, cont) in pending { cont.resume(returning: false) }
        pending.removeAll()
    }

    func playEarcon(name: String) async -> Bool {
        earconNames.append(name)
        return false
    }
}

private func eventually(
    _ predicate: @escaping @Sendable () async -> Bool
) async throws {
    for _ in 0 ..< 300 {
        if await predicate() { return }
        try await Task.sleep(for: .milliseconds(10))
    }
    throw WireDriverError.timeout
}
