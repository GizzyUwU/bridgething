import BridgethingCompanionCore
import BridgethingCompanion
import Foundation
import XCTest

private let waitTimeout: TimeInterval = 10

private final class RecordingWsInbox: WsInbox, @unchecked Sendable {
    private let condition = NSCondition()
    private(set) var events: [String] = []

    init() {
        super.init(noHandle: .init())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("test inbox only")
    }

    override func onOpen(id: String, acceptedProtocol: String?) {
        push("open:\(id)")
    }

    override func onText(id: String, text: String) {
        push("text:\(id):\(text)")
    }

    override func onBinary(id: String, bytes: Data) {
        push("binary:\(id):\(bytes.count)")
    }

    override func onClosed(id: String, code: UInt16?, reason: String) {
        push("closed:\(id)")
    }

    private func push(_ event: String) {
        condition.lock()
        events.append(event)
        condition.broadcast()
        condition.unlock()
    }

    func waitForClosed() -> [String] {
        let deadline = Date().addingTimeInterval(waitTimeout)
        condition.lock()
        defer { condition.unlock() }
        while !events.contains(where: { $0.hasPrefix("closed:") }) {
            if !condition.wait(until: deadline) { break }
        }
        return events
    }
}

final class UrlSessionWsTransportTests: XCTestCase {
    func testAnInvalidUrlReportsClosed() {
        let inbox = RecordingWsInbox()
        UrlSessionWsTransport().connect(
            connect: WsConnect(id: "c1", url: "", protocols: [], headers: []), inbox: inbox
        )
        let events = inbox.waitForClosed()
        XCTAssertEqual(events, ["closed:c1"])
    }

    func testAConnectFailureReportsClosedExactlyOnce() {
        let port = MiniHttpServer.unusedPort()
        let inbox = RecordingWsInbox()
        let transport = UrlSessionWsTransport()
        transport.connect(
            connect: WsConnect(id: "c2", url: "ws://127.0.0.1:\(port)/ws", protocols: [], headers: []),
            inbox: inbox
        )
        let events = inbox.waitForClosed()
        XCTAssertEqual(events.filter { $0.hasPrefix("closed:") }, ["closed:c2"])
        transport.disconnect(id: "c2", code: nil, reason: nil)
        Thread.sleep(forTimeInterval: 0.2)
        XCTAssertEqual(inbox.waitForClosed().filter { $0.hasPrefix("closed:") }, ["closed:c2"])
    }

    func testANonWebsocketEndpointReportsClosed() throws {
        let server = try XCTUnwrap(MiniHttpServer { _, _, _ in
            (200, [], Data("not a websocket".utf8))
        })
        defer { server.stop() }
        let inbox = RecordingWsInbox()
        let transport = UrlSessionWsTransport()
        transport.connect(
            connect: WsConnect(id: "c3", url: "ws://127.0.0.1:\(server.port)/ws", protocols: [], headers: []),
            inbox: inbox
        )
        let events = inbox.waitForClosed()
        XCTAssertEqual(events.filter { $0.hasPrefix("closed:") }, ["closed:c3"])
        withExtendedLifetime(transport) {}
    }
}
