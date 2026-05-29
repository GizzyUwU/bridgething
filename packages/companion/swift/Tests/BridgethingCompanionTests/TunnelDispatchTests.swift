import BridgethingGateway
import BridgethingSchema
import BridgethingTestKit
import Foundation
import XCTest

@testable import BridgethingCompanion

#if canImport(Network)
    import Network

    final class TunnelDispatchTests: XCTestCase {
        private func boot() async throws -> (BridgethingCompanion, WireDriver) {
            let adapter = InMemoryAdapter()
            let companion = BridgethingCompanion(
                adapter: adapter,
                lyricsResolver: FakeLyricsResolver(),
                host: HostInfo(appName: "tunnel-test", appVersion: "0.0.1", osName: "macOS")
            )
            try await companion.start()
            let driver = WireDriver(adapter: adapter)
            await driver.start()
            driver.connect()
            return (companion, driver)
        }

        func testTunnelOpenEchoClose() async throws {
            let echo = try EchoServer()
            let port = try await echo.start()
            defer { echo.stop() }

            let (companion, driver) = try await boot()
            let id = UUID()

            let openResp = try await driver.request(
                .tunnel(.open(TunnelOpen(tunnelId: id, host: "127.0.0.1", port: port))),
                timeout: .seconds(5)
            )
            guard case .tunnel(.openReply) = openResp.data else {
                await companion.stop()
                return XCTFail("expected tunnel openReply, got \(openResp.data)")
            }

            let payload = Data("ping-through-the-phone".utf8)
            try await driver.send(.tunnel(.data(TunnelData(tunnelId: id, bytes: payload))))

            let echoed = try await driver.waitOutbound(timeout: .seconds(5)) { msg in
                if case .tunnel(.data) = msg.data { return true }
                return false
            }
            guard case let .tunnel(.data(td)) = echoed.data else {
                await companion.stop()
                return XCTFail("expected outbound tunnel data, got \(echoed.data)")
            }
            XCTAssertEqual(td.tunnelId, id)
            XCTAssertEqual(td.bytes, payload)

            try await driver.send(.tunnel(.close(TunnelClosed(tunnelId: id, reason: nil))))
            await companion.stop()
        }

        func testTunnelOpenToClosedPortReturnsError() async throws {
            let (companion, driver) = try await boot()
            let resp = try await driver.request(
                .tunnel(.open(TunnelOpen(tunnelId: UUID(), host: "127.0.0.1", port: 1))),
                timeout: .seconds(5)
            )
            guard case .tunnel(.errorReply) = resp.data else {
                await companion.stop()
                return XCTFail("expected tunnel errorReply for refused connect, got \(resp.data)")
            }
            await companion.stop()
        }
    }

    private final class EchoServer: @unchecked Sendable {
        private let listener: NWListener
        private let queue = DispatchQueue(label: "dev.bridgething.test.echo")

        init() throws {
            listener = try NWListener(using: .tcp)
            listener.newConnectionHandler = { conn in
                conn.start(queue: DispatchQueue(label: "dev.bridgething.test.echo.conn"))
                func pump() {
                    conn.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { data, _, isComplete, error in
                        if let data, !data.isEmpty {
                            conn.send(content: data, completion: .contentProcessed { _ in })
                        }
                        if isComplete || error != nil { conn.cancel() } else { pump() }
                    }
                }
                pump()
            }
        }

        /// polls listener.port rather than bridging the callback-only state handler into async code.
        func start() async throws -> UInt16 {
            listener.start(queue: queue)
            for _ in 0 ..< 300 {
                if let port = listener.port?.rawValue, port != 0 { return port }
                try await Task.sleep(for: .milliseconds(10))
            }
            throw WireDriverError.timeout
        }

        func stop() {
            listener.cancel()
        }
    }
#endif
