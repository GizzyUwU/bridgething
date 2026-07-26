import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import XCTest

@testable import BridgethingCompanion

final class ConnectResumeGateTests: XCTestCase {
    private let device = Device(id: "carthing-1", name: "Bridgething")

    private func boot(cooldown: TimeInterval = 300) async throws -> (BridgethingCompanion, InMemoryAdapter, FakeGlue) {
        let adapter = InMemoryAdapter()
        let glue = FakeGlue()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "test-companion", appVersion: "0.0.1", osName: "macOS")
        )
        await companion.setAutoResumeCooldown(cooldown)
        try await companion.attach(glue)
        try await companion.start()
        return (companion, adapter, glue)
    }

    private func peerConnects(_ glue: FakeGlue) async -> [Bool] {
        for _ in 0 ..< 100 {
            let seen = glue.calls.compactMap { call -> Bool? in
                if case let .peerConnected(allow) = call { return allow }
                return nil
            }
            if !seen.isEmpty { return seen }
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        return []
    }

    func testReconnectSoonAfterDropStillResumesOnceCooldownHasElapsed() async throws {
        let (companion, adapter, glue) = try await boot(cooldown: 0.05)

        adapter.connect(device)
        let first = await peerConnects(glue)
        XCTAssertEqual(first, [true], "first connect resumes")

        try await Task.sleep(nanoseconds: 80_000_000)
        adapter.simulate(.disconnected(deviceId: device.id))
        adapter.connect(device)

        var both = await peerConnects(glue)
        for _ in 0 ..< 50 where both.count < 2 {
            try? await Task.sleep(nanoseconds: 10_000_000)
            both = await peerConnects(glue)
        }
        XCTAssertEqual(
            both, [true, true],
            "the drop is recent but the last resume is not, so this connect must resume"
        )
        await companion.stop()
    }

    func testSecondConnectInsideCooldownDoesNotResumeAgain() async throws {
        let (companion, adapter, glue) = try await boot()

        adapter.connect(device)
        let first = await peerConnects(glue)
        XCTAssertEqual(first, [true], "first connect resumes")

        adapter.simulate(.disconnected(deviceId: device.id))
        adapter.connect(device)

        var both = await peerConnects(glue)
        for _ in 0 ..< 50 where both.count < 2 {
            try? await Task.sleep(nanoseconds: 10_000_000)
            both = await peerConnects(glue)
        }
        XCTAssertEqual(
            both, [true, false],
            "a re-dial inside the cooldown must not resume a second time"
        )
        await companion.stop()
    }

    func testDisabledDeviceNeverResumes() async throws {
        let (companion, adapter, glue) = try await boot()
        await companion.setDeviceAutoResume(deviceId: device.id, enabled: false)

        adapter.connect(device)

        let seen = await peerConnects(glue)
        XCTAssertEqual(seen, [false], "auto-resume off must veto regardless of timing")
        await companion.stop()
    }
}
