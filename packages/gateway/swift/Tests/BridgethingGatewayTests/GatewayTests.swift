import XCTest
import BridgethingSchema
@testable import BridgethingGateway

final class GatewayTests: XCTestCase {
  private let codec = Codec(compression: .none, encoding: .msgpack)
  private let testDevice = Device(id: "carthing-1", name: "Car Thing")

  func testForwardsConnectAndDisconnectEvents() async throws {
    let adapter = MockAdapter()
    let gateway = BridgethingGateway(adapter: adapter, codec: codec)
    try await gateway.start()

    adapter.simulate(.connected(testDevice))
    adapter.simulate(.disconnected(deviceId: testDevice.id))

    var iter = gateway.events.makeAsyncIterator()
    guard case .connected(let device) = await iter.next() else {
      XCTFail("expected .connected"); return
    }
    XCTAssertEqual(device, testDevice)

    guard case .disconnected(let id) = await iter.next() else {
      XCTFail("expected .disconnected"); return
    }
    XCTAssertEqual(id, testDevice.id)

    await gateway.stop()
  }

  func testDecodesIncomingFramesIntoMessages() async throws {
    let adapter = MockAdapter()
    let gateway = BridgethingGateway(adapter: adapter, codec: codec)
    try await gateway.start()

    adapter.simulate(.connected(testDevice))

    let original = BridgeToGatewayMsg(
      id: UUID(),
      meta: .event,
      data: .forward(.text("hello, gateway"))
    )
    let frame = try codec.encode(original)
    adapter.simulate(.bytes(deviceId: testDevice.id, frame))

    var iter = gateway.events.makeAsyncIterator()
    _ = await iter.next() // consume the .connected we already asserted in the prior test
    guard case .message(let id, let msg) = await iter.next() else {
      XCTFail("expected .message"); return
    }
    XCTAssertEqual(id, testDevice.id)
    XCTAssertEqual(msg.id, original.id)
    if case .forward(.text(let text)) = msg.data {
      XCTAssertEqual(text, "hello, gateway")
    } else {
      XCTFail("unexpected msg.data: \(msg.data)")
    }

    await gateway.stop()
  }

  func testReassemblesFramesAcrossChunks() async throws {
    let adapter = MockAdapter()
    let gateway = BridgethingGateway(adapter: adapter, codec: codec)
    try await gateway.start()
    adapter.simulate(.connected(testDevice))

    let original = BridgeToGatewayMsg(
      id: UUID(),
      meta: .command,
      data: .ack
    )
    let frame = try codec.encode(original)
    let mid = frame.count / 2
    adapter.simulate(.bytes(deviceId: testDevice.id, frame.prefix(mid)))
    adapter.simulate(.bytes(deviceId: testDevice.id, frame.suffix(from: mid)))

    var iter = gateway.events.makeAsyncIterator()
    _ = await iter.next() // .connected
    guard case .message(_, let msg) = await iter.next() else {
      XCTFail("expected .message after reassembly"); return
    }
    XCTAssertEqual(msg.id, original.id)

    await gateway.stop()
  }

  func testSendEncodesAndForwardsToAdapter() async throws {
    let adapter = MockAdapter()
    let gateway = BridgethingGateway(adapter: adapter, codec: codec)
    try await gateway.start()

    let outbound = GatewayToBridgeMsg(
      id: UUID(),
      meta: .command,
      data: .webapp(.list)
    )
    try await gateway.send(deviceId: testDevice.id, outbound)

    var iter = adapter.sentFrames.makeAsyncIterator()
    guard let sent = await iter.next() else {
      XCTFail("expected a sent frame"); return
    }
    XCTAssertEqual(sent.deviceId, testDevice.id)
    let decoded = try codec.decode(GatewayToBridgeMsg.self, from: sent.frame)
    XCTAssertEqual(decoded.id, outbound.id)

    await gateway.stop()
  }

  func testRequestResponseCorrelation() async throws {
    let adapter = MockAdapter()
    let gateway = BridgethingGateway(adapter: adapter, codec: codec)
    try await gateway.start()
    adapter.simulate(.connected(testDevice))

    let codec = self.codec
    let deviceId = testDevice.id

    let requestTask = Task {
      try await gateway.request(deviceId: deviceId, .webapp(.list))
    }

    var sentIter = adapter.sentFrames.makeAsyncIterator()
    guard let sent = await sentIter.next() else {
      XCTFail("expected a sent frame"); return
    }
    let request = try codec.decode(GatewayToBridgeMsg.self, from: sent.frame)
    if case .request = request.meta { } else {
      XCTFail("expected request meta on outbound frame, got \(request.meta)")
    }

    let response = BridgeToGatewayMsg(
      id: UUID(),
      meta: .response(ResponseMeta(requestId: request.id)),
      data: .ack
    )
    adapter.simulate(.bytes(deviceId: deviceId, try codec.encode(response)))

    let result = try await requestTask.value
    guard case .response(let r) = result.meta else {
      XCTFail("expected .response meta on result"); return
    }
    XCTAssertEqual(r.requestId, request.id)

    await gateway.stop()
  }

  func testRequestTimeoutFiresWhenNoResponseArrives() async throws {
    let adapter = MockAdapter()
    let gateway = BridgethingGateway(adapter: adapter, codec: codec)
    try await gateway.start()
    adapter.simulate(.connected(testDevice))

    do {
      _ = try await gateway.request(
        deviceId: testDevice.id,
        .webapp(.list),
        timeout: .milliseconds(100)
      )
      XCTFail("expected timeout error")
    } catch let error as BridgethingGatewayError {
      XCTAssertEqual(error, .requestTimedOut)
    }

    await gateway.stop()
  }
}

extension BridgethingGatewayError: Equatable {
  public static func == (lhs: BridgethingGatewayError, rhs: BridgethingGatewayError) -> Bool {
    switch (lhs, rhs) {
    case (.notRunning, .notRunning),
         (.alreadyRunning, .alreadyRunning),
         (.requestTimedOut, .requestTimedOut),
         (.shutdown, .shutdown):
      return true
    case (.unexpectedResponse(let a), .unexpectedResponse(let b)):
      return a == b
    default:
      return false
    }
  }
}
