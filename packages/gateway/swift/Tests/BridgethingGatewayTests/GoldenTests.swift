@testable import BridgethingGateway
import BridgethingSchema
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

  /// Decode the wire frame, sanity-check the fields we share across every
  /// fixture, then re-encode and re-decode to confirm the schema round-trips
  /// without losing data. Encoded bytes are not byte-compared because msgpack
  /// named-map field ordering is implementation-defined.
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

    // Rust's serde_json::to_value emitted:
    // {"kind":"playback-changed","payload":{"playing":true,"positionMs":12345}}
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
    // #filePath: …/packages/gateway/swift/Tests/BridgethingGatewayTests/GoldenTests.swift
    // Up six levels lands at the repo root; the fixture file lives under
    // crates/lib/fixtures/.
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

  /// Pulls `meta.kind` out of the fixture's decoded_json so tests can assert
  /// the SDK's tagged-enum decode produced the expected variant.
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

/// Tiny untyped JSON wrapper used only to peek at fields for spot-checks.
/// We don't compare the SDK's struct output to decoded_json directly because
/// of the type-representation gap: fixtures encode UUID Data as strings and
/// binary Data as int arrays, but Swift's `Data` Codable defaults to base64.
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
