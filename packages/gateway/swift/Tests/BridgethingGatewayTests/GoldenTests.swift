@testable import BridgethingGateway
import BridgethingSchema
import MessagePack
import XCTest

private let fixedID = UUID(uuidString: "0192f2a0-bbb0-7c00-a000-000000000001")!
private let fixedRequestID = UUID(uuidString: "0192f2a0-bbb0-7c00-a000-000000000099")!

final class GoldenTests: XCTestCase {
  func testGoldenFixturesDecodeAndRoundTrip() throws {
    let goldens = try loadGoldens()
    XCTAssertGreaterThan(goldens.fixtures.count, 0, "expected fixtures in golden.json")

    let codec = Codec(compression: .none, encoding: .msgpack)

    for fixture in goldens.fixtures {
      try checkFixture(fixture, codec: codec)
    }
  }

  private func checkFixture(_ fixture: GoldenFixture, codec: Codec) throws {
    let frame = Data(hex: fixture.framedHex)

    let header = try FrameHeader.parse(frame)
    XCTAssertEqual(header.priority, fixture.priority, "priority mismatch on \(fixture.name)")

    switch fixture.direction {
    case .bridgeToGateway:
      let decoded = try codec.decode(BridgeToGatewayMsg.self, from: frame)
      XCTAssertEqual(decoded.id, fixedID, "id mismatch on \(fixture.name)")
      try assertMetaMatches(decoded.meta, fixture: fixture)

      let reEncoded = try codec.encode(decoded, priority: fixture.priority)
      let reHeader = try FrameHeader.parse(reEncoded)
      XCTAssertEqual(reHeader.priority, fixture.priority, "round-trip priority changed on \(fixture.name)")
      let reDecoded = try codec.decode(BridgeToGatewayMsg.self, from: reEncoded)
      XCTAssertEqual(reDecoded.id, decoded.id, "round-trip id changed on \(fixture.name)")

    case .gatewayToBridge:
      let decoded = try codec.decode(GatewayToBridgeMsg.self, from: frame)
      XCTAssertEqual(decoded.id, fixedID, "id mismatch on \(fixture.name)")
      try assertMetaMatches(decoded.meta, fixture: fixture)

      let reEncoded = try codec.encode(decoded, priority: fixture.priority)
      let reHeader = try FrameHeader.parse(reEncoded)
      XCTAssertEqual(reHeader.priority, fixture.priority, "round-trip priority changed on \(fixture.name)")
      let reDecoded = try codec.decode(GatewayToBridgeMsg.self, from: reEncoded)
      XCTAssertEqual(reDecoded.id, decoded.id, "round-trip id changed on \(fixture.name)")
    }
  }

  private func assertMetaMatches(_ meta: MsgMeta, fixture: GoldenFixture) throws {
    let expectedKind = try fixture.metaKind()
    switch (meta, expectedKind) {
    case (.command, "command"), (.event, "event"), (.request, "request"):
      break
    case let (.response(payload), "response"):
      XCTAssertEqual(
        payload.requestId, fixedRequestID,
        "requestId mismatch on \(fixture.name)"
      )
    default:
      XCTFail("meta variant \(meta) didn't match fixture-declared kind \(expectedKind) on \(fixture.name)")
    }
  }

  func testForwardTextDecodesToExpectedString() throws {
    let goldens = try loadGoldens()
    let fixture = try XCTUnwrap(goldens.fixtures.first { $0.name == "bridge_to_gateway/forward-text-event" })
    let codec = Codec(compression: .none, encoding: .msgpack)
    let msg = try codec.decode(BridgeToGatewayMsg.self, from: Data(hex: fixture.framedHex))
    guard case let .forward(.text(text)) = msg.data else {
      XCTFail("expected .forward(.text), got \(msg.data)"); return
    }
    XCTAssertEqual(text, "hello, gateway")
  }

  func testForwardJsonRoundTripsArbitraryJSONOverMsgpack() throws {
    let goldens = try loadGoldens()
    let fixture = try XCTUnwrap(goldens.fixtures.first { $0.name == "bridge_to_gateway/forward-json-event" })
    let codec = Codec(compression: .none, encoding: .msgpack)
    let msg = try codec.decode(BridgeToGatewayMsg.self, from: Data(hex: fixture.framedHex))
    guard case let .forward(.json(value)) = msg.data else {
      XCTFail("expected .forward(.json), got \(msg.data)"); return
    }

    guard case let .object(dict) = value else { XCTFail("expected object, got \(value)"); return }
    XCTAssertEqual(dict["kind"], .string("playback-changed"))
    guard case let .object(payload) = try XCTUnwrap(dict["payload"]) else {
      XCTFail("expected payload object"); return
    }
    XCTAssertEqual(payload["playing"], .bool(true))
    XCTAssertEqual(payload["positionMs"], .int(12345))

    let reEncoded = try codec.encode(msg)
    let reDecoded = try codec.decode(BridgeToGatewayMsg.self, from: reEncoded)
    guard case let .forward(.json(reValue)) = reDecoded.data else {
      XCTFail("expected .forward(.json) after round-trip"); return
    }
    XCTAssertEqual(value, reValue, "JSON value drifted on msgpack round-trip")
  }

  func testForwardBinaryDecodesToExpectedBytes() throws {
    let goldens = try loadGoldens()
    let fixture = try XCTUnwrap(goldens.fixtures.first { $0.name == "bridge_to_gateway/forward-binary-event" })
    let codec = Codec(compression: .none, encoding: .msgpack)
    let msg = try codec.decode(BridgeToGatewayMsg.self, from: Data(hex: fixture.framedHex))
    guard case let .forward(.binary(bytes)) = msg.data else {
      XCTFail("expected .forward(.binary), got \(msg.data)"); return
    }
    XCTAssertEqual(bytes, Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]))
  }

  private static let webappInfoFrameMissingNewerFieldHex = """
  dead02000000000000000000000000f283a26964c4100192f2a0bbb07c00a000000000000001a46d65746181a46b696e64\
  a56576656e74a46461746182a474797065a6776562617070a46461746182a56576656e74af776562617070496e7374616c\
  6c6564a46461746188a26964c4100192f2a0bbb07c00a000000000000101a46e616d65a444656d6fa6736f75726365a969\
  6e7374616c6c6564a4726f6c65a87374616e64617264a776657273696f6ea5302e312e30a6636f6e66696790ab7065726d\
  697373696f6e7390aa70726f76656e616e6365d92968747470733a2f2f617070732e6272696467657468696e672e636f6d\
  2f636174616c6f672e6a736f6e
  """

  func testFrameFromADaemonPredatingAFieldDecodesToTheRustDefault() throws {
    let codec = Codec(compression: .none, encoding: .msgpack)
    let frame = Data(hex: Self.webappInfoFrameMissingNewerFieldHex.replacingOccurrences(of: "\n", with: ""))

    let msg = try codec.decode(BridgeToGatewayMsg.self, from: frame)
    guard case let .webapp(.webappInstalled(info)) = msg.data else {
      XCTFail("expected .webapp(.webappInstalled), got \(msg.data)"); return
    }
    XCTAssertEqual(info.name, "Demo")
    XCTAssertEqual(info.role, .standard)
    XCTAssertFalse(info.rendersVoiceDisplay, "an absent defaulted field must fall back, not fail the frame")
  }

  func testAnAbsentKeyTakesANonFalseDefault() throws {
    let profile = try MessagePackDecoder().decode(OverlayProfile.self, from: Data([0x80]))
    XCTAssertTrue(profile.notifications)
    XCTAssertTrue(profile.call)
    XCTAssertTrue(profile.pairing)
    XCTAssertTrue(profile.connection)
    XCTAssertTrue(profile.volume)
  }

  func testGzipFrameRoundTrip() throws {
    let codec = Codec(compression: .gzip, encoding: .msgpack)
    let original = BridgeToGatewayMsg(
      id: fixedID,
      meta: .response(ResponseMeta(requestId: fixedRequestID)),
      data: .ack
    )
    let frame = try codec.encode(original)
    let decoded = try codec.decode(BridgeToGatewayMsg.self, from: frame)
    XCTAssertEqual(decoded.id, original.id)
    if case let .response(r) = decoded.meta {
      XCTAssertEqual(r.requestId, fixedRequestID)
    } else {
      XCTFail("expected .response meta, got \(decoded.meta)")
    }
  }

  // MARK: - fixture loading

  private func loadGoldens() throws -> GoldenFile {
    let url = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent() // BridgethingGatewayTests
      .deletingLastPathComponent() // Tests
      .deletingLastPathComponent() // swift
      .deletingLastPathComponent() // gateway
      .deletingLastPathComponent() // packages
      .deletingLastPathComponent() // repo root
      .appendingPathComponent("crates/lib/fixtures/golden.json")
      .standardizedFileURL
    let data = try Data(contentsOf: url)
    return try JSONDecoder().decode(GoldenFile.self, from: data)
  }
}

private struct GoldenFile: Decodable {
  let version: UInt8
  let magic: String
  let fixtures: [GoldenFixture]
}

private struct GoldenFixture: Decodable {
  let name: String
  let description: String
  let direction: Direction
  let priority: Priority
  let decodedJson: AnyCodable
  let msgpackHex: String
  let framedHex: String

  enum CodingKeys: String, CodingKey {
    case name, description, direction, priority
    case decodedJson = "decoded_json"
    case msgpackHex = "msgpack_hex"
    case framedHex = "framed_hex"
  }

  func metaKind() throws -> String {
    guard
      let dict = decodedJson.value as? [String: Any],
      let meta = dict["meta"] as? [String: Any],
      let kind = meta["kind"] as? String
    else {
      throw NSError(domain: "GoldenFixture", code: 1, userInfo: [
        NSLocalizedDescriptionKey: "decoded_json missing meta.kind on \(name)",
      ])
    }
    return kind
  }
}

private enum Direction: String, Decodable {
  case bridgeToGateway = "bridge_to_gateway"
  case gatewayToBridge = "gateway_to_bridge"
}

private struct AnyCodable: Decodable {
  let value: Any

  init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    if let v = try? container.decode([String: AnyCodable].self) {
      value = v.mapValues { $0.value }
    } else if let v = try? container.decode([AnyCodable].self) {
      value = v.map(\.value)
    } else if let v = try? container.decode(String.self) {
      value = v
    } else if let v = try? container.decode(Int.self) {
      value = v
    } else if let v = try? container.decode(Double.self) {
      value = v
    } else if let v = try? container.decode(Bool.self) {
      value = v
    } else if container.decodeNil() {
      value = NSNull()
    } else {
      throw DecodingError.dataCorruptedError(
        in: container, debugDescription: "unsupported JSON value"
      )
    }
  }
}

private extension Data {
  init(hex: String) {
    var data = Data()
    var idx = hex.startIndex
    while idx < hex.endIndex {
      let next = hex.index(idx, offsetBy: 2, limitedBy: hex.endIndex) ?? hex.endIndex
      if let byte = UInt8(hex[idx ..< next], radix: 16) {
        data.append(byte)
      }
      idx = next
    }
    self = data
  }
}
