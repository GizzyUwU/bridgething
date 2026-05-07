@testable import BridgethingGateway
import BridgethingSchema
import XCTest

final class FrameAccumulatorTests: XCTestCase {
  private let codec = Codec(compression: .none, encoding: .msgpack)

  func testNilWhenEmpty() throws {
    var acc = FrameAccumulator()
    XCTAssertNil(try acc.nextFrame())
  }

  func testNilWhenHeaderIncomplete() throws {
    var acc = FrameAccumulator()
    acc.append(Data([0xDE, 0xAD, 0x02, 0x00, 0x00]))
    XCTAssertNil(try acc.nextFrame())
  }

  func testNilWhenPayloadIncomplete() throws {
    let frame = try makeAckFrame()
    var acc = FrameAccumulator()
    acc.append(frame.prefix(frame.count - 1))
    XCTAssertNil(try acc.nextFrame())
  }

  func testEmitsCompleteFrame() throws {
    let frame = try makeAckFrame()
    var acc = FrameAccumulator()
    acc.append(frame)
    XCTAssertEqual(try acc.nextFrame(), frame)
    XCTAssertNil(try acc.nextFrame())
  }

  func testEmitsMultipleFramesInSingleChunk() throws {
    let f1 = try makeAckFrame()
    let f2 = try makeAckFrame()
    var acc = FrameAccumulator()
    acc.append(f1 + f2)
    XCTAssertEqual(try acc.nextFrame(), f1)
    XCTAssertEqual(try acc.nextFrame(), f2)
    XCTAssertNil(try acc.nextFrame())
  }

  func testReassemblesByteAtATime() throws {
    let frame = try makeAckFrame()
    var acc = FrameAccumulator()
    for byte in frame {
      acc.append(Data([byte]))
    }
    XCTAssertEqual(try acc.nextFrame(), frame)
  }

  func testKeepsTrailingPartialFrame() throws {
    let f1 = try makeAckFrame()
    let f2 = try makeAckFrame()
    var acc = FrameAccumulator()
    // First full frame plus the first 5 bytes of the next - accumulator should
    // pop the first and hold the rest.
    acc.append(f1 + f2.prefix(5))
    XCTAssertEqual(try acc.nextFrame(), f1)
    XCTAssertNil(try acc.nextFrame())
    acc.append(f2.suffix(from: 5))
    XCTAssertEqual(try acc.nextFrame(), f2)
  }

  func testThrowsOnBadMagic() throws {
    var acc = FrameAccumulator()
    var bytes = Data(repeating: 0, count: FrameHeader.length)
    bytes[0] = 0xBA
    bytes[1] = 0xAD
    acc.append(bytes)
    XCTAssertThrowsError(try acc.nextFrame()) { err in
      guard case let FrameAccumulator.Error.invalidMagic(m) = err else {
        XCTFail("expected invalidMagic, got \(err)"); return
      }
      XCTAssertEqual(m, 0xBAAD)
    }
  }

  func testThrowsOnUnsupportedVersion() throws {
    var acc = FrameAccumulator()
    var bytes = Data(repeating: 0, count: FrameHeader.length)
    bytes[0] = 0xDE; bytes[1] = 0xAD
    bytes[2] = 99
    acc.append(bytes)
    XCTAssertThrowsError(try acc.nextFrame()) { err in
      guard case FrameAccumulator.Error.unsupportedVersion(99) = err else {
        XCTFail("expected unsupportedVersion(99), got \(err)"); return
      }
    }
  }

  func testThrowsOnOversizedPayload() throws {
    var acc = FrameAccumulator(maxPayloadSize: 1024)
    var bytes = Data(repeating: 0, count: FrameHeader.length)
    bytes[0] = 0xDE; bytes[1] = 0xAD
    bytes[2] = FrameHeader.version
    // length = 1 MiB - over the cap.
    let big: UInt64 = 1 << 20
    for i in 0 ..< 8 {
      bytes[8 + i] = UInt8((big >> ((7 - i) * 8)) & 0xFF)
    }
    acc.append(bytes)
    XCTAssertThrowsError(try acc.nextFrame()) { err in
      guard case FrameAccumulator.Error.payloadTooLarge(big, max: 1024) = err else {
        XCTFail("expected payloadTooLarge, got \(err)"); return
      }
    }
  }

  // MARK: - helpers

  private func makeAckFrame() throws -> Data {
    let msg = BridgeToGatewayMsg(
      id: UUID(),
      meta: .command,
      data: .ack
    )
    return try codec.encode(msg)
  }
}
