@testable import BridgethingGateway
import Foundation
import XCTest

final class CodecCompressionTests: XCTestCase {
  private let codec = Codec(compression: .none, encoding: .msgpack)

  private func oversizedPayload() -> String {
    String(repeating: "x", count: Codec.autoGzipPayloadThreshold + 2048)
  }

  // deterministic high-entropy bytes (xorshift), incompressible like jpeg asset bytes.
  private func incompressibleData(_ n: Int) -> Data {
    var state: UInt64 = 0x9E37_79B9_7F4A_7C15
    var out = Data(capacity: n)
    for _ in 0 ..< n {
      state ^= state << 13
      state ^= state >> 7
      state ^= state << 17
      out.append(UInt8(state & 0xFF))
    }
    return out
  }

  func testSmallFrameStaysUncompressed() throws {
    let frame = try codec.encode("hello")
    XCTAssertEqual(try FrameHeader.parse(frame).compression, .none)
  }

  func testOversizedFrameAutoGzips() throws {
    let payload = oversizedPayload()
    let frame = try codec.encode(payload)
    XCTAssertEqual(try FrameHeader.parse(frame).compression, .gzip, "a frame past one link packet is gzipped")
    XCTAssertLessThan(frame.count, payload.utf8.count, "gzip actually shrank the frame")
    XCTAssertEqual(try codec.decode(String.self, from: frame), payload)
  }

  func testOversizedIncompressibleNormalFrameStaysRaw() throws {
    // an asset (jpeg) on the normal lane via a pull-on-miss response must not be gzip-bloated.
    let blob = incompressibleData(Codec.autoGzipPayloadThreshold + 4096)
    let frame = try codec.encode(blob)
    XCTAssertEqual(try FrameHeader.parse(frame).compression, .none, "incompressible payload stays raw")
    XCTAssertEqual(try codec.decode(Data.self, from: frame), blob)
  }

  func testOversizedBulkFrameStaysRaw() throws {
    // the bulk lane carries already-compressed asset bytes; never auto-gzip it.
    let frame = try codec.encode(oversizedPayload(), priority: .bulk)
    XCTAssertEqual(try FrameHeader.parse(frame).compression, .none)
  }

  func testExplicitCompressionOverridesAutoGzip() throws {
    // `Compression.none` spelled out: a bare `.none` would bind to `Optional.none` (nil)
    // and fall through to auto-gzip.
    let frame = try codec.encode(oversizedPayload(), compression: Compression.none)
    XCTAssertEqual(try FrameHeader.parse(frame).compression, .none)
  }
}
