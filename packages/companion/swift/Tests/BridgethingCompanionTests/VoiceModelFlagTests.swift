import BridgethingGateway
import BridgethingSchema
import BridgethingTestKit
import XCTest

@testable import BridgethingCompanion

final class VoiceModelFlagTests: XCTestCase {
    private func announcedAvailability(voiceModel: Bool) async throws -> SurfaceAvailability {
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "voice-flag-test", appVersion: "0.0.1", osName: "macOS"),
            capabilities: CompanionCapabilityFlags(voiceModel: voiceModel)
        )
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()

        let frame = try await driver.waitOutbound(timeout: .seconds(3)) { msg in
            if case .capabilities(.announce) = msg.data { return true }
            return false
        }
        await companion.stop()
        guard case let .capabilities(.announce(caps)) = frame.data else {
            throw XCTSkip("expected capabilities announce, got \(frame.data)")
        }
        return caps.available
    }

    func testVoiceModelFlagDoesNotChangeAnnouncedSurfaces() async throws {
        let on = try await announcedAvailability(voiceModel: true)
        let off = try await announcedAvailability(voiceModel: false)
        XCTAssertEqual(on.geo, off.geo)
        XCTAssertEqual(on.notifications, off.notifications)
        XCTAssertEqual(on.netFetch, off.netFetch)
        XCTAssertEqual(on.netWs, off.netWs)
        XCTAssertEqual(on.audioTts, off.audioTts)
        XCTAssertEqual(on.lyrics, off.lyrics)
        XCTAssertEqual(on.playbackTargets, off.playbackTargets)
    }

    func testDefaultsToOnSoVoiceWorksWithoutConfiguration() {
        XCTAssertTrue(CompanionCapabilityFlags().voiceModel)
    }
}
